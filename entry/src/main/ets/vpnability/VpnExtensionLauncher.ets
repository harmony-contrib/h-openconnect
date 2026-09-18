import { vpnExtension } from '@kit.NetworkKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';
import {
  buildVpnBootstrapWant,
  HOpenPlatformSharedMemoryFds,
} from './VpnConfig';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectVpn';

/**
 * Start the real system VPN Extension without descriptors, then transfer the
 * existing ashmem descriptors over the one-time native handoff channel.
 *
 * Some emulator VPN dispatchers acknowledge an FD-bearing Want but never
 * create the Extension process. Keeping descriptors out of the system start
 * also avoids making NetManager the owner of application session transport.
 */
export async function startVpnExtensionWithHandoff(
  optionsJson: string,
  attemptId: string,
  sharedMemory: HOpenPlatformSharedMemoryFds,
): Promise<void> {
  let handoffToken: string;
  try {
    handoffToken = hopenconnectUi.preparePlatformVpnHandoff(attemptId);
  } catch (err) {
    throw new Error(describeError(err) || 'prepare VPN session FD handoff failed');
  }
  try {
    hilog.info(LOG_DOMAIN, LOG_TAG,
      'dispatching descriptor-free system VPN bootstrap attempt %{public}s', attemptId);
    await vpnExtension.startVpnExtensionAbility(
      buildVpnBootstrapWant(attemptId, handoffToken),
    );
    hilog.info(LOG_DOMAIN, LOG_TAG,
      'system VPN bootstrap accepted attempt %{public}s', attemptId);

    hilog.info(LOG_DOMAIN, LOG_TAG,
      'sending VPN session FD handoff attempt %{public}s', attemptId);
    await hopenconnectUi.sendPlatformVpnHandoff(
      sharedMemory.ashmemFd,
      sharedMemory.notificationFd,
      attemptId,
      optionsJson,
    );
    hilog.info(LOG_DOMAIN, LOG_TAG,
      'VPN session FD handoff completed attempt %{public}s', attemptId);
  } catch (err) {
    throw new Error(describeError(err) || 'start VPN Extension with session handoff failed');
  }
}
