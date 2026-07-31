use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionLifecycle {
    #[default]
    Disconnected,
    Connecting,
    Authenticating,
    Establishing,
    Connected,
    Disconnecting,
    Failed,
}

impl ConnectionLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Authenticating => "authenticating",
            Self::Establishing => "establishing",
            Self::Connected => "connected",
            Self::Disconnecting => "disconnecting",
            Self::Failed => "failed",
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::Authenticating | Self::Establishing | Self::Disconnecting
        )
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Connected)
    }
}

/// Wire protocol for OpenConnect (ics-openconnect `vpn_protocol` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolKind {
    #[default]
    AnyConnect,
    /// Legacy UI label; maps to AnyConnect.
    Ssl,
    /// Legacy UI label; OpenConnect has no pure IPsec — still AnyConnect.
    Ipsec,
    Juniper,
    GlobalProtect,
    Pulse,
    F5,
    Fortinet,
    Array,
}

impl ProtocolKind {
    pub fn as_openconnect(self) -> &'static str {
        match self {
            Self::AnyConnect | Self::Ssl | Self::Ipsec => "anyconnect",
            Self::Juniper => "nc",
            Self::GlobalProtect => "gp",
            Self::Pulse => "pulse",
            Self::F5 => "f5",
            Self::Fortinet => "fortinet",
            Self::Array => "array",
        }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            Self::AnyConnect | Self::Ssl => "AnyConnect",
            Self::Ipsec => "IPsec",
            Self::Juniper => "Juniper NC",
            Self::GlobalProtect => "GlobalProtect",
            Self::Pulse => "Pulse",
            Self::F5 => "F5",
            Self::Fortinet => "Fortinet",
            Self::Array => "Array",
        }
    }

    pub fn from_label(label: &str) -> Self {
        let l = label.to_ascii_lowercase();
        match l.as_str() {
            "ipsec" => Self::Ipsec,
            "nc" | "juniper" | "juniper nc" => Self::Juniper,
            "gp" | "globalprotect" => Self::GlobalProtect,
            "pulse" => Self::Pulse,
            "f5" => Self::F5,
            "fortinet" => Self::Fortinet,
            "array" => Self::Array,
            "ssl" | "anyconnect" => Self::AnyConnect,
            _ => Self::AnyConnect,
        }
    }

    pub fn all() -> &'static [Self] {
        // The app product and its compatibility matrix are AnyConnect-only.
        // Keep deserialization support for migrated OpenConnect profiles, but
        // do not advertise unverified vendor protocols in production UI.
        &[Self::AnyConnect]
    }
}

/// ics-openconnect `software_token` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareToken {
    #[default]
    Disabled,
    SecurId,
    Totp,
}

impl SoftwareToken {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::SecurId => "RSA SecurID",
            Self::Totp => "TOTP",
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.to_ascii_lowercase().as_str() {
            "securid" | "rsa securid" | "stoken" => Self::SecurId,
            "totp" => Self::Totp,
            _ => Self::Disabled,
        }
    }
}

/// ics-openconnect `split_tunnel_mode` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SplitTunnelMode {
    /// Use server split-includes; default route if none.
    #[default]
    Auto,
    /// Custom networks; DNS via VPN.
    OnVpnDns,
    /// Custom networks; DNS via uplink (no VPN DNS).
    OnUplinkDns,
}

