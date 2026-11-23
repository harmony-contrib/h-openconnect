use std::path::Path;

const ENTRY_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/entryability/EntryAbility.ets");
const TUNNEL_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/vpnability/HOpenConnectVpnExtensionAbility.ets");
const INDEX_PAGE: &str = include_str!("../../../entry/src/main/ets/pages/Index.ets");
const HOST_SERVICES: &str = include_str!("../../../entry/src/main/ets/host/HOpenHostServices.ets");
const PLATFORM_IDENTITY: &str =
    include_str!("../../../entry/src/main/ets/common/PlatformIdentity.ets");
const PACKAGE_MANIFEST: &str = include_str!("../../../oh-package.json5");
const BASE_STRINGS: &str =
    include_str!("../../../entry/src/main/resources/base/element/string.json");
const APP_SANITIZE: &str = include_str!("../../hopenconnect_app/src/sanitize.rs");
const APP_OBSERVATION: &str = include_str!("../../hopenconnect_app/src/observation.rs");
const UI_LIB: &str = include_str!("../src/lib.rs");
const WEB_AUTH: &str = include_str!("../src/web_auth.rs");

const NSH_HOME_PAGE: &str = include_str!("../../../ui/nsh/src/pages/home.tsx");
const NSH_PROFILES_PAGE: &str = include_str!("../../../ui/nsh/src/pages/profiles.tsx");
const NSH_EDITOR_PAGE: &str = include_str!("../../../ui/nsh/src/pages/profile-editor.tsx");
const NSH_CHALLENGE_PAGE: &str = include_str!("../../../ui/nsh/src/pages/auth-challenge.tsx");
const NSH_STATS_PAGE: &str = include_str!("../../../ui/nsh/src/pages/statistics.tsx");
const NSH_LOGS_PAGE: &str = include_str!("../../../ui/nsh/src/pages/logs.tsx");
const NSH_SETTINGS_PAGE: &str = include_str!("../../../ui/nsh/src/pages/settings.tsx");
const NSH_DESKTOP_NAV: &str = include_str!("../../../ui/nsh/src/desktop-navigation.tsx");

fn contains_vpn_word(source: &str) -> bool {
    source
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("vpn"))
}

/// Checks string literals inside TSX source so internal command IDs (`"vpn.connect"`)
/// or identifiers do not false-positive while user-facing copy is strictly audited.
fn contains_vpn_word_in_user_copy(source: &str) -> bool {
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut current = String::new();
    for ch in source.chars() {
        if let Some(quote) = in_quote {
            if escaped {
                current.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_quote = None;
                let trimmed = current.trim();
                let is_internal_command = trimmed.starts_with("vpn.");
                if !is_internal_command && contains_vpn_word(trimmed) {
                    return true;
                }
                current.clear();
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' || ch == '`' {
            in_quote = Some(ch);
            current.clear();
        }
    }
    false
}

#[test]
fn startup_opens_the_nsh_surface_without_a_privacy_gate() {
    let removed_gate = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../entry/src/main/ets/common/PrivacyConsent.ets");

    assert!(!removed_gate.exists());
    assert!(INDEX_PAGE.contains("NshSurface"));
    assert!(INDEX_PAGE.contains("ContentSlot(this.surface.content)"));
    assert!(!INDEX_PAGE.contains("PrivacyConsent"));
    assert!(!INDEX_PAGE.contains("privacyAccepted"));
    assert!(!INDEX_PAGE.contains("agreeAndContinue"));
    assert!(!INDEX_PAGE.contains("declineAndExit"));
    assert!(!INDEX_PAGE.contains("grantPrivacyConsent"));
    assert!(!INDEX_PAGE.contains("hasPrivacyConsent"));
}

#[test]
fn platform_identity_is_configured_without_consent_state() {
    assert!(PLATFORM_IDENTITY.contains("export function configureNativePlatformIdentity("));
    assert!(PLATFORM_IDENTITY.contains("deviceInfo.ODID"));
    assert_eq!(PLATFORM_IDENTITY.matches("    deviceInfo.ODID,").count(), 1);
    assert!(!PLATFORM_IDENTITY.contains("PrivacyConsent"));
    assert!(!PLATFORM_IDENTITY.contains("hasPrivacyConsent"));
    assert!(!PLATFORM_IDENTITY.contains("IfConsented"));

    for startup_source in [ENTRY_ABILITY, TUNNEL_ABILITY] {
        assert!(startup_source.contains("configureNativePlatformIdentity(this.context"));
        assert!(!startup_source.contains("IfConsented"));
        assert!(!startup_source.contains("privacy policy consent is required"));
    }
}

#[test]
fn user_visible_copy_does_not_contain_the_vpn_term() {
    for (name, source) in [
        ("ArkUI entry page", INDEX_PAGE),
        ("package description", PACKAGE_MANIFEST),
        ("base string resources", BASE_STRINGS),
    ] {
        assert!(!contains_vpn_word(source), "{name} contains the VPN term");
    }

    for (name, source) in [
        ("Home page", NSH_HOME_PAGE),
        ("Profiles page", NSH_PROFILES_PAGE),
        ("Profile editor page", NSH_EDITOR_PAGE),
        ("Auth challenge page", NSH_CHALLENGE_PAGE),
        ("Statistics page", NSH_STATS_PAGE),
        ("Logs page", NSH_LOGS_PAGE),
        ("Settings/About/Privacy page", NSH_SETTINGS_PAGE),
        ("Desktop navigation", NSH_DESKTOP_NAV),
    ] {
        assert!(
            !contains_vpn_word_in_user_copy(source),
            "{name} contains the VPN term in user-visible copy"
        );
    }
}

#[test]
fn dynamic_text_is_sanitized_at_the_application_observation_boundary() {
    assert!(APP_SANITIZE.contains("pub fn sanitize_display_text"));
    assert!(APP_SANITIZE.contains("normalized.match_indices(\"vpn\")"));
    assert!(APP_OBSERVATION.contains("sanitize_auth_challenge"));
    assert!(APP_OBSERVATION.contains("sanitize_diagnostic_entry"));
    assert!(APP_OBSERVATION.contains("sanitize_optional"));
}

#[test]
fn interactive_sso_uses_upstream_native_auth_session_and_browser_bridge() {
    assert!(WEB_AUTH.contains("set_external_browser_handler"));
    assert!(UI_LIB.contains("register_external_browser_handler"));
    assert!(HOST_SERVICES.contains("NativeAuthSession"));
    assert!(HOST_SERVICES.contains("from '@nsh/harmonyos'"));
    assert!(HOST_SERVICES.contains("registerExternalBrowserHandler"));
}

#[test]
fn app_home_enables_native_openconnect_progress_logging_in_both_processes() {
    assert_eq!(UI_LIB.matches("set_var(\"HANYCONNECT_HOME\"").count(), 2);
}
