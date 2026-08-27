const ENTRY_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/entryability/EntryAbility.ets");
const VPN_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/vpnability/HOpenConnectVpnExtensionAbility.ets");
const INDEX_PAGE: &str = include_str!("../../../entry/src/main/ets/pages/Index.ets");
const PLATFORM_IDENTITY: &str =
    include_str!("../../../entry/src/main/ets/common/PlatformIdentity.ets");
const PRIVACY_CONSENT: &str = include_str!("../../../entry/src/main/ets/common/PrivacyConsent.ets");

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("section start");
    let tail = &source[start..];
    let end = tail.find(end).expect("section end");
    &tail[..end]
}

#[test]
fn odid_read_is_owned_by_the_consent_guard() {
    assert!(PLATFORM_IDENTITY.contains("if (!hasPrivacyConsent(context))"));
    assert!(PLATFORM_IDENTITY.contains("deviceInfo.ODID"));
    assert_eq!(PLATFORM_IDENTITY.matches("    deviceInfo.ODID,").count(), 1);
    assert!(!ENTRY_ABILITY.contains("deviceInfo.ODID"));
    assert!(!VPN_ABILITY.contains("deviceInfo.ODID"));
    assert!(!INDEX_PAGE.contains("deviceInfo.ODID"));
}

#[test]
fn startup_and_vpn_extension_share_the_versioned_consent_gate() {
    assert!(PRIVACY_CONSENT.contains("CURRENT_PRIVACY_POLICY_VERSION"));
    assert!(PRIVACY_CONSENT.contains("acceptedPrivacyPolicyVersion"));
    assert!(ENTRY_ABILITY.contains("configureNativePlatformIdentityIfConsented"));
    assert!(VPN_ABILITY.contains("configureNativePlatformIdentityIfConsented"));
    assert!(VPN_ABILITY.contains("privacy policy consent is required"));

    let agreement = section(
        INDEX_PAGE,
        "private agreeAndContinue",
        "private showPrivacySaveError",
    );
    let grant = agreement
        .find("grantPrivacyConsent")
        .expect("persist consent");
    let identity = agreement
        .find("configureNativePlatformIdentityIfConsented")
        .expect("configure identity");
    let accepted = agreement
        .find("this.privacyAccepted = true")
        .expect("open main interface");
    assert!(grant < identity && identity < accepted);

    let build = section(INDEX_PAGE, "  build()", "  @Builder");
    let accepted_branch = section(build, "if (this.privacyAccepted)", "} else");
    assert!(accepted_branch.contains("DefaultXComponent"));
}
