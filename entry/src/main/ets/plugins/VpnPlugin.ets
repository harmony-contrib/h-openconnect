import {
  AsyncPluginBase,
  BridgeCallContext,
  BridgeContextRequirement,
  BridgePluginContext,
  BridgePluginHookContext,
  BridgeTypedValue,
} from '@ohos-rs/ability';
import { vpnExtension } from '@kit.NetworkKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';
import { buildVpnWant, HOpenPlatformSharedMemoryFds } from '../vpnability/VpnConfig';
import {
  authorizeVpnForEmulatorDebug,
  canRecoverRejectedEmulatorStop,
  isVpnEmulatorCompatibilityEnabled,
} from '../vpnability/VpnEmulatorCompatibility';
import { startVpnExtensionWithHandoff } from '../vpnability/VpnExtensionLauncher';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectVpn';

const VPN_START_REQUEST_TYPE = 'hopenconnect.VpnStartRequest';
const VPN_START_RESPONSE_TYPE = 'hopenconnect.VpnStartResponse';
const VPN_STOP_REQUEST_TYPE = 'hopenconnect.VpnStopRequest';
const VPN_STOP_RESPONSE_TYPE = 'hopenconnect.VpnStopResponse';
const VPN_COOPERATIVE_STOP_TIMEOUT_MS: number = 3000;

let vpnOperationChain: Promise<void> = Promise.resolve();

interface VpnStartRequest {
  optionsJson: string;
}

/**
 * `hopenconnect.vpn` — system VPN extension control. Owns the full start
 * orchestration (shared-memory initialization, first-authorization redispatch
 * and outcome reporting) so the Ability entry stays a thin assembly point.
 */
export class VpnPlugin extends AsyncPluginBase {
  readonly id = 'hopenconnect.vpn';
  readonly requires: BridgeContextRequirement[] = ['ability'];

  private vpnStartRequest: Promise<void> | undefined = undefined;
  private activeVpnStartAttemptId: string = '';
  private operationGeneration: number = 0;
  private operationChain: Promise<void> = Promise.resolve();
  private startupVpnPreparation: Promise<void> = Promise.resolve();
  private platformSharedMemory?: HOpenPlatformSharedMemoryFds;

  onInstall(_context: BridgePluginHookContext): void {
    this.initializePlatformSharedMemory();
    authorizeVpnForEmulatorDebug();
    this.startupVpnPreparation = this.prepareVpnForEmulatorDebug();
  }

  async invokeAsync(
    action: string,
    request: BridgeTypedValue,
    _context: BridgeCallContext,
  ): Promise<BridgeTypedValue> {
    if (action === 'start-vpn') {
      if (request.typeName !== VPN_START_REQUEST_TYPE || typeof request.value !== 'object') {
        throw new Error(`hopenconnect.vpn expected ${VPN_START_REQUEST_TYPE}, got ${request.typeName}`);
      }
      const payload = request.value as VpnStartRequest;
      if (typeof payload.optionsJson !== 'string') {
        throw new Error(`hopenconnect.vpn invalid start request shape`);
      }
      await this.requestStartVpn(payload.optionsJson);
      return { typeName: VPN_START_RESPONSE_TYPE, value: {} };
    }
    if (action === 'stop-vpn') {
      if (request.typeName !== VPN_STOP_REQUEST_TYPE || typeof request.value !== 'object') {
        throw new Error(`hopenconnect.vpn expected ${VPN_STOP_REQUEST_TYPE}, got ${request.typeName}`);
      }
      await this.requestStopVpn();
      return { typeName: VPN_STOP_RESPONSE_TYPE, value: {} };
    }
    throw new Error(`Unsupported hopenconnect.vpn action '${action}'`);
  }

  /** Public entry for in-app callers (debug automation). */
  async requestStartVpn(optionsJson: string): Promise<void> {
    await this.requestStartVpnWithContext(optionsJson, this.getContext());
  }

  /** Public entry for in-app callers (debug automation). */
  async requestStopVpn(): Promise<void> {
    await this.requestStopVpnWithContext(this.getContext());
  }

