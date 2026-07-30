import { bundleManager } from '@kit.AbilityKit';
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

export function configureNativePlatformIdentity(nativeApi: NativePlatformIdentityApi): void {
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
