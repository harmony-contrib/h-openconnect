import { common } from '@kit.AbilityKit';
import { preferences } from '@kit.ArkData';

export const CURRENT_PRIVACY_POLICY_VERSION: number = 2;

const PRIVACY_PREFERENCES_NAME: string = 'hopenconnect_privacy';
const ACCEPTED_POLICY_VERSION_KEY: string = 'acceptedPrivacyPolicyVersion';

function privacyPreferences(context: common.Context): preferences.Preferences | null {
  try {
    return preferences.getPreferencesSync(context, { name: PRIVACY_PREFERENCES_NAME });
  } catch (_err) {
    return null;
  }
}

export function hasPrivacyConsent(context: common.Context): boolean {
  try {
    const store = privacyPreferences(context);
    if (store === null) {
      return false;
    }
    const acceptedVersion = store.getSync(ACCEPTED_POLICY_VERSION_KEY, 0) as number;
    return acceptedVersion === CURRENT_PRIVACY_POLICY_VERSION;
  } catch (_err) {
    return false;
  }
}

export function grantPrivacyConsent(context: common.Context): boolean {
  try {
    const store = privacyPreferences(context);
    if (store === null) {
      return false;
    }
    store.putSync(ACCEPTED_POLICY_VERSION_KEY, CURRENT_PRIVACY_POLICY_VERSION);
    store.flushSync();
    return true;
  } catch (_err) {
    return false;
  }
}

export function withdrawPrivacyConsent(context: common.Context): boolean {
  try {
    const store = privacyPreferences(context);
    if (store === null) {
      return false;
    }
    store.deleteSync(ACCEPTED_POLICY_VERSION_KEY);
    store.flushSync();
    return true;
  } catch (_err) {
    return false;
  }
}