impl SplitTunnelMode {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::OnVpnDns => "Split + VPN DNS",
            Self::OnUplinkDns => "Split + uplink DNS",
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.to_ascii_lowercase().as_str() {
            "on_vpn_dns" | "split + vpn dns" | "onvpndns" => Self::OnVpnDns,
            "on_uplink_dns" | "split + uplink dns" | "onuplinkdns" => Self::OnUplinkDns,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    #[default]
    Password,
    Certificate,
    PasswordAndCertificate,
    Saml,
}

impl AuthMethod {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Password => "Password",
            Self::Certificate => "Certificate",
            Self::PasswordAndCertificate => "Password+Cert",
            Self::Saml => "SAML",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub server: String,
    pub group: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default)]
    pub protocol: ProtocolKind,
    pub auth_method: AuthMethod,
    /// Client certificate path (PEM/P12) or alias.
    pub certificate: String,
    /// Optional separate private key path (ics `private_key`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub private_key: String,
    /// Secondary user certificate for Cisco multiple-certificate authentication.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_private_key: String,
    /// Optional CA file path (ics `ca_certificate`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca_certificate: String,
    /// PKCS#12 / encrypted PEM passphrase persisted with the local profile.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_key_password: String,
    /// HTTP proxy URL for CSTP (openconnect `--proxy` / ics `http_proxy`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_proxy: String,
    /// Peer cert pin: pin-sha256:… / sha1:… / sha256:… (openconnect `--servercert`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_cert_hash: String,
    pub backup_servers: String,
    #[serde(default = "default_true")]
    pub strict_certificate_trust: bool,
    #[serde(default = "default_true")]
    pub block_untrusted_servers: bool,
    pub allow_local_lan: bool,
    /// Ignore server split-include and push a default route so all IPv4 traffic
    /// goes through the VPN (plus system VPN DNS).
    #[serde(default)]
    pub force_global: bool,
    /// ics `split_tunnel_mode` (used when `force_global` is false).
    #[serde(default)]
    pub split_tunnel_mode: SplitTunnelMode,
    /// Comma/space separated CIDRs for custom split (ics `split_tunnel_networks`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub split_tunnel_networks: String,
    pub connect_on_demand: bool,
    pub external_browser_auth: bool,
    pub fips_mode: bool,
    /// Explicit OpenConnect `--allow-insecure-crypto`; independent from
    /// certificate trust and disabled by default.
    #[serde(default)]
    pub allow_insecure_crypto: bool,
    /// ics `use_dtls` (default true).
    #[serde(default = "default_true")]
    pub use_dtls: bool,
    /// AnyConnect protocol platform sent as the XML `device-id`.
    #[serde(default = "default_reported_os")]
    pub reported_os: String,
    /// Optional exact User-Agent override (empty = runtime OpenHarmony identity).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_agent: String,
    /// Version reported in AnyConnect XML (empty = runtime application version).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_version: String,
    /// TLS SNI override (ics `sni`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sni: String,
    /// ics `require_pfs`.
    #[serde(default)]
    pub require_pfs: bool,
    /// ics `disable_xml_post`.
    #[serde(default)]
    pub disable_xml_post: bool,
    /// Dead-peer detection seconds; 0 = protocol default (ics `dpd_value`).
    #[serde(default)]
    pub dpd_seconds: u32,
    /// ics software token mode.
    #[serde(default)]
    pub software_token: SoftwareToken,
    /// Token secret / string (ics `token_string`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_string: String,
    /// Optional CSD wrapper script path (ics `custom_csd_wrapper`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub csd_wrapper: String,
    /// OHOS VpnConfig.trustedApplications (package names; empty = all apps).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trusted_applications: String,
    /// OHOS VpnConfig.blockedApplications (package names excluded from VPN).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blocked_applications: String,
    pub mtu: u32,
    pub favorite: bool,
}

fn default_true() -> bool {
    true
}

fn default_reported_os() -> String {
    "OpenHarmony".to_owned()
}

/// Private / link-local IPv4 prefixes kept off-tunnel when allow_local_lan.
const RFC1918_EXCLUDES: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
];

