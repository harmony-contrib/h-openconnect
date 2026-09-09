use std::path::Path;

const ENTRY_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/entryability/EntryAbility.ets");
const TUNNEL_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/vpnability/HOpenConnectVpnExtensionAbility.ets");
const INDEX_PAGE: &str = include_str!("../../../entry/src/main/ets/pages/Index.ets");
const PLATFORM_IDENTITY: &str =
    include_str!("../../../entry/src/main/ets/common/PlatformIdentity.ets");
const EN_US: &str = include_str!("../locales/en-US.ftl");
const ZH_CN: &str = include_str!("../locales/zh-CN.ftl");
const PACKAGE_MANIFEST: &str = include_str!("../../../oh-package.json5");
const UI_MODEL: &str = include_str!("../src/model.rs");
const UI_STATE: &str = include_str!("../src/state.rs");
const UI_LIB: &str = include_str!("../src/lib.rs");
const HOME_PAGE: &str = include_str!("../src/view/pages/home.rs");
const CONNECTIONS_PAGE: &str = include_str!("../src/view/pages/connections.rs");
const CHALLENGE_PAGE: &str = include_str!("../src/view/pages/challenge.rs");
const LOGS_PAGE: &str = include_str!("../src/view/pages/logs.rs");

fn contains_vpn_word(source: &str) -> bool {
    source
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("vpn"))
}

#[test]
fn startup_opens_the_native_ui_without_a_privacy_gate() {
    let removed_gate = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../entry/src/main/ets/common/PrivacyConsent.ets");

    assert!(!removed_gate.exists());
    assert_eq!(INDEX_PAGE.matches("DefaultXComponent({").count(), 1);
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
        ("English locale", EN_US),
        ("Chinese locale", ZH_CN),
        ("ArkUI entry page", INDEX_PAGE),
        ("package description", PACKAGE_MANIFEST),
    ] {
        assert!(!contains_vpn_word(source), "{name} contains the VPN term");
    }

    assert!(!EN_US.to_ascii_lowercase().contains("privacy consent"));
    assert!(!ZH_CN.contains("同意隐私政策"));
}

#[test]
fn dynamic_text_is_sanitized_at_every_visible_error_boundary() {
    assert!(UI_MODEL.contains("pub fn sanitize_display_text"));
    assert!(UI_MODEL.contains("normalized.match_indices(\"vpn\")"));
    assert!(UI_STATE.contains("let message = sanitize_display_text(&message);"));
    assert!(HOME_PAGE.contains("content: sanitize_display_text(&error)"));
    assert!(CONNECTIONS_PAGE.contains("content: sanitize_display_text(&error)"));
    assert!(CHALLENGE_PAGE.matches("sanitize_display_text").count() >= 2);
    assert!(LOGS_PAGE.contains("let message = sanitize_display_text(&log.message);"));
}

#[test]
fn interactive_sso_registers_a_direct_ui_browser_handler() {
    let set_app = UI_LIB.find("bridge::set_app(handle)").expect("UI bridge");
    let register = UI_LIB
        .find("set_external_browser_handler")
        .expect("OpenConnect browser handler");
    assert!(set_app < register);
    assert!(UI_LIB.contains("bridge::open_external_browser_blocking(uri.to_owned()).is_ok()"));
}

#[test]
fn app_home_enables_native_openconnect_progress_logging_in_both_processes() {
    assert_eq!(UI_LIB.matches("set_var(\"HANYCONNECT_HOME\"").count(), 2);
}
