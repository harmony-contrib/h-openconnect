//! Keep an authenticated `anyconnect::Client` alive across the platform TUN
//! handoff, then run OpenConnect's mainloop on a dedicated thread.

use crate::auth_bridge::{
    apply_credentials_to_fields, can_autofill_without_ui, merge_reply_values, AuthCredentials,
    AuthInteraction,
};
use crate::error::{CoreError, CoreResult};
use crate::model::{
    AuthChallenge, AuthField, AuthFieldChoice, AuthFieldKind, AuthGroupDiscovery,
    ConnectionProfile, NetworkSnapshot, SessionStats, VpnOptions,
};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const DEFAULT_ANYCONNECT_VERSION: &str = "4.10.07061";
const DEFAULT_ANYCONNECT_USER_AGENT: &str = "AnyConnect Android 4.10.07061";
const DEFAULT_MOBILE_PLATFORM: &str = "OpenHarmony";
const DEFAULT_MOBILE_DEVICE_TYPE: &str = "phone";

/// Live statistics mirrored from OpenConnect's stats callback / OC_CMD_STATS.
#[derive(Debug, Default)]
pub struct SharedTraffic {
    pub stats: SessionStats,
}

pub struct PendingNativeSession {
    /// Present when CSTP is (or will be) owned in this process. UI cookie-only
    /// handoff leaves this `None`; the VPN extension rebuilds from cookie.
    pub client: Option<anyconnect::Client>,
    pub network: NetworkSnapshot,
    pub options: VpnOptions,
    pub traffic: Arc<Mutex<SharedTraffic>>,
    /// OpenConnect requires the platform TUN to be attached before the
    /// optional DTLS transport is initialized.
    setup_dtls_after_tun: bool,
}

pub struct RunningNativeSession {
    command: anyconnect::CommandHandle,
    join: JoinHandle<CoreResult<()>>,
    traffic: Arc<Mutex<SharedTraffic>>,
}

impl RunningNativeSession {
    pub fn request_stats(&self) {
        let _ = self.command.send(anyconnect::Command::Statistics);
    }

    pub fn traffic_snapshot(&self) -> SessionStats {
        self.traffic
            .lock()
            .map(|guard| guard.stats.clone())
            .unwrap_or_default()
    }

    pub fn seed_network_meta(&self, assigned_ip: String, gateway: String, mtu: u32) {
        if let Ok(mut guard) = self.traffic.lock() {
            guard.stats.assigned_ip = assigned_ip;
            guard.stats.gateway = gateway;
            guard.stats.mtu = mtu;
        }
    }

    pub fn cancel(&self) {
        let _ = self.command.send(anyconnect::Command::Cancel);
    }