fn split_package_list(raw: &str) -> Vec<String> {
    raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn split_server_list(raw: &str) -> Vec<String> {
    raw.split(|character: char| character.is_whitespace() || character == ',' || character == ';')
        .map(str::trim)
        .filter(|server| !server.is_empty())
        .map(|server| {
            if server.starts_with("https://") || server.starts_with("http://") {
                server.to_owned()
            } else {
                format!("https://{server}")
            }
        })
        .collect()
}

impl ConnectionProfile {
    pub fn new_draft() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            server: String::new(),
            group: String::new(),
            username: String::new(),
            password: String::new(),
            protocol: ProtocolKind::AnyConnect,
            auth_method: AuthMethod::Password,
            certificate: String::new(),
            private_key: String::new(),
            secondary_certificate: String::new(),
            secondary_private_key: String::new(),
            ca_certificate: String::new(),
            key_password: String::new(),
            secondary_key_password: String::new(),
            http_proxy: String::new(),
            server_cert_hash: String::new(),
            backup_servers: String::new(),
            strict_certificate_trust: true,
            block_untrusted_servers: true,
            allow_local_lan: false,
            force_global: false,
            split_tunnel_mode: SplitTunnelMode::Auto,
            split_tunnel_networks: String::new(),
            connect_on_demand: false,
            external_browser_auth: false,
            fips_mode: false,
            allow_insecure_crypto: false,
            use_dtls: true,
            reported_os: default_reported_os(),
            user_agent: String::new(),
            client_version: String::new(),
            sni: String::new(),
            require_pfs: false,
            disable_xml_post: false,
            dpd_seconds: 0,
            software_token: SoftwareToken::Disabled,
            token_string: String::new(),
            csd_wrapper: String::new(),
            trusted_applications: String::new(),
            blocked_applications: String::new(),
            mtu: 0,
            favorite: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("connection name is required".to_owned());
        }
        let server = self.server.trim();
        if server.is_empty() {
            return Err("server address is required".to_owned());
        }
        if server.chars().any(char::is_whitespace) {
            return Err("server address must not contain whitespace".to_owned());
        }
        let normalized = self.server_url();
        let authority = normalized
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(normalized.as_str())
            .split('/')
            .next()
            .unwrap_or_default();
        if authority.is_empty() {
            return Err("server address has no host".to_owned());
        }
        if self.mtu != 0 && !(576..=1500).contains(&self.mtu) {
            return Err("MTU must be between 576 and 1500, or automatic".to_owned());
        }
        if self.dpd_seconds > 86_400 {
            return Err("DPD interval must not exceed 86400 seconds".to_owned());
        }
        for route in self
            .split_tunnel_networks
            .split([',', ' ', '\n', ';'])
            .map(str::trim)
            .filter(|route| !route.is_empty())
        {
            if normalize_route_cidr(route).is_none() {
                return Err(format!("invalid split-tunnel network: {route}"));
            }
        }
        let trusted = split_package_list(&self.trusted_applications);
        let blocked = split_package_list(&self.blocked_applications);
        if let Some(package) = trusted
            .iter()
            .find(|package| blocked.iter().any(|blocked| blocked == *package))
        {
            return Err(format!(
                "application {package} cannot be both trusted and blocked"
            ));
        }
        Ok(())
    }

    pub fn server_url(&self) -> String {
        let server = self.server.trim();
        if server.starts_with("https://") || server.starts_with("http://") {
            server.to_owned()
        } else if server.is_empty() {
            String::new()
        } else {
            format!("https://{server}")
        }
    }

    pub fn summary_auth(&self) -> String {
        match self.auth_method {
            AuthMethod::Password => {
                if self.username.is_empty() {
                    "Password".to_owned()
                } else {
                    format!("Password · {}", self.username)
                }
            }
            AuthMethod::Certificate => {
                if self.certificate.is_empty() {
                    "Certificate".to_owned()
                } else {
                    format!("Cert · {}", self.certificate)
                }
            }
            AuthMethod::PasswordAndCertificate => "Password + Certificate".to_owned(),
            AuthMethod::Saml => "SAML".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub connected_seconds: u64,
    pub assigned_ip: String,
    pub gateway: String,
    pub mtu: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshot {
    pub address: Option<String>,
    pub netmask: Option<String>,
    /// IPv6 address when headend pushes one (ics addAddress v6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_v6: Option<String>,
    /// IPv6 prefix or `addr/prefix` string from OpenConnect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netmask_v6: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub mtu: i32,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub split_excludes: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
    /// AnyConnect split-DNS suffixes, in headend order.
    #[serde(default)]
    pub split_dns: Vec<String>,
}

/// Kind of an OpenConnect auth-form field surfaced to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthFieldKind {
    #[default]
    Text,
    Password,
    Select,
    Token,
    /// Hidden options stay server-owned; never rendered.
    Hidden,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthFieldChoice {
    pub name: String,
    pub label: String,
}

/// Authentication groups advertised by the headend's initial AnyConnect form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthGroupDiscovery {
    /// Exact protocol value of the server-selected group, when present.
    #[serde(default)]
    pub selected: Option<String>,
    /// Display labels plus their exact protocol values.
    #[serde(default)]
    pub groups: Vec<AuthFieldChoice>,
}

/// Stable identity of one option in a server authentication form.
///
/// Option names are not unique in the OpenConnect protocol. The form id,
/// original option ordinal, and a value-free structural digest together bind
/// a UI answer to the exact option that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthFieldKey {
    #[serde(default)]
    pub form_id: Option<String>,
    #[serde(default)]
    pub option_index: u32,
    #[serde(default)]
    pub option_digest: String,
}

