#[cfg(any(feature = "native-anyconnect", test))]
const OPENHARMONY_REPORTED_OS: &str = "OpenHarmony";
pub const DEFAULT_ANYCONNECT_VERSION: &str = "4.10.07061";
pub const DEFAULT_ANYCONNECT_USER_AGENT: &str = "AnyConnect Android 4.10.07061";
#[cfg(any(feature = "native-anyconnect", test))]
const OPENCONNECT_OPENHARMONY_OS: &str = "android";

pub fn default_user_agent() -> String {
    DEFAULT_ANYCONNECT_USER_AGENT.to_owned()
}

pub fn default_client_version() -> String {
    DEFAULT_ANYCONNECT_VERSION.to_owned()
}

#[cfg(any(feature = "native-anyconnect", test))]
pub fn openconnect_reported_os(configured: &str) -> &str {
    let configured = configured.trim();
    if configured.is_empty() || configured.eq_ignore_ascii_case(OPENHARMONY_REPORTED_OS) {
        OPENCONNECT_OPENHARMONY_OS
    } else {
        configured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_defaults_use_the_stable_anyconnect_android_identity() {
        assert_eq!(default_user_agent(), "AnyConnect Android 4.10.07061");
        assert_eq!(default_client_version(), "4.10.07061");
        assert!(default_user_agent().starts_with("AnyConnect"));
    }

    #[test]
    fn openharmony_maps_to_openconnect_android_device_id() {
        assert_eq!(openconnect_reported_os("OpenHarmony"), "android");
        assert_eq!(openconnect_reported_os(""), "android");
        assert_eq!(openconnect_reported_os("linux"), "linux");
    }
}
