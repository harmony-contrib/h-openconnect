/**
 * VPN Extension — paws create order + AnyConnect prepare for real addresses.
 *
 *   1) createVpnConnection + register per-fd protect
 *   2) prepareVpnInExtension (CSTP / network_config) on physical net
 *   3) create(VpnConfig) — paws-shaped config (see VpnConfig.ets)
 *   4) protectProcessNet
 *   5) startVpn(fd) OpenConnect mainloop
 *
 * No generateVpnId / multitun. Classic vpn-tun only (paws).
 */
import { Want } from '@kit.AbilityKit';
import { vpnExtension, VpnExtensionAbility } from '@kit.NetworkKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { configureNativePlatformIdentity } from '../common/PlatformIdentity';
import {
  buildVpnConfig,
  parseOptions,
  readPlatformSharedMemoryFds,
  readPlatformStartAttemptId,
  readOptionsJson,
  setHOpenBundleName,
} from './VpnConfig';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectVpn';
const PREPARE_TIMEOUT_MS: number = 120000;
const PLATFORM_OPERATION_TIMEOUT_MS: number = 30000;

function withTimeout<T>(operation: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return new Promise<T>((resolve: (value: T) => void, reject: (reason?: Object | string) => void): void => {
    let settled: boolean = false;
    const timer: number = setTimeout((): void => {
      if (!settled) {
        settled = true;
        reject(new Error(`${label} timed out after ${timeoutMs}ms`));
      }
    }, timeoutMs);
    operation.then((value: T): void => {
      if (!settled) {
        settled = true;
        clearTimeout(timer);
        resolve(value);
      }
    }).catch((err: Error): void => {
      if (!settled) {
        settled = true;
        clearTimeout(timer);
        reject(err);
      }
    });
  });
}

function describeError(err: Error | Object | string | number | boolean | null | undefined): string {
  if (err instanceof Error) {
    return `${err.name}: ${err.message}`;
  }
  if (typeof err === 'string') {
    return err;
  }
  if (typeof err === 'number' || typeof err === 'boolean') {
    return err.toString();
  }
  if (!err) {
    return '';
  }
  const json = JSON.stringify(err);
  return json && json !== '{}' ? json : err.toString();
}

export default class HOpenConnectVpnExtensionAbility extends VpnExtensionAbility {
  private vpnConnection?: vpnExtension.VpnConnection;
  private tunFd: number = -1;
  private starting: boolean = false;
  private platformSubscriptionActive: boolean = false;
  private platformSubscriptionGeneration: number = 0;
  private requestGeneration: number = 0;
  private activeAttemptId: string = '';
  private heartbeatTimer: number = -1;
  private requestChain: Promise<void> = Promise.resolve();
  private pendingPlatformOperation?: Promise<void>;
  private pendingPlatformOperationConnection?: vpnExtension.VpnConnection;
  private pendingCleanup?: Promise<string>;
  private pendingCleanupConnection?: vpnExtension.VpnConnection;

