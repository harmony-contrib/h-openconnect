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

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectVpn';

const VPN_START_REQUEST_TYPE = 'hopenconnect.VpnStartRequest';
const VPN_START_RESPONSE_TYPE = 'hopenconnect.VpnStartResponse';
const VPN_STOP_REQUEST_TYPE = 'hopenconnect.VpnStopRequest';
const VPN_STOP_RESPONSE_TYPE = 'hopenconnect.VpnStopResponse';

const VPN_EXTENSION_ATTACH_GRACE_MS: number = 350;
const VPN_EXTENSION_REDISPATCH_ATTACH_TIMEOUT_MS: number = 5000;

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
  private platformSharedMemory?: HOpenPlatformSharedMemoryFds;

  onInstall(_context: BridgePluginHookContext): void {
    this.initializePlatformSharedMemory();
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
    if (this.vpnStartRequest) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'reusing pending VPN start request');
      return this.vpnStartRequest;
    }
    const request = this.performStartVpn(optionsJson, context);
    this.vpnStartRequest = request;
    try {
      await request;
    } finally {
      if (this.vpnStartRequest === request) {
        this.vpnStartRequest = undefined;
      }
    }
  }

  private dispatchVpnStart(optionsJson: string, attemptId: string, context: BridgePluginContext): void {
    this.runVpnStartDispatch(optionsJson, attemptId, context).catch((err: Error) => {
      const message = describeError(err) || 'system rejected VPN start';
      try {
        hopenconnectUi.failUnattachedPlatformVpnStart(attemptId, message);
      } catch (innerErr) {
        hilog.error(
          LOG_DOMAIN,
          LOG_TAG,
          'publish system VPN rejection failed %{public}s',
          describeError(innerErr),
        );
      }
    });
  }

  private async invokeVpnExtensionStart(optionsJson: string, attemptId: string): Promise<void> {
    try {
      await vpnExtension.startVpnExtensionAbility(
        buildVpnWant(optionsJson, this.platformSharedMemory, attemptId),
      );
    } catch (err) {
      throw new Error(describeError(err) || 'system rejected VPN start');
    }
  }

  private async runVpnStartDispatch(
    optionsJson: string,
    attemptId: string,
    _context: BridgePluginContext,
  ): Promise<void> {
    await this.invokeVpnExtensionStart(optionsJson, attemptId);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'system acknowledged VPN start attempt %{public}s', attemptId);

    if (await hopenconnectUi.awaitPlatformVpnStartAttachment(attemptId, VPN_EXTENSION_ATTACH_GRACE_MS)) {
      return;
    }

    // The first-authorization dialog on some HarmonyOS builds starts the VPN
    // Extension with a new Want containing only bundleName/abilityName. Once
    // authorization succeeds, send the original payload again so the running
    // Extension receives the ashmem descriptors and attempt id via onRequest.
    hilog.warn(
      LOG_DOMAIN,
      LOG_TAG,
      'VPN Extension did not receive the authorized Want; redispatching attempt %{public}s',
      attemptId,
    );
    await this.invokeVpnExtensionStart(optionsJson, attemptId);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'redispatched VPN start attempt %{public}s', attemptId);

    if (!(await hopenconnectUi.awaitPlatformVpnStartAttachment(
      attemptId,
      VPN_EXTENSION_REDISPATCH_ATTACH_TIMEOUT_MS,
    ))) {
      throw new Error('VPN Extension did not receive the shared-memory descriptors after authorization');
    }
  }

  private async performStartVpn(optionsJson: string, context: BridgePluginContext): Promise<void> {
    let attemptId: string = '';
    try {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'request VPN start received');
      attemptId = hopenconnectUi.beginPlatformVpnStart();
      this.activeVpnStartAttemptId = attemptId;

      this.dispatchVpnStart(optionsJson, attemptId, context);

      const outcome = await hopenconnectUi.awaitPlatformVpnStart(attemptId);
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
    try {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'request VPN stop received');
      if (this.activeVpnStartAttemptId.length > 0) {
        hopenconnectUi.cancelPlatformVpnStart(this.activeVpnStartAttemptId);
      }
      await vpnExtension.stopVpnExtensionAbility(buildVpnWant());
      hopenconnectUi.setPlatformVpnRunning(false);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'requested VPN stop');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'request VPN stop failed %{public}s', describeError(err));
      throw new Error(describeError(err) || 'request VPN stop failed');
    }
  }
}