/// One input on an authentication form / challenge page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthField {
    /// Exact server option identity used for draft storage and reply binding.
    #[serde(default)]
    pub key: AuthFieldKey,
    pub name: String,
    pub label: String,
    pub kind: AuthFieldKind,
    /// Prefill (username/password from profile, or prior value).
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub choices: Vec<AuthFieldChoice>,
    /// The protocol-designated authentication-group selector. Connection-time
    /// challenges keep this server-selected field out of user-editable values.
    #[serde(default)]
    pub auth_group: bool,
    /// OpenConnect's `OC_FORM_OPT_SECOND_AUTH` marker. Primary profile
    /// credentials must never be assigned to a field carrying this protocol
    /// flag; clients must not infer OTP/SMS semantics from its name or label.
    #[serde(default)]
    pub second_auth: bool,
    /// True when the UI must collect this field (empty required interactive).
    #[serde(default)]
    pub required: bool,
}

/// A single OpenConnect authentication form awaiting user input.
///
/// OpenConnect may present multiple challenges in sequence; each round gets a
/// new `id`. The auth worker blocks until [`AuthChallengeReply`] arrives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthChallenge {
    pub id: u64,
    pub round: u32,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub form_id: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    pub fields: Vec<AuthField>,
}

/// User response to a pending [`AuthChallenge`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthChallengeReply {
    pub id: u64,
    /// Values in the same order as the visible challenge options.
    #[serde(default)]
    pub values: Vec<AuthFieldValue>,
    #[serde(default)]
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthFieldValue {
    /// Exact option identity copied from [`AuthField::key`].
    pub key: AuthFieldKey,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub lifecycle: ConnectionLifecycle,
    pub active_connection_id: Option<String>,
    pub connections: Vec<ConnectionProfile>,
    pub stats: SessionStats,
    pub network: NetworkSnapshot,
    pub last_error: Option<String>,
    pub diagnostics: Vec<DiagnosticEntry>,
    pub app_version: String,
    pub sdk_ready: bool,
    pub anyconnect_version: Option<String>,
    pub backend: String,
    /// Present while OpenConnect is blocked on an interactive auth form.
    #[serde(default)]
    pub pending_auth: Option<AuthChallenge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VpnOptions {
    pub addresses: Vec<String>,
    pub routes: Vec<String>,
    /// Split-exclude prefixes (OHOS RouteInfo.isExcludedRoute when supported).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_routes: Vec<String>,
    pub dns_addresses: Vec<String>,
    /// DNS search domains for the system resolver (HarmonyOS VpnConfig.searchDomains).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_domains: Vec<String>,
    pub mtu: u32,
    pub allow_bypass: bool,
    /// When true, system VPN uses a full default route regardless of split-include.
    #[serde(default)]
    pub force_global: bool,
    /// Handoff fields for the isolated VPN-extension process (same UID, new process).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// OpenConnect cookie from UI-process auth; extension reuses it for CSTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default)]
    pub accept_untrusted: bool,
    /// Whether the authentication request may advertise browser-based SSO.
    #[serde(default)]
    pub external_auth_allowed: bool,
    /// Stable privacy-scoped identifier reported as AnyConnect mobile metadata.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mobile_unique_id: String,
    /// Ordered failover URLs from the profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup_servers: Vec<String>,
    // --- prefs mirrored for extension rebuild (ics setPreferences) ---
    #[serde(default = "default_true")]
    pub use_dtls: bool,
    #[serde(default = "default_reported_os")]
    pub reported_os: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sni: String,
    #[serde(default)]
    pub require_pfs: bool,
    #[serde(default)]
    pub disable_xml_post: bool,
    #[serde(default)]
    pub dpd_seconds: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vpn_protocol: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_version: String,
    #[serde(default)]
    pub allow_insecure_crypto: bool,
    #[serde(default)]
    pub fips_mode: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca_certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub private_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_private_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secondary_key_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_proxy: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_cert_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub csd_wrapper: String,
    #[serde(default)]
    pub software_token: SoftwareToken,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_string: String,
    #[serde(default)]
    pub split_tunnel_mode: SplitTunnelMode,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub split_tunnel_networks: String,
    /// Package names for OHOS VpnConfig.trustedApplications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_applications: Vec<String>,
    /// Package names for OHOS VpnConfig.blockedApplications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_applications: Vec<String>,
}

impl VpnOptions {
    /// Build system VPN options from OpenConnect network info.
    ///
    /// Routing / DNS follows [ics-openconnect](https://gitlab.com/openconnect/ics-openconnect)
    /// `OpenConnectManagementThread.setIPInfo`:
    /// 1. `addAddress(addr, netmask)` — CIDR from headend (not forced /32)
    /// 2. Routes = split-includes, or `0.0.0.0/0` when forced / empty IPv4 splits
    /// 3. **Always** `addDnsServer` + host route `/32` for each DNS
    /// 4. **Always** search domain when the headend pushes one
    pub fn from_network(network: &NetworkSnapshot, profile: &ConnectionProfile) -> Self {
        let host = network
            .address
            .as_deref()
            .and_then(|address| address.split('/').next())
            .map(str::trim)
            .filter(|address| !address.is_empty());

        // ics: CIDRIP(ip.addr, ip.netmask). OpenHarmony VpnConfig ParseAddress
        // rejects IPv4 prefixLength >= 32 (see communication_netmanager_ext).
        let mut addresses = Vec::new();
        if let Some(host) = host {
            if host.contains(':') {
                addresses.push(format!("{host}/128"));
            } else {
                let mut prefix = network
                    .netmask
                    .as_deref()
                    .and_then(ipv4_netmask_prefix)
                    .unwrap_or(24);
                if prefix == 0 {
                    prefix = 24;
                }
                if prefix >= 32 {
                    prefix = 31;
                }
                addresses.push(format!("{host}/{prefix}"));
            }
        }
        // ics IPv6 address when present.
        if let Some(v6) = network
            .address_v6
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(mask6) = network.netmask_v6.as_deref() {
                if mask6.contains('/') {
                    addresses.push(mask6.trim().to_owned());
                } else if let Ok(bits) = mask6.trim().parse::<u8>() {
                    addresses.push(format!("{v6}/{bits}"));
                } else {
                    addresses.push(format!("{v6}/64"));
                }
            } else if v6.contains('/') {
                addresses.push(v6.to_owned());
            } else {
                addresses.push(format!("{v6}/64"));
            }
        }

        let has_ipv6 = network
            .address_v6
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
            || addresses.iter().any(|a| a.contains(':'));

        // ics: min MTU 576 IPv4 / 1280 when IPv6 present (RFC 2460 / KK floor).
        let mtu = if network.mtu > 0 {
            network.mtu as u32
        } else if profile.mtu > 0 {
            profile.mtu
        } else {
            1400
        }
        .max(if has_ipv6 { 1280 } else { 576 });

        // ics uses exactly the resolvers supplied by the headend. When none
        // are supplied, leave the list empty so the platform keeps its
        // uplink resolver instead of inventing a DNS server inside the tunnel.
        let dns = network.dns.clone();

        // Cisco AnyConnect constructs the suffix list in this order:
        // default domain first, then every headend split-DNS suffix.
        let mut search_domains = Vec::new();
        if let Some(domain) = network.domain.as_deref() {
            append_search_domains(&mut search_domains, domain);
        }
        for domain in &network.split_dns {
            append_search_domains(&mut search_domains, domain);
        }

        // --- routing (ics setIPInfo + addDefaultRoutes + addSubnetRoutes) ---
        let server_split: Vec<String> = network
            .routes
            .iter()
            .filter_map(|r| normalize_route_cidr(r))
            .collect();
        let custom_split: Vec<String> = profile
            .split_tunnel_networks
            .split([',', ' ', '\n', ';'])
            .filter_map(|s| normalize_route_cidr(s.trim()))
            .collect();

        let (mut routes, dns_out, use_default) =
            match (profile.force_global, profile.split_tunnel_mode) {
                // ics full tunnel: default IPv4 (+ IPv6 when configured).
                (true, _) => {
                    let mut r = vec!["0.0.0.0/0".to_owned()];
                    if has_ipv6 {
                        r.push("::/0".to_owned());
                    }
                    (r, dns.clone(), true)
                }
                (false, SplitTunnelMode::OnVpnDns) if !custom_split.is_empty() => {
                    (custom_split, dns.clone(), false)
                }
                (false, SplitTunnelMode::OnUplinkDns) if !custom_split.is_empty() => {
                    // ics: custom routes but empty DNS list (use uplink resolver).
                    (custom_split, Vec::new(), false)
                }
                _ => {
                    // Auto: server includes; default when empty (ics addDefaultRoutes).
                    if server_split.is_empty() {
                        let mut r = vec!["0.0.0.0/0".to_owned()];
                        if has_ipv6 {
                            r.push("::/0".to_owned());
                        }
                        (r, dns.clone(), true)
                    } else {
                        (server_split, dns.clone(), false)
                    }
                }
            };

        // ics ALWAYS (when VPN DNS is used): addRoute(dns, /32 or /128).
        for server in &dns_out {
            let dns_host = server.split('%').next().unwrap_or(server).trim();
            if dns_host.is_empty() {
                continue;
            }
            let host_route = if dns_host.contains(':') {
                format!("{dns_host}/128")
            } else {
                format!("{dns_host}/32")
            };
            if !routes.iter().any(|r| r == &host_route) {
                routes.insert(0, host_route);
            }
        }

        let excluded_routes: Vec<String> = network
            .split_excludes
            .iter()
            .filter_map(|r| normalize_route_cidr(r))
            .collect();

        let accept_untrusted =
            !profile.strict_certificate_trust && !profile.block_untrusted_servers;
        let mut options = Self {
            addresses,
            routes,
            excluded_routes,
            dns_addresses: dns_out,
            search_domains: if matches!(profile.split_tunnel_mode, SplitTunnelMode::OnUplinkDns)
                && !profile.force_global
            {
                Vec::new()
            } else {
                search_domains
            },
            mtu,
            allow_bypass: profile.allow_local_lan && !use_default,
            force_global: use_default,
            server: Some(profile.server_url()),
            username: Some(profile.username.clone()),
            password: Some(profile.password.clone()),
            group: Some(profile.group.clone()),
            cookie: None,
            accept_untrusted,
            external_auth_allowed: profile.external_browser_auth
                || matches!(profile.auth_method, AuthMethod::Saml),
            mobile_unique_id: profile.id.clone(),
            backup_servers: split_server_list(&profile.backup_servers),
            use_dtls: profile.use_dtls,
            reported_os: profile.reported_os.clone(),
            sni: profile.sni.clone(),
            require_pfs: profile.require_pfs,
            disable_xml_post: profile.disable_xml_post,
            dpd_seconds: profile.dpd_seconds,
            vpn_protocol: profile.protocol.as_openconnect().to_owned(),
            user_agent: profile.user_agent.clone(),
            client_version: profile.client_version.clone(),
            allow_insecure_crypto: profile.allow_insecure_crypto,
            fips_mode: profile.fips_mode,
            ca_certificate: profile.ca_certificate.clone(),
            certificate: profile.certificate.clone(),
            private_key: profile.private_key.clone(),
            secondary_certificate: profile.secondary_certificate.clone(),
            secondary_private_key: profile.secondary_private_key.clone(),
            key_password: profile.key_password.clone(),
            secondary_key_password: profile.secondary_key_password.clone(),
            http_proxy: profile.http_proxy.clone(),
            server_cert_hash: profile.server_cert_hash.clone(),
            csd_wrapper: profile.csd_wrapper.clone(),
            software_token: profile.software_token,
            token_string: profile.token_string.clone(),
            split_tunnel_mode: profile.split_tunnel_mode,
            split_tunnel_networks: profile.split_tunnel_networks.clone(),
            trusted_applications: split_package_list(&profile.trusted_applications),
            blocked_applications: split_package_list(&profile.blocked_applications),
        };
        // OHOS has no Android allowBypass: approximate ics allow_local_lan by
        // excluding RFC1918 + link-local when the tunnel carries a default route.
        // (Under pure split-include, LAN is already off-tunnel unless listed.)
        if profile.allow_local_lan && use_default {
            for lan in RFC1918_EXCLUDES {
                if !options.excluded_routes.iter().any(|r| r == lan) {
                    options.excluded_routes.push((*lan).to_owned());
                }
            }
        }
        options.normalize_routes();
        options
    }

    /// Re-apply full-tunnel routes (ics force path): defaults + DNS host routes.
    pub fn apply_force_global(&mut self) {
        if !self.force_global {
            return;
        }
        let has_ipv6 = self.addresses.iter().any(|a| a.contains(':'));
        let mut routes = vec!["0.0.0.0/0".to_owned()];
        if has_ipv6 {
            routes.push("::/0".to_owned());
        }
        for s in &self.dns_addresses {
            let host = s.split('%').next().unwrap_or(s).trim();
            if host.is_empty() {
                continue;
            }
            let host_route = if host.contains(':') {
                format!("{host}/128")
            } else {
                format!("{host}/32")
            };
            if !routes.iter().any(|r| r == &host_route) {
                routes.insert(0, host_route);
            }
        }
        self.routes = routes;
        self.allow_bypass = false;
        self.normalize_routes();
    }

    /// Convert any dotted-netmask routes to CIDR; normalise network bits (ics CIDRIP).
    pub fn normalize_routes(&mut self) {
        self.routes = self
            .routes
            .iter()
            .filter_map(|r| normalize_route_cidr(r))
            .collect();
        let mut seen = std::collections::HashSet::new();
        self.routes.retain(|r| seen.insert(r.clone()));
    }
}

