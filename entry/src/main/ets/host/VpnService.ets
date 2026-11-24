import { vpnExtension } from "@kit.NetworkKit";
import { hilog } from "@kit.PerformanceAnalysisKit";
import hopenconnectUi from "libhopenconnect_ui.so";
import { describeError } from "../common/Errors";
import { buildVpnWant, HOpenPlatformSharedMemoryFds } from "../vpnability/VpnConfig";
import {
  authorizeVpnForEmulatorDebug,
  canRecoverRejectedEmulatorStop,
  isVpnEmulatorCompatibilityEnabled,
} from "../vpnability/VpnEmulatorCompatibility";
import { startVpnExtensionWithHandoff } from "../vpnability/VpnExtensionLauncher";

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = "HOpenConnectVpn";
const VPN_COOPERATIVE_STOP_TIMEOUT_MS: number = 3000;

let vpnOperationChain: Promise<void> = Promise.resolve();

/**
 * System tunnel extension controller owned by the HarmonyOS Ability host.
 * Preserves attempt-scoped shared memory initialization, descriptor-free
 * handoff, cooperative stop fencing, and emulator debug recovery.
 */
export class VpnService {
  private vpnStartRequest: Promise<void> | undefined = undefined;
  private activeVpnStartAttemptId: string = "";
  private operationGeneration: number = 0;
  private operationChain: Promise<void> = Promise.resolve();
  private startupVpnPreparation: Promise<void> = Promise.resolve();
  private platformSharedMemory?: HOpenPlatformSharedMemoryFds;

  attachHost(): void {
    this.initializePlatformSharedMemory();
    authorizeVpnForEmulatorDebug();
    this.startupVpnPreparation = this.prepareVpnForEmulatorDebug();
  }

  detachHost(): void {
    this.activeVpnStartAttemptId = "";
  }