  public onCreate(want: Want): void {
    super.onCreate(want);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'onCreate');
    this.startFromWant(want);
  }

  public onRequest(want: Want, startId: number): void {
    hilog.info(LOG_DOMAIN, LOG_TAG, 'onRequest %{public}d', startId);
    this.startFromWant(want);
  }

  public onDestroy(): void {
    hilog.info(LOG_DOMAIN, LOG_TAG, 'onDestroy');
    this.requestGeneration += 1;
    this.stopHeartbeat();
    this.stopPlatformSubscription();
    try {
      hopenconnectUi.setPlatformVpnRunning(false);
    } catch (_err) {
      // ignore
    }
    this.destroyVpn().catch((err: Error): void => {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'onDestroy cleanup failed %{public}s', describeError(err));
    });
  }

  private configureNativeHome(): void {
    try {
      setHOpenBundleName(this.context.applicationInfo.name);
      const home = `${this.context.filesDir}/h-openconnect`;
      if (typeof (hopenconnectUi as ESObject).configureAppHomeForExtension === 'function') {
        (hopenconnectUi as ESObject).configureAppHomeForExtension(home);
      } else {
        hopenconnectUi.configureAppHome(home);
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native home (extension)');
    } catch (err) {
      const message = describeError(err) || 'configure native home failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'configure native home failed %{private}s', message);
      throw new Error(message);
    }
  }

  private registerIcsSocketProtect(connection: vpnExtension.VpnConnection): void {
    if (typeof (hopenconnectUi as ESObject).registerSocketProtect !== 'function') {
      throw new Error('native socket protect registration is unavailable');
    }
    (hopenconnectUi as ESObject).registerSocketProtect({
      protectSocket: async (fd: number): Promise<void> => {
        if (fd < 0 || this.vpnConnection !== connection) {
          throw new Error('VpnConnection is unavailable');
        }
        try {
          await connection.protect(fd);
          hilog.info(LOG_DOMAIN, LOG_TAG, 'protect(fd) ok fd=%{public}d', fd);
        } catch (err) {
          const message = describeError(err);
          hilog.error(LOG_DOMAIN, LOG_TAG, 'protect(fd) failed fd=%{public}d %{public}s',
            fd, message);
          throw new Error(message || 'protect(fd) failed');
        }
      },
    });
    hilog.info(LOG_DOMAIN, LOG_TAG, 'ics socket protect registered');
  }

  private startFromWant(want: Want): void {
    try {
      configureNativePlatformIdentity(this.context, hopenconnectUi);
      this.configureNativeHome();
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN request rejected %{public}s', describeError(err));
      return;
    }
    const sharedMemory = readPlatformSharedMemoryFds(want);
    if (!sharedMemory) {
      // The first-authorization system dialog may bootstrap the Extension with
      // a replacement Want that contains no application parameters. Keep the
      // Extension idle until EntryAbility redispatches the original Want via
      // onRequest; it owns the bounded failure if that rebound never arrives.
      hilog.warn(
        LOG_DOMAIN,
        LOG_TAG,
        'VPN authorization bootstrap has no shared-memory descriptors; waiting for rebound request',
      );
      return;
    }
    let attemptId: string;
    let optionsJson: string;
    try {
      attemptId = readPlatformStartAttemptId(want);
      optionsJson = readOptionsJson(want);
    } catch (err) {
      const message = describeError(err) || 'VPN request has invalid parameters';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN request rejected %{public}s', message);
      return;
    }
    if (!this.isLatestWant(
      sharedMemory.ashmemFd,
      sharedMemory.notificationFd,
      attemptId,
    )) {
      return;
    }
    const generation = ++this.requestGeneration;
    this.requestChain = this.requestChain.then(async (): Promise<void> => {
      await this.handleRequest(
        sharedMemory.ashmemFd,
        sharedMemory.notificationFd,
        attemptId,
        optionsJson,
        generation,
      );
    }).catch(async (err: Error): Promise<void> => {
      if (!this.isCurrentRequest(generation, attemptId)) {
        hilog.info(LOG_DOMAIN, LOG_TAG, 'ignored stale VPN request completion');
        return;
      }
      const message = describeError(err) || 'vpn start failed';
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN start failed %{public}s', message);
      await this.destroyVpn();
    });
  }

  private async handleRequest(
    ashmemFd: number,
    notificationFd: number,
    attemptId: string,
    optionsJson: string,
    generation: number,
  ): Promise<void> {
    if (generation !== this.requestGeneration) {
      return;
    }
    await this.waitForPendingCleanup();
    if (generation !== this.requestGeneration) {
      return;
    }
    // The UI can supersede a queued request while an older cleanup barrier is
    // settling. Revalidate immediately before any destroy or IPC replacement.
    if (!this.isLatestWant(ashmemFd, notificationFd, attemptId)) {
      return;
    }
    if (this.activeAttemptId.length > 0 && this.activeAttemptId !== attemptId) {
      await this.destroyVpn();
      if (generation !== this.requestGeneration) {
        return;
      }
    }

    // A reused extension may still have an event waiter parked on the previous
    // UI process's notification socket. Stop it before replacing the IPC pair.
    this.stopPlatformSubscription();
    hopenconnectUi.attachPlatformSharedMemory(ashmemFd, notificationFd);
    hopenconnectUi.bindPlatformVpnStart(attemptId);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'bound VPN start attempt %{public}s', attemptId);
    this.startPlatformSubscription();
    this.activeAttemptId = attemptId;

    if (this.tunFd >= 0 && this.vpnConnection) {
      const lifecycle = this.extensionTick();
      if (lifecycle === 'connected') {
        hopenconnectUi.setPlatformVpnRunning(true);
        this.startHeartbeat(generation, attemptId);
        hilog.info(LOG_DOMAIN, LOG_TAG, 'healthy VPN rebound to shared memory');
        return;
      }
      await this.destroyVpn();
      if (generation !== this.requestGeneration) {
        return;
      }
      this.stopPlatformSubscription();
      hopenconnectUi.attachPlatformSharedMemory(ashmemFd, notificationFd);
      hopenconnectUi.bindPlatformVpnStart(attemptId);
      this.startPlatformSubscription();
      this.activeAttemptId = attemptId;
    }

    this.starting = true;
    hilog.info(LOG_DOMAIN, LOG_TAG, 'optionsJson length=%{public}d', optionsJson.length);
    try {
      await this.performStart(optionsJson, attemptId, generation);
    } finally {
      if (this.isCurrentRequest(generation, attemptId)) {
        this.starting = false;
      }
    }
  }

  private isCurrentRequest(generation: number, attemptId: string): boolean {
    return generation === this.requestGeneration && attemptId === this.activeAttemptId;
  }

  private isLatestWant(ashmemFd: number, notificationFd: number, attemptId: string): boolean {
    try {
      hopenconnectUi.validatePlatformVpnStartRequest(ashmemFd, notificationFd, attemptId);
      return true;
    } catch (err) {
      hilog.warn(
        LOG_DOMAIN,
        LOG_TAG,
        'ignored stale VPN request %{public}s %{public}s',
        attemptId,
        describeError(err),
      );
      return false;
    }
  }

  private requireCurrentRequest(
    generation: number,
    attemptId: string,
    connection?: vpnExtension.VpnConnection,
  ): void {
    if (!this.isCurrentRequest(generation, attemptId) ||
      (connection !== undefined && this.vpnConnection !== connection)) {
      throw new Error('stale VPN request generation');
    }
  }

  private startPlatformSubscription(): void {
    if (this.platformSubscriptionActive) {
      return;
    }
    this.platformSubscriptionActive = true;
    const generation = ++this.platformSubscriptionGeneration;
    this.runPlatformSubscription(generation);
  }

  private stopPlatformSubscription(): void {
    this.platformSubscriptionActive = false;
    this.platformSubscriptionGeneration += 1;
    // Wake the waiter parked in waitForPlatformChangeEvent so the loop can
    // observe the flag and unwind without waiting for the next peer frame.
    try {
      hopenconnectUi.cancelPlatformChangeWait();
    } catch (err) {
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'cancel platform change wait failed %{public}s', describeError(err));
    }
  }

  private async runPlatformSubscription(generation: number): Promise<void> {
    while (this.platformSubscriptionActive && generation === this.platformSubscriptionGeneration) {
      try {
        // Fully event driven: the native wait parks on the shared-memory
        // notification socket until the UI publishes a control frame (or the
        // wait is cancelled by stopPlatformSubscription). No polling.
        const changed = await hopenconnectUi.waitForPlatformChangeEvent();
        if (this.platformSubscriptionActive && generation === this.platformSubscriptionGeneration) {
          hopenconnectUi.syncPlatformChanges();
          this.reconcileExtensionTerminal();
          if (!changed) {
            // Cancellation/no-peer returns false immediately. Back off so a
            // disappearing UI process cannot turn this into a hot loop.
            await new Promise<void>((resolve: () => void): void => {
              setTimeout(resolve, 250);
            });
          }
        }
      } catch (err) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'platform subscription failed %{public}s', describeError(err));
        if (generation === this.platformSubscriptionGeneration) {
          this.platformSubscriptionActive = false;
        }
      }
    }
  }

  private async performStart(
    optionsJson: string,
    attemptId: string,
    generation: number,
  ): Promise<void> {
    this.requireCurrentRequest(generation, attemptId);
    await this.waitForPendingCleanup();
    this.requireCurrentRequest(generation, attemptId);
    hopenconnectUi.setPlatformVpnStarting(true);

    const connection = vpnExtension.createVpnConnection(this.context);
    this.vpnConnection = connection;
    this.registerIcsSocketProtect(connection);

    // 1) CSTP on physical network — need real headend address/DNS before create.
    let effectiveOptionsJson = optionsJson;
    const prepared: string = await withTimeout(
      hopenconnectUi.prepareVpnInExtension(optionsJson),
      PREPARE_TIMEOUT_MS,
      'native VPN prepare',
    );
    this.requireCurrentRequest(generation, attemptId, connection);
    if (typeof prepared === 'string' && prepared.length > 2) {
      effectiveOptionsJson = prepared;
      hilog.info(LOG_DOMAIN, LOG_TAG, 'extension prepare ok len=%{public}d', prepared.length);
    }

    const options = parseOptions(effectiveOptionsJson);
    if (!options.addresses || options.addresses.length === 0) {
      throw new Error('extension prepare returned no VPN addresses');
    }

    // 2) paws-shaped system VPN config (see VpnConfig.ets).
    const config: vpnExtension.VpnConfig = buildVpnConfig(options);
    hilog.info(LOG_DOMAIN, LOG_TAG,
      'create VpnConfig mtu=%{public}d addresses=%{public}d routes=%{public}d dns=%{public}d',
      config.mtu ?? 0,
      config.addresses ? config.addresses.length : 0,
      config.routes ? config.routes.length : 0,
      config.dnsAddresses ? config.dnsAddresses.length : 0);

    let createSettled: boolean = false;
    let createOperation: Promise<number>;
    try {
      createOperation = connection.create(config);
    } catch (err) {
      throw new Error(`VpnConnection.create rejected synchronously: ${describeError(err)}`);
    }
    const createBarrier: Promise<void> = createOperation.then((_fd: number): void => {
      createSettled = true;
    }).catch((_err: Error): void => {
      createSettled = true;
    });
    this.pendingPlatformOperation = createBarrier;
    this.pendingPlatformOperationConnection = connection;
    let fd: number;
    try {
      fd = await withTimeout(
        createOperation,
        PLATFORM_OPERATION_TIMEOUT_MS,
        'VpnConnection.create',
      );
    } finally {
      if (createSettled && this.pendingPlatformOperation === createBarrier) {
        this.pendingPlatformOperation = undefined;
        this.pendingPlatformOperationConnection = undefined;
      }
    }
    if (!this.isCurrentRequest(generation, attemptId) || this.vpnConnection !== connection) {
      await this.destroyVpn();
      throw new Error('stale VPN request generation after TUN create');
    }
    this.tunFd = fd;
    hilog.info(LOG_DOMAIN, LOG_TAG, 'VPN TUN fd=%{public}d', fd);

    // 3) paws: process-wide protect after TUN is up. Per-fd protection remains
    // authoritative for every OpenConnect control socket.
    if (typeof connection.protectProcessNet === 'function') {
      let protectSettled: boolean = false;
      const protectOperation: Promise<void> = connection.protectProcessNet();
      const protectBarrier: Promise<void> = protectOperation.then((): void => {
        protectSettled = true;
      }).catch((_err: Error): void => {
        protectSettled = true;
      });
      this.pendingPlatformOperation = protectBarrier;
      this.pendingPlatformOperationConnection = connection;
      try {
        await withTimeout(
          protectOperation,
          PLATFORM_OPERATION_TIMEOUT_MS,
          'VpnConnection.protectProcessNet',
        );
      } finally {
        if (protectSettled && this.pendingPlatformOperation === protectBarrier) {
          this.pendingPlatformOperation = undefined;
          this.pendingPlatformOperationConnection = undefined;
        }
      }
      this.requireCurrentRequest(generation, attemptId, connection);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'protectProcessNet ok');
    }

    // 4) OpenConnect packet loop.
    let startSettled: boolean = false;
    const startOperation: Promise<void> = hopenconnectUi.startVpn(fd, effectiveOptionsJson);
    const startBarrier: Promise<void> = startOperation.then((): void => {
      startSettled = true;
    }).catch((_err: Error): void => {
      startSettled = true;
    });
    this.pendingPlatformOperation = startBarrier;
    this.pendingPlatformOperationConnection = connection;
    try {
      await withTimeout(
        startOperation,
        PLATFORM_OPERATION_TIMEOUT_MS,
        'native VPN start',
      );
    } finally {
      if (startSettled && this.pendingPlatformOperation === startBarrier) {
        this.pendingPlatformOperation = undefined;
        this.pendingPlatformOperationConnection = undefined;
      }
    }
    this.requireCurrentRequest(generation, attemptId, connection);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'native startVpn completed fd=%{public}d', fd);
    this.startHeartbeat(generation, attemptId);
  }

  private extensionTick(): string {
    if (typeof (hopenconnectUi as ESObject).extensionTick !== 'function') {
      throw new Error('native extension heartbeat is unavailable');
    }
    return hopenconnectUi.extensionTick();
  }

  private startHeartbeat(generation: number, attemptId: string): void {
    this.stopHeartbeat();
    this.reconcileExtensionTerminal();
    if (!this.isCurrentRequest(generation, attemptId) || this.tunFd < 0) {
      return;
    }
    this.heartbeatTimer = setInterval((): void => {
      if (!this.isCurrentRequest(generation, attemptId)) {
        this.stopHeartbeat();
        return;
      }
      this.reconcileExtensionTerminal();
    }, 3000);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer >= 0) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = -1;
    }
  }

  private reconcileExtensionTerminal(): void {
    if (this.tunFd < 0 || this.activeAttemptId.length === 0) {
      return;
    }
    const generation = this.requestGeneration;
    const attemptId = this.activeAttemptId;
    let lifecycle: string;
    try {
      lifecycle = this.extensionTick();
    } catch (err) {
      const message = describeError(err) || 'extension heartbeat failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'extension heartbeat failed %{public}s', message);
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      lifecycle = 'failed';
    }
    if (lifecycle !== 'disconnected' && lifecycle !== 'failed') {
      return;
    }
    this.stopHeartbeat();
    // Freeze the old owner before cleanup starts. The UI may publish an
    // automatic-reconnect attempt immediately after observing this terminal
    // frame; normal subscription sync must not adopt it while old stop/destroy
    // work is still in flight.
    this.stopPlatformSubscription();
    this.requestChain = this.requestChain.then(async (): Promise<void> => {
      if (this.isCurrentRequest(generation, attemptId)) {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'terminal native session; destroying platform TUN');
        await this.destroyVpn();
      }
    });
  }

  private beginVpnCleanup(
    connection: vpnExtension.VpnConnection | undefined,
    attemptId: string,
  ): Promise<string> {
    if (this.pendingCleanup) {
      if (connection === undefined || connection === this.pendingCleanupConnection) {
        return this.pendingCleanup;
      }
      // A new connection is forbidden until the previous cleanup settles, so
      // reaching this branch indicates a violated serialization invariant.
      return Promise.resolve('a previous VpnConnection cleanup is still pending');
    }
    const pendingOperation = connection !== undefined &&
      this.pendingPlatformOperationConnection === connection ? this.pendingPlatformOperation : undefined;
    if (pendingOperation) {
      this.pendingPlatformOperation = undefined;
      this.pendingPlatformOperationConnection = undefined;
    }
    const cleanup: Promise<string> = (async (): Promise<string> => {
      const errors: string[] = [];
      try {
        // Stop invalidates the native generation immediately; awaiting its
        // completion here prevents a late global stop from cancelling a newer
        // native session.
        await hopenconnectUi.stopVpn(attemptId);
      } catch (err) {
        errors.push(`native VPN stop: ${describeError(err)}`);
      }
      if (pendingOperation) {
        // A timed-out create/protect/start Promise is not cancelled by
        // Promise semantics. Wait for it before destroying this exact object.
        await pendingOperation;
      }
      if (connection) {
        try {
          await connection.destroy();
        } catch (err) {
          errors.push(`VpnConnection.destroy: ${describeError(err)}`);
        }
      }
      return errors.join('; ');
    })();
    this.pendingCleanup = cleanup;
    this.pendingCleanupConnection = connection;
    return cleanup;
  }

  private async waitForPendingCleanup(): Promise<void> {
    const cleanup = this.pendingCleanup;
    if (!cleanup) {
      return;
    }
    const error = await cleanup;
    if (this.pendingCleanup === cleanup && error.length === 0) {
      this.pendingCleanup = undefined;
      this.pendingCleanupConnection = undefined;
    }
    if (error.length > 0) {
      throw new Error(`previous VPN cleanup failed: ${error}`);
    }
  }

  private async destroyVpn(): Promise<void> {
    this.stopHeartbeat();
    this.stopPlatformSubscription();
    const connection = this.vpnConnection;
    const attemptId = this.activeAttemptId;
    this.tunFd = -1;
    this.vpnConnection = undefined;
    this.starting = false;
    this.activeAttemptId = '';
    try {
      if (typeof (hopenconnectUi as ESObject).clearSocketProtect === 'function') {
        hopenconnectUi.clearSocketProtect();
      }
    } catch (err) {
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'clear socket protect failed %{public}s', describeError(err));
    }
    const cleanup = this.beginVpnCleanup(connection, attemptId);
    try {
      const error = await withTimeout(cleanup, PLATFORM_OPERATION_TIMEOUT_MS, 'VPN cleanup');
      if (error.length > 0) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN cleanup failed %{public}s', error);
        return;
      }
      if (this.pendingCleanup === cleanup) {
        this.pendingCleanup = undefined;
        this.pendingCleanupConnection = undefined;
      }
    } catch (err) {
      // Keep pendingCleanup as a barrier. A later request waits for this same
      // Promise instead of creating a new classic VPN that a late destroy could
      // tear down.
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN cleanup pending %{public}s', describeError(err));
    }
  }
}