/// Accept `a.b.c.d/24`, `a.b.c.d/255.255.255.0`, or bare `a.b.c.d` → CIDR string.
/// Matches ics-openconnect `CIDRIP` (including network-bit normalisation).
pub fn normalize_route_cidr(route: &str) -> Option<String> {
    let route = route.trim();
    if route.is_empty() {
        return None;
    }
    let (ip, suffix) = match route.split_once('/') {
        Some((ip, suffix)) => (ip.trim(), suffix.trim()),
        None => return Some(format!("{route}/32")),
    };
    if ip.is_empty() {
        return None;
    }
    if ip.contains(':') {
        // IPv6: keep numeric prefix only
        let bits: u8 = if suffix.contains('.') {
            return None;
        } else {
            suffix.parse().ok()?
        };
        if bits > 128 {
            return None;
        }
        return Some(format!("{ip}/{bits}"));
    }
    let bits = if suffix.contains('.') {
        ipv4_netmask_prefix(suffix)?
    } else {
        let b: u8 = suffix.parse().ok()?;
        if b > 32 {
            return None;
        }
        b
    };
    // ics CIDRIP.normalise(): zero host bits for the network address
    let net = ipv4_network_cidr(ip, bits)?;
    Some(net)
}

fn append_search_domains(domains: &mut Vec<String>, value: &str) {
    for domain in value
        .split(&[',', ' ', ';'][..])
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
    {
        if !domains
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(domain))
        {
            domains.push(domain.to_owned());
        }
    }
}

