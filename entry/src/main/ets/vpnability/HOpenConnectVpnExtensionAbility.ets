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
  readOptionsJson,
  setHOpenBundleName,
} from './VpnConfig';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectVpn';

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
    try {
      if (typeof (hopenconnectUi as ESObject).clearSocketProtect === 'function') {
        (hopenconnectUi as ESObject).clearSocketProtect();
      }
    } catch (_err) {
      // ignore
    }
    try {
      hopenconnectUi.setPlatformVpnRunning(false);
    } catch (_err) {
      // ignore
    }
    this.destroyVpn();
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

  private registerIcsSocketProtect(): void {
    const connection = this.vpnConnection;
    if (!connection) {
      return;
    }
    try {
      if (typeof (hopenconnectUi as ESObject).registerSocketProtect === 'function') {
        (hopenconnectUi as ESObject).registerSocketProtect({
          protectSocket: async (fd: number): Promise<void> => {
            if (fd < 0 || !this.vpnConnection) {
              throw new Error('VpnConnection is unavailable');
            }
            try {
              await this.vpnConnection.protect(fd);
              hilog.info(LOG_DOMAIN, LOG_TAG, 'protect(fd) ok fd=%{public}d', fd);
            } catch (err) {
              const message = describeError(err);
              hilog.warn(LOG_DOMAIN, LOG_TAG, 'protect(fd) failed fd=%{public}d %{public}s',
                fd, message);
              throw new Error(message || 'protect(fd) failed');
            }
          },
        });
        hilog.info(LOG_DOMAIN, LOG_TAG, 'ics socket protect registered');
      }
    } catch (err) {
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'register socket protect skipped %{public}s', describeError(err));
    }
  }

  private startFromWant(want: Want): void {
    try {
      this.configureNativeHome();
      configureNativePlatformIdentity(hopenconnectUi);
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN request rejected %{public}s', describeError(err));
      return;
    }
    const sharedMemory = readPlatformSharedMemoryFds(want);
    if (!sharedMemory) {
      const message = 'VPN start is missing platform shared memory descriptors';
      hilog.error(LOG_DOMAIN, LOG_TAG, message);
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // No ashmem session is available to publish this failure.
      }
      return;
    }
    try {
      hopenconnectUi.attachPlatformSharedMemory(sharedMemory.ashmemFd, sharedMemory.notificationFd);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'attached platform shared memory');
      this.startPlatformSubscription();
    } catch (err) {
      const message = `attach platform shared memory failed: ${describeError(err)}`;
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN request rejected %{public}s', message);
      this.starting = false;
      return;
    }
    if (this.tunFd >= 0) {
      this.starting = false;
      try {
        hopenconnectUi.setPlatformVpnRunning(true);
        hilog.info(LOG_DOMAIN, LOG_TAG, 'reported existing VPN to rebound platform shared memory');
      } catch (err) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'report existing VPN state failed %{public}s', describeError(err));
      }
      return;
    }
    if (this.starting) {
      try {
        hopenconnectUi.setPlatformVpnStarting(true);
        hilog.info(LOG_DOMAIN, LOG_TAG, 'reported pending VPN to rebound platform shared memory');
      } catch (err) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'report pending VPN state failed %{public}s', describeError(err));
      }
      return;
    }
    this.starting = true;
    let optionsJson: string;
    try {
      optionsJson = readOptionsJson(want);
    } catch (err) {
      const message = describeError(err) || 'VPN request has no valid options';
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN request rejected %{public}s', message);
      this.starting = false;
      return;
    }
    hilog.info(LOG_DOMAIN, LOG_TAG, 'optionsJson length=%{public}d', optionsJson.length);
    this.performStart(optionsJson).catch((err: Error) => {
      const message = describeError(err) || 'vpn start failed';
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN start failed %{public}s', message);
      this.destroyVpn();
      this.starting = false;
    });
  }

  private startPlatformSubscription(): void {
    if (this.platformSubscriptionActive) {
      return;
    }
    this.platformSubscriptionActive = true;
    this.runPlatformSubscription();
  }

  private stopPlatformSubscription(): void {
    this.platformSubscriptionActive = false;
  }

  private async runPlatformSubscription(): Promise<void> {
    while (this.platformSubscriptionActive) {
      try {
        const changed = await hopenconnectUi.waitForPlatformChange(1000);
        if (changed && this.platformSubscriptionActive) {
          hopenconnectUi.syncPlatformChanges();
        }
      } catch (err) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'platform subscription failed %{public}s', describeError(err));
        this.platformSubscriptionActive = false;
      }
    }
  }

  private async performStart(optionsJson: string): Promise<void> {
    try {
      hopenconnectUi.setPlatformVpnStarting(true);
    } catch (_err) {
      // ignore
    }

    this.vpnConnection = vpnExtension.createVpnConnection(this.context);
    this.registerIcsSocketProtect();

    // 1) CSTP on physical network — need real headend address/DNS before create.
    let effectiveOptionsJson = optionsJson;
    try {
      const prepared: string = await (hopenconnectUi as ESObject).prepareVpnInExtension(optionsJson);
      if (typeof prepared === 'string' && prepared.length > 2) {
        effectiveOptionsJson = prepared;
        hilog.info(LOG_DOMAIN, LOG_TAG, 'extension prepare ok len=%{public}d', prepared.length);
      }
    } catch (prepErr) {
      const message = describeError(prepErr) || 'extension prepare failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'extension prepare failed %{public}s', message);
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      this.starting = false;
      throw new Error(message);
    }

    const options = parseOptions(effectiveOptionsJson);
    if (!options.addresses || options.addresses.length === 0) {
      const message = 'extension prepare returned no VPN addresses';
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      this.starting = false;
      throw new Error(message);
    }

    // 2) paws-shaped system VPN config (see VpnConfig.ets).
    const config: vpnExtension.VpnConfig = buildVpnConfig(options);
    hilog.info(LOG_DOMAIN, LOG_TAG,
      'create VpnConfig mtu=%{public}d addresses=%{public}d routes=%{public}d dns=%{public}d',
      config.mtu ?? 0,
      config.addresses ? config.addresses.length : 0,
      config.routes ? config.routes.length : 0,
      config.dnsAddresses ? config.dnsAddresses.length : 0);

    try {
      const fd = await this.vpnConnection.create(config);
      this.tunFd = fd;
      hilog.info(LOG_DOMAIN, LOG_TAG, 'VPN TUN fd=%{public}d', fd);

      // 3) paws: process-wide protect after TUN is up.
      try {
        if (typeof this.vpnConnection.protectProcessNet === 'function') {
          await this.vpnConnection.protectProcessNet();
          hilog.info(LOG_DOMAIN, LOG_TAG, 'protectProcessNet ok');
        }
      } catch (protectErr) {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'protectProcessNet failed %{public}s',
          describeError(protectErr));
      }

      // 4) OpenConnect packet loop.
      await hopenconnectUi.startVpn(fd, effectiveOptionsJson);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'native startVpn completed fd=%{public}d', fd);
    } catch (err) {
      const message = describeError(err) || 'native startVpn failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'VPN pipeline failed %{public}s', message);
      try {
        hopenconnectUi.setPlatformVpnFailed(message);
      } catch (_inner) {
        // ignore
      }
      this.destroyVpn();
      throw new Error(message);
    } finally {
      this.starting = false;
    }
  }

  private destroyVpn(): void {
    this.stopPlatformSubscription();
    const connection = this.vpnConnection;
    this.tunFd = -1;
    this.vpnConnection = undefined;
    if (!connection) {
      return;
    }
    try {
      connection.destroy().catch((err: Error) => {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'destroy VPN failed %{public}s', describeError(err));
      });
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'destroy VPN threw %{public}s', describeError(err));
    }
  }
}
