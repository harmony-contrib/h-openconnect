import { bundleManager, common } from '@kit.AbilityKit';
import { deviceInfo } from '@kit.BasicServicesKit';
import { hasPrivacyConsent } from './PrivacyConsent';

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

/**
 * Configure the native AnyConnect identity only after the current privacy
 * policy has been accepted. Keep deviceInfo.ODID inside this guarded function
 * so future call sites cannot accidentally read it before consent.
 */
export function configureNativePlatformIdentityIfConsented(
  context: common.Context,
  nativeApi: NativePlatformIdentityApi,
): boolean {
  if (!hasPrivacyConsent(context)) {
    return false;
  }
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
  return true;
}
