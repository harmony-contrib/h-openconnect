import { bundleManager, common } from '@kit.AbilityKit';
import { deviceInfo } from '@kit.BasicServicesKit';

export interface NativePlatformIdentityApi {
  configurePlatformIdentity(
    osFullName: string,
    displayVersion: string,
    sdkApiVersion: string,
    deviceType: string,
    appVersion: string,
    uniqueId: string,
  ): void;
}

/** Configure the native AnyConnect identity for the current process. */
export function configureNativePlatformIdentity(
  context: common.Context,
  nativeApi: NativePlatformIdentityApi,
): void {
  const bundleInfo = bundleManager.getBundleInfoForSelfSync(
    bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT,
  );
  nativeApi.configurePlatformIdentity(
    deviceInfo.osFullName,
    deviceInfo.displayVersion,
    deviceInfo.sdkApiVersion.toString(),
    deviceInfo.deviceType,
    bundleInfo.versionName,
    deviceInfo.ODID,
  );
}