    pub fn join(self, timeout: Duration) -> CoreResult<()> {
        // Best-effort: mainloop should exit after Cancel. We cannot truly timeout
        // a JoinHandle without killing the thread, so we just join.
        let _ = timeout;
        match self.join.join() {
            Ok(result) => result,
            Err(_) => Err(CoreError::msg("anyconnect mainloop thread panicked")),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.join.is_finished()
    }
}

/// Authenticate against the headend and return a client ready for TUN attach.
///
/// When `interaction` is provided, OpenConnect forms that cannot be auto-filled
/// from the profile block until the UI submits [`crate::model::AuthChallengeReply`].
/// Pass `None` for non-interactive paths (VPN extension re-auth).
pub fn authenticate(
    profile: &ConnectionProfile,
    interaction: Option<Arc<AuthInteraction>>,
) -> CoreResult<PendingNativeSession> {
    use anyconnect::{Client, LogLevel, Statistics};

    let url = auth_url_for_profile(profile);
    if url.is_empty() {
        return Err(CoreError::msg("server address is empty"));
    }

    let creds = AuthCredentials {
        username: profile.username.clone(),
        password: profile.password.clone(),
        group: profile.group.clone(),
    };
    // Untrusted certificates are accepted only when both safety switches are
    // explicitly disabled.
    let accept_untrusted = !profile.strict_certificate_trust && !profile.block_untrusted_servers;
    let server_cert_hash = profile.server_cert_hash.trim().to_owned();
    let traffic = Arc::new(Mutex::new(SharedTraffic::default()));
    let traffic_cb = Arc::clone(&traffic);
    let interaction = interaction.clone();
    let auth_form_error = Arc::new(Mutex::new(None::<String>));
    let callback_error = Arc::clone(&auth_form_error);
    let initial_auth_group = profile.group.trim().to_owned();
    let mut auth_group_initialized = !initial_auth_group.is_empty();

    // Stable AnyConnect UA (do not embed our crate version — headends fingerprint it).
    let user_agent = effective_user_agent(profile);
    let mut builder = Client::builder()
        .url(url.clone())
        .protocol(profile.protocol.as_openconnect())
        .user_agent(user_agent)
        .authentication_handler(move |form| {
            handle_auth_form(
                form,
                &creds,
                interaction.as_ref(),
                &mut auth_group_initialized,
                &callback_error,
            )
        })
        .certificate_validator(move |error| {
            peer_cert_decision(
                accept_untrusted,
                &error.reason,
                error.fingerprint.as_deref(),
            )
        })
        .config_handler(persist_server_config)
        .statistics_handler(move |stats: Statistics| {
            if let Ok(mut guard) = traffic_cb.lock() {
                guard.stats.bytes_sent = stats.transmitted_bytes;
                guard.stats.bytes_received = stats.received_bytes;
                guard.stats.packets_sent = stats.transmitted_packets;
                guard.stats.packets_received = stats.received_packets;
            }
        });

    // Prefer per-fd protect when registered (ArkTS → vpnConnection.protect).
    // Falls back to no-op; process-wide protectProcessNet covers sockets
    // created after TUN create.
    builder = builder.protect_socket_handler(crate::platform_protect::invoke);
    for pin in server_certificate_hashes(&server_cert_hash) {
        builder = builder.server_certificate_hash(pin);
    }

    // SAML / SSO-v2: OpenConnect binds localhost:29786 then calls this to open
    // the IdP URL in the system browser (ics `external_browser` / openconnect
    // `openconnect_set_external_browser_callback`).
    if wants_external_browser(profile) {
        builder = builder.external_browser_handler(|uri| crate::platform_browser::open(uri));
    }

    let mut client = builder.build()?;
    client.set_log_level(LogLevel::Info);
    client
        .set_auth_group((!initial_auth_group.is_empty()).then_some(initial_auth_group.as_str()))?;
    apply_openconnect_prefs(&mut client, profile, accept_untrusted)?;

    client.obtain_cookie().map_err(|err| {
        if let Some(detail) = auth_form_error.lock().ok().and_then(|guard| guard.clone()) {
            return CoreError::msg(detail);
        }
        let msg = err.to_string();
        // OpenConnect returns >0 (typically 1) when the form handler cancels.
        if msg.contains("status 1") {
            return CoreError::msg(
                "认证已取消（未提交动态口令/验证码，或连接过程中点了取消）".to_owned(),
            );
        }
        let mut hint = String::new();
        if !accept_untrusted {
            hint.push_str(" (tip: disable strict certificate trust for self-signed lab servers)");
        }
        // Prefer meaningful progress lines (errors / form messages), not CSP headers.
        if let Some(tail) = last_progress_lines_interesting(6) {
            hint.push_str(" | ");
            hint.push_str(&tail);
        } else if msg.contains("status -5") || msg.contains("status -EIO") {
            hint.push_str(" [EIO: login form never reached — check network/TLS/server URL]");
        }
        CoreError::msg(format!("{msg}{hint}"))
    })?;
    // The UI owns interactive authentication. The VPN extension owns CSTP and
    // the TUN, so hand off the authenticated cookie without opening CSTP here.
    let cookie = client.cookie();
    if cookie.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
        return Err(CoreError::msg(
            "obtain_cookie succeeded but cookie is empty",
        ));
    }
    // Keep the authentication endpoint with the cookie so its scope and the
    // subsequent CSTP request stay consistent.
    // Addresses filled by extension after CSTP; placeholder only for Want size.
    let snapshot = NetworkSnapshot {
        address: None,
        netmask: None,
        address_v6: None,
        netmask_v6: None,
        gateway: Some(profile.server.clone()),
        dns: Vec::new(),
        mtu: if profile.mtu > 0 {
            profile.mtu as i32
        } else {
            1400
        },
        routes: Vec::new(),
        split_excludes: Vec::new(),
        domain: None,
        split_dns: Vec::new(),
    };
    let mut options = VpnOptions::from_network(&snapshot, profile);
    // Clear placeholder address so extension must produce real ones.
    // Keep force_global / allow_bypass / credentials / cookie for handoff.
    options.addresses.clear();
    options.routes.clear();
    options.dns_addresses.clear();
    options.cookie = cookie;
    options.server = Some(url);
    options.accept_untrusted = accept_untrusted;
    options.force_global = profile.force_global;
    // Carry protocol prefs for extension rebuild.
    options.use_dtls = profile.use_dtls;
    options.reported_os = profile.reported_os.clone();
    options.sni = profile.sni.clone();
    options.require_pfs = profile.require_pfs;
    options.disable_xml_post = profile.disable_xml_post;
    options.dpd_seconds = profile.dpd_seconds;
    options.vpn_protocol = profile.protocol.as_openconnect().to_owned();
    options.user_agent = profile.user_agent.clone();
    options.client_version = profile.client_version.clone();
    options.allow_insecure_crypto = profile.allow_insecure_crypto;
    options.fips_mode = profile.fips_mode;
    options.external_auth_allowed = wants_external_browser(profile);
    options.mobile_unique_id = profile.id.clone();
    options.apply_force_global();

    // Drop Client without an open CSTP session so the headend keeps the cookie
    // usable for the extension process. Do not call logout/reset first.
    drop(client);

    Ok(PendingNativeSession {
        client: None,
        network: snapshot,
        options,
        traffic,
        setup_dtls_after_tun: false,
    })
}