  async requestStartVpn(optionsJson: string): Promise<void> {
    await this.startupVpnPreparation;
    if (this.vpnStartRequest) {
      return this.vpnStartRequest;
    }
    const intentEpoch = hopenconnectUi.advancePlatformVpnIntent();
    const generation = ++this.operationGeneration;
    const request = this.enqueueOperation(async (): Promise<void> => {
      if (
        generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)
      ) {
        throw new Error("connection start request was superseded");
      }
      await this.performStartVpn(optionsJson, generation, intentEpoch);
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

  async requestStopVpn(bestEffort: boolean = false): Promise<void> {
    await this.startupVpnPreparation;
    const intentEpoch = hopenconnectUi.advancePlatformVpnIntent();
    ++this.operationGeneration;
    const sessionId = hopenconnectUi.claimCurrentPlatformVpnStop(intentEpoch);
    if (bestEffort && sessionId.length === 0) {
      return;
    }
    await this.enqueueOperation(async (): Promise<boolean> => {
      return this.performStopVpn(sessionId, intentEpoch, bestEffort);
    });
  }

  private initializePlatformSharedMemory(): void {
    try {
      const fields = hopenconnectUi.initializePlatformSharedMemory().split(",");
      const ashmemFd = Number(fields[0]);
      const notificationFd = Number(fields[1]);
      if (
        fields.length !== 2 ||
        !Number.isInteger(ashmemFd) ||
        ashmemFd < 0 ||
        !Number.isInteger(notificationFd) ||
        notificationFd < 0
      ) {
        throw new Error("native platform shared memory returned invalid file descriptors");
      }
      this.platformSharedMemory = { ashmemFd, notificationFd };
      hilog.info(LOG_DOMAIN, LOG_TAG, "initialized platform shared memory");
    } catch (err) {
      hilog.error(
        LOG_DOMAIN,
        LOG_TAG,
        "initialize platform shared memory failed %{public}s",
        describeError(err),
      );
      throw new Error(describeError(err) || "initialize platform shared memory failed");
    }
  }

  private enqueueOperation<T>(operation: () => Promise<T>): Promise<T> {
    const localReady = this.operationChain.catch((_err: Error): void => {});
    const request = vpnOperationChain
      .catch((_err: Error): void => {})
      .then((): Promise<void> => localReady)
      .then(operation);
    this.operationChain = request.then(
      (_value: T): void => {},
      (_err: Error): void => {},
    );
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
          hilog.info(LOG_DOMAIN, LOG_TAG, "debug startup tunnel state is already clean");
          return;
        }
        await this.performStopVpn(recoverableSessionId, intentEpoch, true);
        hilog.info(
          LOG_DOMAIN,
          LOG_TAG,
          "debug startup cleanup completed for attempt %{public}s",
          recoverableSessionId,
        );
      });
    } catch (err) {
      hilog.error(
        LOG_DOMAIN,
        LOG_TAG,
        "debug startup tunnel preparation failed %{public}s",
        describeError(err),
      );
    }
  }

  private async performStartVpn(
    optionsJson: string,
    generation: number,
    intentEpoch: string,
  ): Promise<void> {
    let attemptId: string = "";
    try {
      if (
        generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)
      ) {
        throw new Error("connection start request was superseded");
      }
      attemptId = hopenconnectUi.beginPlatformVpnStartForIntent(intentEpoch);
      this.activeVpnStartAttemptId = attemptId;
      const sharedMemory = this.platformSharedMemory;
      if (!sharedMemory) {
        throw new Error("platform shared memory is unavailable");
      }

      await startVpnExtensionWithHandoff(optionsJson, attemptId, sharedMemory);
      if (
        generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)
      ) {
        throw new Error("connection start request was superseded");
      }

      const outcome = await hopenconnectUi.awaitPlatformVpnStart(attemptId);
      if (
        generation !== this.operationGeneration ||
        !hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch)
      ) {
        throw new Error("connection start request was superseded");
      }
      if (outcome !== "connected") {
        throw new Error(`unexpected connection start outcome: ${outcome}`);
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, "tunnel start completed attempt %{public}s", attemptId);
    } catch (err) {
      const message = describeError(err) || "request connection start failed";
      if (attemptId.length > 0) {
        try {
          hopenconnectUi.failPlatformVpnStart(attemptId, message);
        } catch (innerErr) {
          hilog.error(
            LOG_DOMAIN,
            LOG_TAG,
            "publish start failure failed %{public}s",
            describeError(innerErr),
          );
        }
      }
      hilog.error(LOG_DOMAIN, LOG_TAG, "request tunnel start failed %{public}s", message);
      throw new Error(message);
    } finally {
      if (this.activeVpnStartAttemptId === attemptId) {
        this.activeVpnStartAttemptId = "";
      }
    }
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
      const cooperativeStopRequested =
        sessionId.length > 0 && hopenconnectUi.requestPlatformVpnStop(sessionId);
      if (cooperativeStopRequested) {
        cleanupObservation = hopenconnectUi.awaitPlatformVpnStop(sessionId).then(
          (completed: boolean): string => (completed ? "complete" : "superseded"),
          (err: Error): string => `error:${describeError(err) || "platform cleanup wait failed"}`,
        );
        const cooperativeResult = await this.waitForCooperativeCleanup(cleanupObservation);
        if (cooperativeResult === "complete") {
          cleanupCompleted = true;
        } else if (cooperativeResult === "superseded") {
          return false;
        } else if (cooperativeResult.startsWith("error:")) {
          hilog.warn(
            LOG_DOMAIN,
            LOG_TAG,
            "cooperative cleanup wait failed %{public}s",
            cooperativeResult.slice("error:".length),
          );
        } else {
          hilog.warn(
            LOG_DOMAIN,
            LOG_TAG,
            "cooperative cleanup still pending for %{public}s; requesting OS stop",
            sessionId,
          );
        }
      }
      if (
        !this.canDispatchPlatformVpnStop(intentEpoch, sessionId) ||
        !hopenconnectUi.beginPlatformVpnOsStop(intentEpoch, sessionId)
      ) {
        return false;
      }
      try {
        await vpnExtension.stopVpnExtensionAbility(buildVpnWant());
      } catch (err) {
        const recoverAsAlreadyStopped = canRecoverRejectedEmulatorStop(
          recoverUnattachedDebugOwner,
          sessionId,
          cooperativeStopRequested,
        );
        if (!recoverAsAlreadyStopped) {
          if (!hopenconnectUi.failPlatformVpnOsStop(intentEpoch, sessionId)) {
            hilog.error(
              LOG_DOMAIN,
              LOG_TAG,
              "failed to retain exact OS stop fence after system rejection",
            );
          }
          throw new Error(describeError(err) || "system rejected stop");
        }
        hilog.warn(
          LOG_DOMAIN,
          LOG_TAG,
          "debug startup cleanup found no attached tunnel for stale attempt %{public}s; " +
            "treating rejected OS stop as already stopped",
          sessionId,
        );
      }
      if (!hopenconnectUi.completePlatformVpnOsStop(intentEpoch, sessionId)) {
        throw new Error(`OS stop confirmation did not match exact fence ${sessionId}`);
      }
      if (sessionId.length > 0 && !cleanupCompleted) {
        cleanupCompleted =
          await hopenconnectUi.recoverPlatformVpnCleanupAfterConfirmedStop(sessionId);
        if (!cleanupCompleted) {
          throw new Error(
            `cleanup owner ${sessionId} remained live or was superseded after confirmed OS stop`,
          );
        }
      }
      hilog.info(LOG_DOMAIN, LOG_TAG, "stop cleanup confirmed attempt %{public}s", sessionId);
      return true;
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, "request tunnel stop failed %{public}s", describeError(err));
      throw new Error(describeError(err) || "request stop failed");
    }
  }

  private async waitForCooperativeCleanup(cleanup: Promise<string>): Promise<string> {
    let timeoutId: number = -1;
    const timeout = new Promise<string>((resolve: (value: string) => void): void => {
      timeoutId = setTimeout((): void => resolve("pending"), VPN_COOPERATIVE_STOP_TIMEOUT_MS);
    });
    const result = await Promise.race([cleanup, timeout]);
    if (timeoutId >= 0) {
      clearTimeout(timeoutId);
    }
    return result;
  }

  private canDispatchPlatformVpnStop(intentEpoch: string, sessionId: string): boolean {
    return (
      hopenconnectUi.isPlatformVpnStopCurrent(intentEpoch, sessionId) ||
      (sessionId.length === 0 && hopenconnectUi.isPlatformVpnIntentCurrent(intentEpoch))
    );
  }
}
