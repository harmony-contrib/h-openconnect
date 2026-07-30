use std::sync::{OnceLock, RwLock};

pub(crate) const OPENHARMONY_REPORTED_OS: &str = "OpenHarmony";
const ANYCONNECT_USER_AGENT_PREFIX: &str = "AnyConnect OpenHarmony";

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlatformIdentity {
    os_version: String,
    device_type: String,
    app_version: String,
    unique_id: String,
}

impl Default for PlatformIdentity {
    fn default() -> Self {
        Self {
            os_version: String::new(),
            device_type: String::new(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            unique_id: String::new(),
        }
    }
}

#[cfg(any(feature = "native-anyconnect", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MobileIdentity {
    pub platform_version: String,
    pub device_type: String,
    pub unique_id: String,
}

static PLATFORM_IDENTITY: OnceLock<RwLock<PlatformIdentity>> = OnceLock::new();

fn platform_identity() -> &'static RwLock<PlatformIdentity> {
    PLATFORM_IDENTITY.get_or_init(|| RwLock::new(PlatformIdentity::default()))
}

/// Supply the OpenHarmony identity obtained from platform and bundle metadata.
///
/// The native library may run in both the UI and VPN-extension processes, so
/// each process calls this during ability startup before creating a client.
pub fn configure_platform_identity(
    os_full_name: String,
    display_version: String,
    sdk_api_version: String,
    device_type: String,
    app_version: String,
    unique_id: String,
) {
    let identity = PlatformIdentity::from_platform(
        &os_full_name,
        &display_version,
        &sdk_api_version,
        &device_type,
        &app_version,
        &unique_id,
    );
    match platform_identity().write() {
        Ok(mut current) => *current = identity,
        Err(poisoned) => *poisoned.into_inner() = identity,
    }
}

pub fn default_user_agent() -> String {
    current_platform_identity().user_agent()
}

pub fn default_client_version() -> String {
    current_platform_identity().client_version()
}

#[cfg(any(feature = "native-anyconnect", test))]
pub fn openconnect_reported_os(configured: &str) -> &str {
    let configured = configured.trim();
    if configured.is_empty() || configured.eq_ignore_ascii_case(OPENHARMONY_REPORTED_OS) {
        OPENHARMONY_REPORTED_OS
    } else {
        configured
    }
}

#[cfg(feature = "native-anyconnect")]
pub(crate) fn mobile_identity() -> MobileIdentity {
    current_platform_identity().mobile_identity()
}

fn current_platform_identity() -> PlatformIdentity {
    match platform_identity().read() {
        Ok(current) => current.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

impl PlatformIdentity {
    fn from_platform(
        os_full_name: &str,
        display_version: &str,
        sdk_api_version: &str,
        device_type: &str,
        app_version: &str,
        unique_id: &str,
    ) -> Self {
        let os_full_name = sanitized_header_component(os_full_name);
        let display_version = sanitized_header_component(display_version);
        let sdk_api_version = sanitized_header_component(sdk_api_version);
        let device_type = sanitized_header_component(device_type);
        let app_version = sanitized_version(app_version);
        let unique_id = sanitized_identifier(unique_id);

        let os_version = openharmony_version(&os_full_name)
            .filter(|version| !version.is_empty())
            .or_else(|| (!display_version.is_empty()).then_some(display_version))
            .or_else(|| (!sdk_api_version.is_empty()).then(|| format!("API {sdk_api_version}")))
            .unwrap_or_default();

        Self {
            os_version,
            device_type,
            app_version: if app_version.is_empty() {
                env!("CARGO_PKG_VERSION").to_owned()
            } else {
                app_version
            },
            unique_id,
        }
    }

    fn user_agent(&self) -> String {
        format!("{ANYCONNECT_USER_AGENT_PREFIX} {}", self.app_version)
    }

    fn client_version(&self) -> String {
        self.app_version.clone()
    }

    #[cfg(any(feature = "native-anyconnect", test))]
    fn mobile_identity(&self) -> MobileIdentity {
        MobileIdentity {
            platform_version: if self.os_version.is_empty() {
                "unknown".to_owned()
            } else {
                self.os_version.clone()
            },
            device_type: if self.device_type.is_empty() {
                OPENHARMONY_REPORTED_OS.to_owned()
            } else {
                self.device_type.clone()
            },
            unique_id: self.unique_id.clone(),
        }
    }
}

fn openharmony_version(os_full_name: &str) -> Option<String> {
    let prefix = OPENHARMONY_REPORTED_OS;
    let starts_with_prefix = os_full_name
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix));
    if starts_with_prefix {
        Some(
            os_full_name[prefix.len()..]
                .trim_start_matches(|character: char| {
                    character.is_whitespace() || matches!(character, '-' | '_' | '/')
                })
                .to_owned(),
        )
    } else if os_full_name.is_empty() {
        None
    } else {
        Some(os_full_name.to_owned())
    }
}

fn sanitized_header_component(raw: &str) -> String {
    raw.chars()
        .map(|character| {
            if character.is_control() || matches!(character, '(' | ')' | ';' | '<' | '>') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(96)
        .collect()
}

fn sanitized_version(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+')
        })
        .take(64)
        .collect()
}

fn sanitized_identifier(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ':')
        })
        .take(128)
        .collect()
}

#[cfg(test)]
mod tests {
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
}