/// Fetch the initial AnyConnect authentication form and return the headend's
/// advertised groups without submitting credentials.
pub fn discover_auth_groups(profile: &ConnectionProfile) -> CoreResult<AuthGroupDiscovery> {
    use anyconnect::{AuthFormResult, Client, LogLevel};

    let url = auth_url_for_profile(profile);
    if url.is_empty() {
        return Err(CoreError::msg("server address is empty"));
    }

    let accept_untrusted = !profile.strict_certificate_trust && !profile.block_untrusted_servers;
    let server_cert_hash = profile.server_cert_hash.trim().to_owned();
    let captured = Arc::new(Mutex::new(None::<AuthGroupDiscovery>));
    let callback_capture = Arc::clone(&captured);
    let user_agent = effective_user_agent(profile);

    let mut builder = Client::builder()
        .url(url.clone())
        .protocol(profile.protocol.as_openconnect())
        .user_agent(user_agent)
        .authentication_handler(move |form| {
            let groups = form
                .auth_group_choices()
                .into_iter()
                .map(|choice| AuthFieldChoice {
                    name: choice.name,
                    label: choice.label,
                })
                .collect();
            let selected = form.selected_auth_group().map(|choice| choice.name);
            if let Ok(mut slot) = callback_capture.lock() {
                *slot = Some(AuthGroupDiscovery { selected, groups });
            }
            // Discovery intentionally stops before username/password or MFA.
            AuthFormResult::Cancelled
        })
        .certificate_validator(move |error| {
            peer_cert_decision(
                accept_untrusted,
                &error.reason,
                error.fingerprint.as_deref(),
            )
        })
        .config_handler(persist_server_config)
        .protect_socket_handler(crate::platform_protect::invoke);
    for pin in server_certificate_hashes(&server_cert_hash) {
        builder = builder.server_certificate_hash(pin);
    }
    let mut client = builder.build()?;
    client.set_log_level(LogLevel::Info);
    apply_openconnect_prefs(&mut client, profile, accept_untrusted)?;
    // Discovery must receive the ordinary group-list form and must never
    // launch an IdP or bind the response to a previously saved group.
    client.set_auth_group(None)?;
    client.set_external_auth_allowed(false);

    let auth_result = client.obtain_cookie();
    let discovery = captured
        .lock()
        .map_err(|_| CoreError::msg("authentication group discovery lock poisoned"))?
        .clone();
    if let Some(discovery) = discovery {
        return Ok(discovery);
    }

    auth_result.map_err(|err| {
        CoreError::msg(format!(
            "failed to fetch authentication groups from {url}: {err}"
        ))
    })?;
    Ok(AuthGroupDiscovery::default())
}

/// Build the OpenConnect URL from the profile server field only.
///
/// Do **not** auto-append the tunnel group as a path segment. On some portals
/// (e.g. sslvpn.sankuai.com) `https://host/group` returns
/// "Invalid host entry. Please re-enter." — the group must be submitted via the
/// auth form (`group_list`), not the URL. Users who truly need a path can put it
/// in the server field themselves (`https://host/path`).
fn auth_url_for_profile(profile: &ConnectionProfile) -> String {
    profile.server_url()
}

/// Whether to register OpenConnect's external-browser SSO callback.
///
/// Enabled when the profile opts into external browser auth, or when the auth
/// method is SAML (browser login is the expected path for SSO-v2 headends).
fn wants_external_browser(profile: &ConnectionProfile) -> bool {
    profile.external_browser_auth || matches!(profile.auth_method, crate::model::AuthMethod::Saml)
}

/// Decide whether to accept a peer certificate (ics trust + openconnect --servercert pin).
fn peer_cert_decision(accept_untrusted: bool, _reason: &str, _fingerprint: Option<&str>) -> bool {
    accept_untrusted
}

fn effective_user_agent(profile: &ConnectionProfile) -> String {
    let configured = profile.user_agent.trim();
    if configured.is_empty() {
        DEFAULT_ANYCONNECT_USER_AGENT.to_owned()
    } else {
        configured.to_owned()
    }
}

fn effective_client_version(profile: &ConnectionProfile) -> &str {
    let configured = profile.client_version.trim();
    if configured.is_empty() {
        DEFAULT_ANYCONNECT_VERSION
    } else {
        configured
    }
}

fn server_certificate_hashes(raw: &str) -> Vec<String> {
    raw.split(|character: char| character.is_whitespace() || character == ',' || character == ';')
        .map(str::trim)
        .filter(|fingerprint| !fingerprint.is_empty())
        .map(str::to_owned)
        .collect()
}

fn should_use_system_trust(server_cert_hash: &str, accept_untrusted: bool) -> bool {
    server_certificate_hashes(server_cert_hash).is_empty() && !accept_untrusted
}

fn persist_server_config(config: &[u8]) -> anyconnect::ConfigWriteResult {
    use anyconnect::ConfigWriteResult;

    let Ok(home) = std::env::var("HANYCONNECT_HOME") else {
        // Host tools may not have an application data directory; accepting the
        // config keeps authentication compatible without pretending a failure.
        return ConfigWriteResult::Accepted;
    };
    let path = std::path::Path::new(&home).join("anyconnect-server-profile.xml");
    if crate::private_fs::write_atomic_private(&path, config).is_err() {
        ConfigWriteResult::Error
    } else {
        ConfigWriteResult::Accepted
    }
}

