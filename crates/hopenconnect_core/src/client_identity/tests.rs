use super::*;

#[test]
fn fallback_identity_is_openharmony() {
    let identity = PlatformIdentity::default();

    assert_eq!(
        identity.user_agent(),
        format!("AnyConnect OpenHarmony {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        identity.mobile_identity(),
        MobileIdentity {
            platform_version: "unknown".to_owned(),
            device_type: "OpenHarmony".to_owned(),
            unique_id: String::new(),
        }
    );
    assert_eq!(default_client_version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn runtime_identity_uses_real_openharmony_version_and_device_type() {
    let identity = PlatformIdentity::from_platform(
        "OpenHarmony-6.0.0.46",
        "6.0",
        "20",
        "phone",
        "1.2.3",
        "dff3cdfd-7beb-1e7d-fdf7-1dbfddd7d30c",
    );

    assert_eq!(identity.user_agent(), "AnyConnect OpenHarmony 1.2.3");
    assert_eq!(identity.client_version(), "1.2.3");
    assert_eq!(
        identity.mobile_identity(),
        MobileIdentity {
            platform_version: "6.0.0.46".to_owned(),
            device_type: "phone".to_owned(),
            unique_id: "dff3cdfd-7beb-1e7d-fdf7-1dbfddd7d30c".to_owned(),
        }
    );
}

#[test]
fn runtime_identity_has_stable_fallbacks_and_sanitizes_headers() {
    let identity = PlatformIdentity::from_platform(
        "",
        "6.0",
        "20",
        "phone;bad",
        "1.2.3\r\nInjected:yes",
        "odid\r\nInjected:yes",
    );
    assert_eq!(
        identity.user_agent(),
        "AnyConnect OpenHarmony 1.2.3Injectedyes"
    );
    assert_eq!(identity.mobile_identity().device_type, "phone bad");
    assert_eq!(identity.mobile_identity().unique_id, "odidInjected:yes");

    let api_only = PlatformIdentity::from_platform("", "", "20", "", "", "");
    assert_eq!(
        api_only.user_agent(),
        format!("AnyConnect OpenHarmony {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(api_only.mobile_identity().platform_version, "API 20");
}

#[test]
fn openharmony_passes_through_as_the_real_openconnect_device_id() {
    assert_eq!(openconnect_reported_os("OpenHarmony"), "OpenHarmony");
    assert_eq!(openconnect_reported_os("openharmony"), "OpenHarmony");
    assert_eq!(openconnect_reported_os(""), "OpenHarmony");
    assert_eq!(openconnect_reported_os("linux"), "linux");
}