fn ipv4_netmask_prefix(mask: &str) -> Option<u8> {
    let mut parts = mask.split('.');
    let mut value: u32 = 0;
    for _ in 0..4 {
        let octet: u8 = parts.next()?.parse().ok()?;
        value = (value << 8) | u32::from(octet);
    }
    if parts.next().is_some() {
        return None;
    }
    // Require a contiguous prefix mask.
    let leading = value.leading_ones();
    let trailing = value.trailing_zeros();
    if leading + trailing == 32 || value == 0 {
        Some(leading as u8)
    } else {
        None
    }
}

/// `host` + prefix → network address in CIDR form (e.g. 11.36.23.173/19 → 11.36.0.0/19).
fn ipv4_network_cidr(host: &str, bits: u8) -> Option<String> {
    if bits > 32 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in host.split('.').enumerate() {
        if i >= 4 {
            return None;
        }
        octets[i] = part.parse().ok()?;
    }
    let mut value = u32::from_be_bytes(octets);
    if bits == 0 {
        value = 0;
    } else {
        let mask = u32::MAX << (32 - bits);
        value &= mask;
    }
    let net = value.to_be_bytes();
    Some(format!(
        "{}.{}.{}.{}/{}",
        net[0], net[1], net[2], net[3], bits
    ))
}

#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub profile: ConnectionProfile,
    /// When true, skip full AnyConnect auth and only run platform/mock path
    /// (used by isolated unit and host tests without a real headend).
    pub dry_run: bool,
}

#[cfg(test)]
mod tests;