  private initializePlatformSharedMemory(): void {
    try {
      const fields = hopenconnectUi.initializePlatformSharedMemory().split(',');
      const ashmemFd = Number(fields[0]);
      const notificationFd = Number(fields[1]);
      if (fields.length !== 2 || !Number.isInteger(ashmemFd) || ashmemFd < 0 ||
        !Number.isInteger(notificationFd) || notificationFd < 0) {
        throw new Error('native platform shared memory returned invalid file descriptors');
      }
      this.platformSharedMemory = { ashmemFd, notificationFd };
      hilog.info(LOG_DOMAIN, LOG_TAG, 'initialized platform shared memory');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'initialize platform shared memory failed %{public}s',
        describeError(err));
      throw new Error(describeError(err) || 'initialize platform shared memory failed');
    }
  }

  private async requestStartVpnWithContext(
    optionsJson: string,
    context: BridgePluginContext,
  ): Promise<void> {
    await this.startupVpnPreparation;
    if (this.vpnStartRequest) {
      return this.vpnStartRequest;
    }
    const intentEpoch = hopenconnectUi.advancePlatformVpnIntent();
    const generation = ++this.operationGeneration;
    const request = this.enqueueOperation(async (): Promise<void> => {
      if (generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)) {
        throw new Error('VPN start request was superseded');
      }
      await this.performStartVpn(optionsJson, context, generation, intentEpoch);
    });
    this.vpnStartRequest = request;
    try {
      await request;
    } finally {
      if (this.vpnStartRequest === request) {
        this.vpnStartRequest = undefined;
      }
    }
  }

  private enqueueOperation<T>(operation: () => Promise<T>): Promise<T> {
    const localReady = this.operationChain.catch((_err: Error): void => {});
    const request = vpnOperationChain
      .catch((_err: Error): void => {})
      .then((): Promise<void> => localReady)
      .then(operation);
    this.operationChain = request.then((_value: T): void => {}, (_err: Error): void => {});
    vpnOperationChain = this.operationChain;
    return request;
  }

  private async prepareVpnForEmulatorDebug(): Promise<void> {
    if (!isVpnEmulatorCompatibilityEnabled()) {
      return;
    }
    try {
      const intentEpoch = hopenconnectUi.advancePlatformVpnIntent();
      await this.enqueueOperation(async (): Promise<void> => {
        if (!hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)) {
          return;
        }
        const recoverableSessionId = hopenconnectUi.claimCurrentPlatformVpnStop(intentEpoch);
        if (recoverableSessionId.length === 0) {
          hilog.info(LOG_DOMAIN, LOG_TAG, 'debug startup VPN state is already clean');
          return;
        }
        await this.performStopVpn(recoverableSessionId, intentEpoch, true);
        hilog.info(LOG_DOMAIN, LOG_TAG,
          'debug startup VPN cleanup completed for attempt %{public}s', recoverableSessionId);
      });
    } catch (err) {
      // Keep app startup usable and surface a precise failure. A later start
      // will fail on the still-owned journal instead of silently replacing it.
      hilog.error(LOG_DOMAIN, LOG_TAG,
        'debug startup VPN preparation failed %{public}s', describeError(err));
    }
  }

  private async performStartVpn(
    optionsJson: string,
    _context: BridgePluginContext,
    generation: number,
    intentEpoch: string,
  ): Promise<void> {
    let attemptId: string = '';
    try {
      if (generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)) {
        throw new Error('VPN start request was superseded');
      }
      attemptId = hopenconnectUi.beginPlatformVpnStartForIntent(intentEpoch);
      this.activeVpnStartAttemptId = attemptId;
      const sharedMemory = this.platformSharedMemory;
      if (!sharedMemory) {
        throw new Error('platform shared memory is unavailable');
      }

      // The emulator drops an FD-bearing first VPN Want before it creates the
      // Extension process. Bootstrap the real system VPN Extension without
      // descriptors, then deliver the existing shared-memory session to that
      // live instance. This remains one logical start attempt and never mocks
      // the VPN lifecycle or tunnel.
      await startVpnExtensionWithHandoff(optionsJson, attemptId, sharedMemory);
      if (generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)) {
        throw new Error('VPN start request was superseded');
      }

      const outcome = await hopenconnectUi.awaitPlatformVpnStart(attemptId);
      if (generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)) {
        throw new Error('VPN start request was superseded');
      }
      if (outcome !== 'connected') {
        throw new Error(`unexpected VPN start outcome: ${outcome}`);
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, 'VPN start completed attempt %{public}s', attemptId);
    } catch (err) {
      const message = describeError(err) || 'request VPN start failed';
      if (attemptId.length > 0) {
        try {
          // Exactly-once: this is a no-op if ashmem already supplied a
          // terminal outcome or a newer attempt replaced this one.
          hopenconnectUi.failPlatformVpnStart(attemptId, message);
        } catch (innerErr) {
          hilog.error(LOG_DOMAIN, LOG_TAG, 'publish VPN start failure failed %{public}s',
            describeError(innerErr));
        }
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, 'request VPN start failed %{public}s', message);
      throw new Error(message);
    } finally {
      if (this.activeVpnStartAttemptId === attemptId) {
        this.activeVpnStartAttemptId = '';
      }
    }
  }

  private async requestStopVpnWithContext(_context: BridgePluginContext): Promise<void> {
    await this.startupVpnPreparation;
    const intentEpoch = hopenconnectUi.advancePlatformVpnIntent();
    ++this.operationGeneration;
    const sessionId = hopenconnectUi.claimCurrentPlatformVpnStop(intentEpoch);
    await this.enqueueOperation(async (): Promise<boolean> => {
      return this.performStopVpn(sessionId, intentEpoch, false);
    });
  }

  private async performStopVpn(
    sessionId: string,
    intentEpoch: string,
    recoverUnattachedDebugOwner: boolean,
  ): Promise<boolean> {
    try {
      if (!this.canDispatchPlatformVpnStop(intentEpoch, sessionId)) {
        return false;
      }
      let cleanupObservation: Promise<string> | undefined = undefined;
      let cleanupCompleted = sessionId.length === 0;
      const cooperativeStopRequested = sessionId.length > 0 &&
        hopenconnectUi.requestPlatformVpnStop(sessionId);
      if (cooperativeStopRequested) {
        cleanupObservation = hopenconnectUi.awaitPlatformVpnStop(sessionId).then(
          (completed: boolean): string => completed ? 'complete' : 'superseded',
          (err: Error): string => `error:${describeError(err) || 'platform cleanup wait failed'}`,
        );
        const cooperativeResult = await this.waitForCooperativeCleanup(cleanupObservation);
        if (cooperativeResult === 'complete') {
          cleanupCompleted = true;
        } else if (cooperativeResult === 'superseded') {
          return false;
        } else if (cooperativeResult.startsWith('error:')) {
          hilog.warn(
            LOG_DOMAIN,
            LOG_TAG,
            'cooperative VPN cleanup wait failed %{public}s',
            cooperativeResult.slice('error:'.length),
          );
        } else {
          hilog.warn(
            LOG_DOMAIN,
            LOG_TAG,
            'cooperative VPN cleanup still pending for %{public}s; requesting OS stop',
            sessionId,
          );
        }
      }
      if (!this.canDispatchPlatformVpnStop(intentEpoch, sessionId) ||
        !hopenconnectUi.beginPlatformVpnOsStop(intentEpoch, sessionId)) {
        return false;
      }
      try {
        await vpnExtension.stopVpnExtensionAbility(buildVpnWant());
      } catch (err) {
        // The emulator can retain our durable Pending/Stopping owner after a
        // failed attempt while SettingsData has no active VPN row. In that
        // exact unattached debug-only case, StopVpnExtensionAbility rejects
        // without an error payload even though there is nothing left to stop.
        // Treat that response as an already-stopped confirmation so the exact
        // stale owner can be released before starting the replacement.
        const recoverAsAlreadyStopped = canRecoverRejectedEmulatorStop(
          recoverUnattachedDebugOwner,
          sessionId,
          cooperativeStopRequested,
        );
        if (!recoverAsAlreadyStopped) {
          if (!hopenconnectUi.failPlatformVpnOsStop(intentEpoch, sessionId)) {
            hilog.error(LOG_DOMAIN, LOG_TAG,
              'failed to retain exact VPN OS stop fence after system rejection');
          }
          throw new Error(describeError(err) || 'system rejected VPN stop');
        }
        hilog.warn(LOG_DOMAIN, LOG_TAG,
          'debug startup cleanup found no attached VPN for stale attempt %{public}s; ' +
          'treating rejected OS stop as already stopped', sessionId);
      }
      if (!hopenconnectUi.completePlatformVpnOsStop(intentEpoch, sessionId)) {
        throw new Error(`VPN OS stop confirmation did not match exact fence ${sessionId}`);
      }
      if (sessionId.length > 0 && !cleanupCompleted) {
        cleanupCompleted =
          await hopenconnectUi.recoverPlatformVpnCleanupAfterConfirmedStop(sessionId);
        if (!cleanupCompleted) {
          throw new Error(
            `VPN cleanup owner ${sessionId} remained live or was superseded after confirmed OS stop`);
        }
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, 'VPN stop cleanup confirmed attempt %{public}s', sessionId);
      return true;
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'request VPN stop failed %{public}s', describeError(err));
      throw new Error(describeError(err) || 'request VPN stop failed');
    }
  }

  private async waitForCooperativeCleanup(cleanup: Promise<string>): Promise<string> {
    let timeoutId: number = -1;
    const timeout = new Promise<string>((resolve: (value: string) => void): void => {
      timeoutId = setTimeout((): void => resolve('pending'), VPN_COOPERATIVE_STOP_TIMEOUT_MS);
    });
    const result = await Promise.race([cleanup, timeout]);
    if (timeoutId >= 0) {
      clearTimeout(timeoutId);
    }
    return result;
  }

  private canDispatchPlatformVpnStop(intentEpoch: string, sessionId: string): boolean {
    return hopenconnectUi.isPlatformVpnStopCurrent(intentEpoch, sessionId) ||
      (sessionId.length === 0 && hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch));
  }
}