/// Apply ics-openconnect `setPreferences`-equivalent options to a live client.
fn apply_openconnect_prefs(
    client: &mut anyconnect::Client,
    profile: &ConnectionProfile,
    accept_untrusted: bool,
) -> CoreResult<()> {
    use anyconnect::TokenMode;
    use std::time::Duration;

    if profile.fips_mode {
        return Err(CoreError::msg(
            "FIPS mode is unavailable: this build has no validated OpenSSL FIPS provider",
        ));
    }

    let os = if profile.reported_os.trim().is_empty() {
        "android"
    } else {
        profile.reported_os.trim()
    };
    client.set_exact_user_agent(effective_user_agent(profile))?;
    client.set_client_version(effective_client_version(profile))?;
    client.set_reported_os(os)?;
    if os == "android" || os == "apple-ios" {
        let unique_id = profile.id.trim();
        if !unique_id.is_empty() {
            client.set_mobile_info(
                DEFAULT_MOBILE_PLATFORM,
                DEFAULT_MOBILE_DEVICE_TYPE,
                unique_id,
            )?;
        }
    }
    if !profile.sni.trim().is_empty() {
        client.set_sni(Some(profile.sni.trim()))?;
    }
    client.set_external_auth_allowed(wants_external_browser(profile));
    client.set_xml_post(!profile.disable_xml_post);
    client.set_perfect_forward_secrecy(profile.require_pfs);
    if profile.dpd_seconds > 0 {
        client.set_dpd_minimum(Some(Duration::from_secs(u64::from(profile.dpd_seconds))))?;
    }
    if profile.mtu > 0 {
        client.set_requested_mtu(Some(profile.mtu))?;
    }
    if !profile.use_dtls {
        client.disable_dtls()?;
    }
    if !profile.ca_certificate.trim().is_empty() {
        client.set_ca_file(profile.ca_certificate.trim())?;
    }
    if !profile.certificate.trim().is_empty() {
        let key = if profile.private_key.trim().is_empty() {
            profile.certificate.trim()
        } else {
            profile.private_key.trim()
        };
        client.set_client_certificate(profile.certificate.trim(), key)?;
    }
    if !profile.key_password.trim().is_empty() {
        client.set_key_password(profile.key_password.trim())?;
    }
    if !profile.secondary_certificate.trim().is_empty() {
        let key = if profile.secondary_private_key.trim().is_empty() {
            profile.secondary_certificate.trim()
        } else {
            profile.secondary_private_key.trim()
        };
        client.set_secondary_client_certificate(profile.secondary_certificate.trim(), key)?;
    }
    if !profile.secondary_key_password.trim().is_empty() {
        client.set_secondary_key_password(profile.secondary_key_password.trim())?;
    }
    if !profile.http_proxy.trim().is_empty() {
        client.set_http_proxy(profile.http_proxy.trim())?;
    }
    if !profile.csd_wrapper.trim().is_empty() {
        #[cfg(unix)]
        {
            client.setup_csd(profile.csd_wrapper.trim(), None)?;
        }
    }
    match profile.software_token {
        crate::model::SoftwareToken::SecurId => {
            let secret = if profile.token_string.is_empty() {
                None
            } else {
                Some(profile.token_string.as_str())
            };
            client.set_token_mode(TokenMode::SecurId, secret)?;
        }
        crate::model::SoftwareToken::Totp => {
            let secret = if profile.token_string.is_empty() {
                None
            } else {
                Some(profile.token_string.as_str())
            };
            client.set_token_mode(TokenMode::Totp, secret)?;
        }
        crate::model::SoftwareToken::Disabled => {}
    }
    // A configured pin is the sole trust source, matching `--servercert`.
    client.set_system_trust(should_use_system_trust(
        &profile.server_cert_hash,
        accept_untrusted,
    ));
    client.set_allow_insecure_crypto(profile.allow_insecure_crypto)?;
    Ok(())
}

fn last_progress_lines_interesting(n: usize) -> Option<String> {
    let home = std::env::var("HANYCONNECT_HOME").ok()?;
    let path = std::path::Path::new(&home).join("openconnect-progress.log");
    let text = std::fs::read_to_string(path).ok()?;
    let skip = [
        "content-security-policy",
        "cross-origin-opener-policy",
        "x-frame-options",
        "strict-transport-security",
        "x-content-type-options",
        "x-xss-protection",
        "cache-control",
        "pragma:",
        "connection:",
        "date:",
        "transfer-encoding",
        "http body",
    ];
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let lower = l.to_ascii_lowercase();
            !skip.iter().any(|s| lower.contains(s))
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(n);
    Some(lines[start..].join(" · "))
}

fn handle_auth_form(
    form: &mut anyconnect::AuthForm<'_>,
    creds: &AuthCredentials,
    interaction: Option<&Arc<AuthInteraction>>,
    auth_group_initialized: &mut bool,
    auth_form_error: &Arc<Mutex<Option<String>>>,
) -> anyconnect::AuthFormResult {
    use anyconnect::{AuthFormResult, FormOptionKind};

    if !creds.group.trim().is_empty() {
        if let Some(result) =
            select_auth_group(form, &creds.group, auth_group_initialized, auth_form_error)
        {
            return result;
        }
    }

    let mut fields = snapshot_form_fields(form);
    apply_credentials_to_fields(&mut fields, creds);

    let values = if can_autofill_without_ui(&fields) {
        fields
            .iter()
            .filter(|f| !f.value.is_empty())
            .map(|f| (f.name.clone(), f.value.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    } else if let Some(interaction) = interaction {
        let challenge = AuthChallenge {
            id: 0,
            round: 0,
            banner: form.banner(),
            message: form.message(),
            error: form.error(),
            form_id: form.id(),
            method: form.method(),
            fields: fields.clone(),
        };
        let reply = interaction.wait_for_reply(challenge);
        if reply.cancelled {
            return AuthFormResult::Cancelled;
        }
        merge_reply_values(&fields, &reply)
    } else {
        // Extension and host-only paths are non-interactive. Authentication
        // challenges are completed in the UI before the cookie handoff.
        if !can_autofill_without_ui(&fields) {
            return AuthFormResult::Cancelled;
        }
        fields
            .iter()
            .filter(|f| !f.value.is_empty())
            .map(|f| (f.name.clone(), f.value.clone()))
            .collect()
    };

    // A group picked in the interactive form changes which AAA fields are
    // active. Refresh the form instead of submitting fields belonging to the
    // old/default RADIUS policy.
    if let Some(group_field) = fields.iter().find(|field| field.auth_group) {
        if let Some(value) = values.get(&group_field.name) {
            if let Some(result) =
                select_auth_group(form, value, auth_group_initialized, auth_form_error)
            {
                return result;
            }
        }
    }

    for mut option in form.options() {
        let name = option.name().unwrap_or_default();
        if matches!(option.kind(), FormOptionKind::Hidden) || option.is_ignored() {
            continue;
        }
        if let Some(value) = values.get(&name) {
            let result = if matches!(option.kind(), FormOptionKind::Select) {
                option.set_choice(value)
            } else {
                option.set_value(value)
            };
            if let Err(err) = result {
                return auth_form_failure(
                    auth_form_error,
                    format!("failed to set authentication field {name}: {err}"),
                );
            }
        }
    }
    AuthFormResult::Submit
}

/// Select the exact server-advertised group value. The first selection, or a
/// later change, requires `NewGroup` so the headend can return the matching
/// primary/secondary authentication fields.
fn select_auth_group(
    form: &mut anyconnect::AuthForm<'_>,
    requested: &str,
    initialized: &mut bool,
    auth_form_error: &Arc<Mutex<Option<String>>>,
) -> Option<anyconnect::AuthFormResult> {
    use anyconnect::AuthFormResult;

    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    let choices = form.auth_group_choices();
    if choices.is_empty() {
        // Other protocols can expose realm/domain as an ordinary named field.
        return None;
    }
    let Some(selection) = choices.iter().position(|choice| {
        choice.name.eq_ignore_ascii_case(requested)
            || choice.label.trim().eq_ignore_ascii_case(requested)
    }) else {
        let available = choices
            .iter()
            .map(|choice| {
                if choice.label.trim().is_empty() || choice.label == choice.name {
                    choice.name.clone()
                } else {
                    format!("{} ({})", choice.label, choice.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Some(auth_form_failure(
            auth_form_error,
            format!(
                "authentication group {requested:?} is not offered by the server; available groups: {available}"
            ),
        ));
    };

    let selection_changed = form.auth_group_selection() != selection as i32;
    if let Err(err) = form.set_auth_group_selection(selection) {
        return Some(auth_form_failure(
            auth_form_error,
            format!("failed to select authentication group {requested:?}: {err}"),
        ));
    }
    if !*initialized || selection_changed {
        *initialized = true;
        return Some(AuthFormResult::NewGroup);
    }
    None
}

fn auth_form_failure(
    auth_form_error: &Arc<Mutex<Option<String>>>,
    message: String,
) -> anyconnect::AuthFormResult {
    if let Ok(mut slot) = auth_form_error.lock() {
        *slot = Some(message.clone());
    }
    anyconnect::AuthFormResult::Error
}

fn snapshot_form_fields(form: &mut anyconnect::AuthForm<'_>) -> Vec<AuthField> {
    use anyconnect::FormOptionKind;

    let mut fields = Vec::new();
    let selected_auth_group = form.selected_auth_group().map(|choice| choice.name);
    for option in form.options() {
        if option.is_ignored() {
            continue;
        }
        let option_kind = option.kind();
        if protocol_owns_auth_value(option_kind) {
            // These values are owned by OpenConnect. TOKEN is populated by the
            // configured software-token generator after this callback, while
            // SSO_TOKEN/SSO_USER are populated by the browser flow.
            continue;
        }
        let kind = match option_kind {
            FormOptionKind::Text => AuthFieldKind::Text,
            FormOptionKind::Password => AuthFieldKind::Password,
            FormOptionKind::Select => AuthFieldKind::Select,
            FormOptionKind::Hidden => AuthFieldKind::Hidden,
            FormOptionKind::Token | FormOptionKind::SsoToken | FormOptionKind::SsoUser => {
                unreachable!("protocol-owned fields were filtered above")
            }
            FormOptionKind::Unknown(_) => AuthFieldKind::Unknown,
        };
        let name = option.name().unwrap_or_default();
        let label = option
            .label()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| name.clone());
        let auth_group = option.is_auth_group();
        let mut value = option.value().unwrap_or_default();
        if auth_group && value.is_empty() {
            value = selected_auth_group.clone().unwrap_or_default();
        }
        let choices = option
            .choices()
            .into_iter()
            .map(|c| {
                let name = c.name;
                let label = if c.label.is_empty() {
                    name.clone()
                } else {
                    c.label
                };
                AuthFieldChoice { name, label }
            })
            .collect();
        let required = !matches!(kind, AuthFieldKind::Hidden | AuthFieldKind::Unknown)
            && value.trim().is_empty();
        fields.push(AuthField {
            name,
            label,
            kind,
            value,
            choices,
            auth_group,
            required,
        });
    }
    fields
}

fn protocol_owns_auth_value(kind: anyconnect::FormOptionKind) -> bool {
    matches!(
        kind,
        anyconnect::FormOptionKind::Token
            | anyconnect::FormOptionKind::SsoToken
            | anyconnect::FormOptionKind::SsoUser
    )
}

/// Establish a live client in the VPN-extension process from the cookie
/// produced by the UI authentication flow.
///
/// HarmonyOS runs `VpnExtensionAbility` in a **separate process**, so the UI
/// cannot hand over a live TCP/`Client`. The UI already completed interactive
/// MFA and wrote a cookie into `session-handoff.json`.
///
pub fn resume_from_options(options: &VpnOptions) -> CoreResult<PendingNativeSession> {
    let primary = options
        .server
        .as_deref()
        .map(normalize_server_url)
        .filter(|server| !server.is_empty())
        .ok_or_else(|| CoreError::msg("VPN options missing server for session resume"))?;
    let mut candidates = vec![primary];
    for backup in &options.backup_servers {
        let backup = normalize_server_url(backup);
        if !backup.is_empty() && !candidates.iter().any(|candidate| candidate == &backup) {
            candidates.push(backup);
        }
    }

    let mut failures = Vec::new();
    for (index, server) in candidates.iter().enumerate() {
        let mut candidate = options.clone();
        candidate.server = Some(server.clone());
        match resume_from_options_once(&candidate) {
            Ok(session) => return Ok(session),
            Err(error) => {
                let message = error.to_string();
                failures.push(format!("{server}: {message}"));
                if index + 1 == candidates.len() || !is_failover_eligible(&message) {
                    break;
                }
            }
        }
    }
    Err(CoreError::msg(format!(
        "all eligible AnyConnect gateways failed: {}",
        failures.join(" | ")
    )))
}

fn normalize_server_url(server: &str) -> String {
    let server = server.trim();
    if server.is_empty() {
        String::new()
    } else if server.starts_with("https://") || server.starts_with("http://") {
        server.to_owned()
    } else {
        format!("https://{server}")
    }
}

fn is_failover_eligible(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cancel")
        || lower.contains("认证已取消")
        || lower.contains("credential")
        || lower.contains("password")
        || lower.contains("status 1")
    {
        return false;
    }
    [
        "connect",
        "network",
        "resolve",
        "dns",
        "timeout",
        "timed out",
        "unreachable",
        "refused",
        "tls",
        "ssl",
        "status -5",
        "eio",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn resume_from_options_once(options: &VpnOptions) -> CoreResult<PendingNativeSession> {
    use anyconnect::{Client, LogLevel, Statistics};

    let url = options
        .server
        .clone()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| CoreError::msg("VPN options missing server for session resume"))?;
    let accept_untrusted = options.accept_untrusted;
    let server_cert_hash = options.server_cert_hash.trim().to_owned();
    let username = options.username.clone().unwrap_or_default();
    let password = options.password.clone().unwrap_or_default();
    let group = options.group.clone().unwrap_or_default();
    let cookie = options.cookie.clone().filter(|c| !c.trim().is_empty());
    let has_cookie = cookie.is_some();
    let traffic = Arc::new(Mutex::new(SharedTraffic::default()));
    let traffic_cb = Arc::clone(&traffic);

    if !has_cookie {
        return Err(CoreError::msg(
            "VPN extension did not receive the authenticated cookie",
        ));
    }

    let initial_auth_group = group.trim().to_owned();
    let mut auth_group_initialized = !initial_auth_group.is_empty();
    let creds = AuthCredentials {
        username,
        password: password.clone(),
        group,
    };
    let auth_form_error = Arc::new(Mutex::new(None::<String>));
    let callback_error = Arc::clone(&auth_form_error);

    let proto = if options.vpn_protocol.trim().is_empty() {
        "anyconnect"
    } else {
        options.vpn_protocol.as_str()
    };
    let user_agent = if options.user_agent.trim().is_empty() {
        DEFAULT_ANYCONNECT_USER_AGENT.to_owned()
    } else {
        options.user_agent.clone()
    };
    // Must match UI-process fingerprint (UA + OS + protocol).
    let mut builder = Client::builder()
        .url(url.clone())
        .protocol(proto)
        .user_agent(user_agent)
        .authentication_handler(move |form| {
            handle_auth_form(
                form,
                &creds,
                None,
                &mut auth_group_initialized,
                &callback_error,
            )
        })
        .certificate_validator(move |error| {
            peer_cert_decision(
                accept_untrusted,
                &error.reason,
                error.fingerprint.as_deref(),
            )
        })
        .config_handler(persist_server_config)
        .statistics_handler(move |stats: Statistics| {
            if let Ok(mut guard) = traffic_cb.lock() {
                guard.stats.bytes_sent = stats.transmitted_bytes;
                guard.stats.bytes_received = stats.received_bytes;
                guard.stats.packets_sent = stats.transmitted_packets;
                guard.stats.packets_received = stats.received_packets;
            }
        })
        .protect_socket_handler(crate::platform_protect::invoke);
    if options.external_auth_allowed {
        // SAML/SSO-v2: OpenConnect listens on localhost:29786 then asks us to
        // open the IdP URL. Extension has no UI Ability, so open() queues a
        // file for the UI process to launch the system browser.
        builder = builder.external_browser_handler(|uri| crate::platform_browser::open(uri));
    }
    for pin in server_certificate_hashes(&server_cert_hash) {
        builder = builder.server_certificate_hash(pin);
    }

    let mut client = builder.build()?;
    client.set_log_level(LogLevel::Info);
    // Rebuild a minimal profile from handoff options for shared prefs application.
    let mut resume_profile = ConnectionProfile::new_draft();
    resume_profile.id = options.mobile_unique_id.clone();
    resume_profile.group = options.group.clone().unwrap_or_default();
    resume_profile.backup_servers = options.backup_servers.join("\n");
    resume_profile.use_dtls = options.use_dtls;
    resume_profile.reported_os = options.reported_os.clone();
    resume_profile.sni = options.sni.clone();
    resume_profile.require_pfs = options.require_pfs;
    resume_profile.disable_xml_post = options.disable_xml_post;
    resume_profile.dpd_seconds = options.dpd_seconds;
    resume_profile.certificate = options.certificate.clone();
    resume_profile.private_key = options.private_key.clone();
    resume_profile.secondary_certificate = options.secondary_certificate.clone();
    resume_profile.secondary_private_key = options.secondary_private_key.clone();
    resume_profile.ca_certificate = options.ca_certificate.clone();
    resume_profile.key_password = options.key_password.clone();
    resume_profile.secondary_key_password = options.secondary_key_password.clone();
    resume_profile.http_proxy = options.http_proxy.clone();
    resume_profile.server_cert_hash = options.server_cert_hash.clone();
    resume_profile.csd_wrapper = options.csd_wrapper.clone();
    resume_profile.software_token = options.software_token;
    resume_profile.token_string = options.token_string.clone();
    resume_profile.mtu = options.mtu;
    resume_profile.user_agent = options.user_agent.clone();
    resume_profile.client_version = options.client_version.clone();
    resume_profile.allow_insecure_crypto = options.allow_insecure_crypto;
    resume_profile.fips_mode = options.fips_mode;
    resume_profile.external_browser_auth = options.external_auth_allowed;
    client.set_auth_group(
        (!resume_profile.group.trim().is_empty()).then_some(resume_profile.group.trim()),
    )?;
    apply_openconnect_prefs(&mut client, &resume_profile, accept_untrusted)?;

    let cookie_value = cookie.as_deref().expect("cookie checked above");
    client
        .set_cookie(cookie_value)
        .map_err(|err| CoreError::msg(format!("extension set_cookie failed: {err}")))?;
    client
        .make_cstp_connection()
        .map_err(|err| CoreError::msg(format!("extension make_cstp failed: {err}")))?;

    let network = client.network_config()?;
    let mut profile = ConnectionProfile::new_draft();
    profile.mtu = options.mtu;
    profile.allow_local_lan = options.allow_bypass;
    profile.force_global = options.force_global;
    profile.split_tunnel_mode = options.split_tunnel_mode;
    profile.split_tunnel_networks = options.split_tunnel_networks.clone();
    // Prefer the handoff network snapshot if cookie resume does not return a
    // complete IP configuration.
    let snapshot = {
        let live = network_snapshot_from_openconnect(&network, &profile);
        if live.address.is_some() {
            live
        } else if !options.addresses.is_empty() {
            NetworkSnapshot {
                address: options
                    .addresses
                    .first()
                    .map(|a| a.split('/').next().unwrap_or(a).to_owned()),
                netmask: None,
                address_v6: None,
                netmask_v6: None,
                gateway: options.server.clone(),
                dns: options.dns_addresses.clone(),
                mtu: options.mtu as i32,
                routes: options.routes.clone(),
                split_excludes: Vec::new(),
                domain: None,
                split_dns: options.search_domains.clone(),
            }
        } else {
            live
        }
    };
    // Always rebuild system routes/DNS/address from the live CSTP snapshot.
    // Handoff placeholders (empty or stale) must not win over real IP config.
    let mut filled = VpnOptions::from_network(&snapshot, &profile);
    filled.cookie = client.cookie().or(options.cookie.clone());
    filled.server = options.server.clone().or(filled.server);
    filled.username = options.username.clone().or(filled.username);
    filled.password = options.password.clone().or(filled.password);
    filled.group = options.group.clone().or(filled.group);
    filled.accept_untrusted = accept_untrusted;
    filled.force_global = options.force_global || filled.force_global;
    // ics: force_global → 0.0.0.0/0; always ensure DNS host routes.
    filled.apply_force_global();
    filled.normalize_routes();
    for server in filled.dns_addresses.clone() {
        let host = server.split('%').next().unwrap_or(&server).trim();
        if host.is_empty() {
            continue;
        }
        let host_route = if host.contains(':') {
            format!("{host}/128")
        } else {
            format!("{host}/32")
        };
        if !filled.routes.iter().any(|r| r == &host_route) {
            filled.routes.insert(0, host_route);
        }
    }
    Ok(PendingNativeSession {
        client: Some(client),
        network: snapshot,
        options: filled,
        traffic,
        setup_dtls_after_tun: options.use_dtls,
    })
}

/// Attach the platform TUN descriptor and spawn OpenConnect's blocking mainloop.
pub fn spawn_mainloop(
    pending: PendingNativeSession,
    tun_fd: i32,
) -> CoreResult<RunningNativeSession> {
    use std::os::fd::BorrowedFd;

    if tun_fd < 0 {
        return Err(CoreError::msg(format!("invalid TUN fd {tun_fd}")));
    }

    let PendingNativeSession {
        client,
        traffic,
        setup_dtls_after_tun,
        ..
    } = pending;
    let mut client = client.ok_or_else(|| {
        CoreError::msg(
            "spawn_mainloop requires a live Client (cookie-only UI handoff is not attachable)",
        )
    })?;

    let command = client.command_handle()?;
    // SAFETY: VpnConnection owns this live descriptor for the duration of the
    // call. anyconnect-rs duplicates it before returning, matching paws and
    // keeping the platform/native lifetimes independent.
    let borrowed = unsafe { BorrowedFd::borrow_raw(tun_fd) };
    client.setup_tun_fd_borrowed(borrowed)?;

    // OpenConnect lifecycle: CSTP -> network config -> TUN -> optional DTLS ->
    // mainloop. A DTLS failure is non-fatal because CSTP remains the transport.
    if setup_dtls_after_tun {
        let _ = client.setup_dtls(60);
    }

    let join = std::thread::Builder::new()
        .name("hanyconnect-mainloop".to_owned())
        .spawn(move || {
            // 300s reconnect timeout, 10s interval — same order of magnitude as
            // the openconnect CLI defaults for interactive clients.
            client.run_mainloop(300, 10).map_err(CoreError::from)
        })
        .map_err(|err| CoreError::msg(format!("failed to spawn mainloop thread: {err}")))?;

    Ok(RunningNativeSession {
        command,
        join,
        traffic,
    })
}

fn network_snapshot_from_openconnect(
    network: &anyconnect::NetworkConfig,
    profile: &ConnectionProfile,
) -> NetworkSnapshot {
    let routes: Vec<String> = if !network.split_includes.is_empty() {
        network
            .split_includes
            .iter()
            .map(|route| route.0.clone())
            .collect()
    } else {
        Vec::new()
    };
    let mtu = if network.mtu > 0 {
        network.mtu
    } else if profile.mtu > 0 {
        profile.mtu as i32
    } else {
        1400
    };
    NetworkSnapshot {
        address: network.address.clone(),
        netmask: network.netmask.clone(),
        address_v6: network.address_v6.clone(),
        netmask_v6: network.netmask_v6.clone(),
        gateway: network.gateway.clone(),
        dns: network.dns.clone(),
        mtu,
        routes,
        split_excludes: network
            .split_excludes
            .iter()
            .map(|route| route.0.clone())
            .collect(),
        domain: network.domain.clone(),
        split_dns: network
            .split_dns
            .iter()
            .map(|domain| domain.0.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_and_generated_token_fields_stay_protocol_owned() {
        assert!(protocol_owns_auth_value(anyconnect::FormOptionKind::Token));
        assert!(protocol_owns_auth_value(
            anyconnect::FormOptionKind::SsoToken
        ));
        assert!(protocol_owns_auth_value(
            anyconnect::FormOptionKind::SsoUser
        ));
        assert!(!protocol_owns_auth_value(
            anyconnect::FormOptionKind::Password
        ));
    }

    #[test]
    fn failover_does_not_retry_user_authentication_failures() {
        assert!(is_failover_eligible(
            "failed to connect: network unreachable"
        ));
        assert!(is_failover_eligible("TLS handshake timed out"));
        assert!(!is_failover_eligible("authentication cancelled"));
        assert!(!is_failover_eligible("invalid password"));
    }

    #[test]
    fn certificate_pin_list_preserves_base64_case() {
        assert_eq!(
            server_certificate_hashes("pin-sha256:AbCdEf+/=, sha256:0011\nsha1:AABB"),
            vec![
                "pin-sha256:AbCdEf+/=".to_owned(),
                "sha256:0011".to_owned(),
                "sha1:AABB".to_owned()
            ]
        );
    }

    #[test]
    fn certificate_policy_uses_exactly_one_intended_trust_source() {
        assert!(should_use_system_trust("", false));
        assert!(!should_use_system_trust("", true));
        assert!(!should_use_system_trust("sha256:0011", false));
        assert!(!should_use_system_trust("pin-sha256:AbCdEf+/=", true));
    }
}
