use crate::l10n::{strings, UiLocale};
use crate::model::{
    AuthChallengeReply, AuthFieldChoice, AuthFieldValue, AuthMethod, ConnectionLifecycle,
    NetworkSnapshot, ProtocolKind, SessionSnapshot, SessionStats, VpnConnection,
};
use crate::platform_callbacks;
use hanyconnect_core::{shared_engine, ConnectRequest, SessionEngine};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguagePreference {
    #[default]
    System,
    ZhCn,
    En,
}

impl LanguagePreference {
    pub fn resolve(self, system: UiLocale) -> UiLocale {
        match self {
            Self::System => system,
            Self::ZhCn => UiLocale::ZhCn,
            Self::En => UiLocale::En,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "zh-CN" => Self::ZhCn,
            "en" => Self::En,
            _ => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn platform_color_mode(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

pub(crate) struct Command<M>(Vec<Pin<Box<dyn Future<Output = M> + Send>>>);

impl<M: Send + 'static> Command<M> {
    pub(crate) fn none() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn perform<F, O, Map>(future: F, map: Map) -> Self
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
        Map: FnOnce(O) -> M + Send + 'static,
    {
        Self(vec![Box::pin(async move { map(future.await) })])
    }

    pub(crate) fn and(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    pub(crate) fn into_futures(
        self,
    ) -> impl Iterator<Item = Pin<Box<dyn Future<Output = M> + Send>>> {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Bootstrap,
    SetLanguagePreference(LanguagePreference),
    SetThemePreference(ThemePreference),
    SelectConnection(String),
    ToggleConnect,
    ConnectionFinished(Result<SessionOutcome, String>),
    TickSession,
    OpenEditor {
        id: Option<String>,
    },
    SetDraftName(String),
    SetDraftServer(String),
    DiscoverGroups {
        server: String,
    },
    GroupsDiscovered {
        server: String,
        result: Result<hanyconnect_core::AuthGroupDiscovery, String>,
    },
    SetDraftGroup(String),
    SetDraftUsername(String),
    SetDraftPassword(String),
    SetDraftProtocol(ProtocolKind),
    SetDraftAuthMethod(AuthMethod),
    SetDraftCertificate(String),
    SetDraftBackupServers(String),
    SetDraftStrictCertificateTrust(bool),
    SetDraftBlockUntrustedServers(bool),
    SetDraftAllowLocalLan(bool),
    SetDraftForceGlobal(bool),
    SetDraftConnectOnDemand(bool),
    SetDraftExternalBrowserAuth(bool),
    SetDraftFipsMode(bool),
    SetDraftAllowInsecureCrypto(bool),
    SetDraftMtu(String),
    SetDraftFavorite(bool),
    SetDraftUseDtls(bool),
    SetDraftReportedOs(String),
    SetDraftSni(String),
    SetDraftRequirePfs(bool),
    SetDraftDisableXmlPost(bool),
    SetDraftDpdSeconds(String),
    SetDraftSoftwareToken(crate::model::SoftwareToken),
    SetDraftTokenString(String),
    SetDraftSplitTunnelMode(crate::model::SplitTunnelMode),
    SetDraftSplitTunnelNetworks(String),
    SetDraftPrivateKey(String),
    SetDraftSecondaryCertificate(String),
    SetDraftSecondaryPrivateKey(String),
    SetDraftCaCertificate(String),
    SetDraftKeyPassword(String),
    SetDraftSecondaryKeyPassword(String),
    SetDraftHttpProxy(String),
    SetDraftServerCertHash(String),
    SetDraftTrustedApplications(String),
    SetDraftBlockedApplications(String),
    SetDraftCsdWrapper(String),
    SetDraftUserAgent(String),
    SetDraftClientVersion(String),
    /// Show/hide uncommon connection editor fields.
    SetEditorShowAdvanced(bool),
    /// Open the system document picker for a certificate-related file.
    PickCertFile(platform_callbacks::CertFileKind),
    /// Result of [`Action::PickCertFile`] (path empty when cancelled / failed).
    CertFilePicked {
        kind: platform_callbacks::CertFileKind,
        result: Result<String, String>,
    },
    SaveDraft,
    DeleteConnection(String),
    ToggleFavorite(String),
    ClearDiagnostics,
    /// Remove a toast by id (Sonner timer / swipe / close).
    DismissToast(u64),
    /// Update one field on the in-progress auth challenge form.
    SetChallengeField {
        name: String,
        value: String,
    },
    /// Submit the current challenge form to the OpenConnect worker.
    SubmitChallenge,
    /// Cancel the pending challenge (and the in-flight connect).
    CancelChallenge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FeedbackToast {
    pub id: u64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) enum SessionOutcome {
    Connected(SessionStats),
    Disconnected,
    PlatformStartRequested,
}

#[derive(Clone)]
pub(crate) struct State {
    pub locale: UiLocale,
    pub system_locale: UiLocale,
    pub language_preference: LanguagePreference,
    pub theme_preference: ThemePreference,
    pub system_dark: bool,
    pub snapshot: SessionSnapshot,
    pub editor_open: bool,
    /// Uncommon connection options (hidden unless toggled).
    pub editor_show_advanced: bool,
    pub draft: VpnConnection,
    /// Authentication groups fetched from the draft server's initial form.
    pub group_choices: Vec<AuthFieldChoice>,
    pub group_discovery_loading: bool,
    pub group_discovery_error: Option<String>,
    /// Timed Sonner toasts (oldest → newest). Key events only.
    pub toasts: Vec<FeedbackToast>,
    next_toast_id: u64,
    pub dry_run: bool,
    /// Local draft values for the pending auth challenge (field name → value).
    pub challenge_values: HashMap<String, String>,
    /// Last challenge id we seeded `challenge_values` from.
    challenge_seed_id: Option<u64>,
    /// User initiated disconnect (skip the unexpected-drop auto-reconnect).
    user_disconnect: bool,
    /// Previous lifecycle for detecting unexpected drops.
    last_lifecycle: ConnectionLifecycle,
}

impl State {
    pub fn new() -> Self {
        let system_locale = detect_system_locale();
        let system_dark = detect_system_dark();
        let dry_run = match std::env::var("HANYCONNECT_DRY_RUN") {
            Ok(value) => value == "1" || value.eq_ignore_ascii_case("true"),
            Err(_) => {
                // Prefer real anyconnect-rs when the binary was built with it.
                !cfg!(feature = "native-anyconnect")
            }
        };
        let snapshot = shared_engine()
            .snapshot()
            .unwrap_or_else(|_| SessionSnapshot {
                lifecycle: ConnectionLifecycle::Disconnected,
                active_connection_id: None,
                connections: Vec::new(),
                stats: SessionStats::default(),
                network: NetworkSnapshot::default(),
                last_error: None,
                diagnostics: Vec::new(),
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
                sdk_ready: false,
                anyconnect_version: None,
                backend: "unavailable".to_owned(),
                pending_auth: None,
            });
        let last_lifecycle = snapshot.lifecycle;
        let (language_preference, theme_preference) = shared_engine()
            .appearance_preferences()
            .map(|(language, theme)| {
                (
                    LanguagePreference::from_str(&language),
                    ThemePreference::from_str(&theme),
                )
            })
            .unwrap_or_default();
        Self {
            locale: language_preference.resolve(system_locale),
            system_locale,
            language_preference,
            theme_preference,
            system_dark,
            snapshot,
            editor_open: false,
            editor_show_advanced: false,
            draft: VpnConnection::new_draft(),
            group_choices: Vec::new(),
            group_discovery_loading: false,
            group_discovery_error: None,
            toasts: Vec::new(),
            next_toast_id: 0,
            dry_run,
            challenge_values: HashMap::new(),
            challenge_seed_id: None,
            user_disconnect: false,
            last_lifecycle,
        }
    }

    pub fn theme_dark(&self) -> bool {
        match self.theme_preference {
            ThemePreference::System => self.system_dark,
            ThemePreference::Light => false,
            ThemePreference::Dark => true,
        }
    }

    pub fn active_connection(&self) -> Option<&VpnConnection> {
        let id = self.snapshot.active_connection_id.as_deref()?;
        self.snapshot
            .connections
            .iter()
            .find(|connection| connection.id == id)
    }

    pub fn sync_engine(&mut self) {
        if let Ok(snapshot) = shared_engine().snapshot() {
            self.snapshot = snapshot;
            self.seed_challenge_draft();
        }
    }

    fn seed_challenge_draft(&mut self) {
        match self.snapshot.pending_auth.clone() {
            Some(challenge) if self.challenge_seed_id != Some(challenge.id) => {
                self.challenge_values = challenge
                    .fields
                    .iter()
                    .filter(|field| {
                        !matches!(
                            field.kind,
                            hanyconnect_core::AuthFieldKind::Hidden
                                | hanyconnect_core::AuthFieldKind::Unknown
                        )
                    })
                    .map(|field| (field.name.clone(), field.value.clone()))
                    .collect();
                self.challenge_seed_id = Some(challenge.id);
            }
            None => {
                self.challenge_values.clear();
                self.challenge_seed_id = None;
            }
            _ => {}
        }
    }

    /// Key-status toast only (connected / failed / save / validation). Max 2.
    fn push_toast(&mut self, message: impl Into<String>) {
        let message = message.into();
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }
        self.next_toast_id = self.next_toast_id.saturating_add(1);
        let id = self.next_toast_id;
        // Keep the list short so the top banner stays readable.
        while self.toasts.len() >= 2 {
            self.toasts.remove(0);
        }
        self.toasts.push(FeedbackToast {
            id,
            message: trimmed.to_owned(),
        });
    }
}

pub(crate) fn reduce(state: &mut State, action: Action) -> Command<Action> {
    let s = strings(state.locale);
    match action {
        Action::Bootstrap => {
            state.sync_engine();
            Command::perform(session_tick_delay(), |_| Action::TickSession)
        }
        Action::SetLanguagePreference(preference) => {
            state.language_preference = preference;
            state.locale = preference.resolve(state.system_locale);
            if let Err(err) = shared_engine()
                .set_appearance_preferences(preference.as_str(), state.theme_preference.as_str())
            {
                state.push_toast(err.to_string());
            }
            Command::none()
        }
        Action::SetThemePreference(preference) => {
            state.theme_preference = preference;
            if let Err(err) = shared_engine()
                .set_appearance_preferences(state.language_preference.as_str(), preference.as_str())
            {
                state.push_toast(err.to_string());
            }
            Command::none()
        }
        Action::SelectConnection(id) => {
            match shared_engine().select_profile(&id) {
                Ok(()) => state.sync_engine(),
                Err(err) => state.push_toast(err.to_string()),
            }
            Command::none()
        }
        Action::ToggleConnect => {
            if state.snapshot.lifecycle.is_busy() {
                return Command::none();
            }
            if state.snapshot.lifecycle.is_active() {
                state.user_disconnect = true;
                state.snapshot.lifecycle = ConnectionLifecycle::Disconnecting;
                state.snapshot.last_error = None;
                let dry_run = state.dry_run;
                return Command::perform(engine_disconnect(dry_run), Action::ConnectionFinished);
            }
            state.user_disconnect = false;
            start_connect(state, &s)
        }
        Action::ConnectionFinished(result) => match result {
            Ok(SessionOutcome::Connected(stats)) => {
                state.sync_engine();
                state.snapshot.stats = stats;
                state.push_toast(s.feedback_connected);
                Command::none()
            }
            Ok(SessionOutcome::PlatformStartRequested) => {
                state.sync_engine();
                // No toast: lifecycle UI already shows connecting/authenticating.
                Command::none()
            }
            Ok(SessionOutcome::Disconnected) => {
                state.sync_engine();
                state.push_toast(s.feedback_disconnected);
                Command::none()
            }
            Err(message) => {
                state.sync_engine();
                state.snapshot.lifecycle = ConnectionLifecycle::Failed;
                // Prefer engine message (already localized for cancel / protocol hints).
                state.snapshot.last_error = Some(message.clone());
                let toast = if message.contains("取消") || message.contains("cancel") {
                    message.clone()
                } else {
                    s.feedback_failed.to_owned()
                };
                state.push_toast(toast);
                Command::none()
            }
        },
        Action::TickSession => {
            let system_locale = detect_system_locale();
            if system_locale != state.system_locale {
                state.system_locale = system_locale;
                state.locale = state.language_preference.resolve(system_locale);
            }
            state.system_dark = detect_system_dark();
            // SAML/SSO: extension queues browser-request.json; open system browser here.
            if let Some(req) = hanyconnect_core::take_browser_open_pending() {
                if let Err(err) = platform_callbacks::open_external_browser(req.uri.clone()) {
                    state.push_toast(format!(
                        "{}: {err}",
                        tr_msg(state.locale, "无法打开浏览器", "Could not open browser")
                    ));
                }
            }
            let prev = state.last_lifecycle;
            state.sync_engine();
            let now = state.snapshot.lifecycle;
            // Profile-level auto-reconnect after an unexpected drop while the app is active.
            let want_reconnect = !state.user_disconnect
                && !state.dry_run
                && prev.is_active()
                && matches!(
                    now,
                    ConnectionLifecycle::Disconnected | ConnectionLifecycle::Failed
                )
                && state
                    .active_connection()
                    .map(|c| c.connect_on_demand)
                    .unwrap_or(false);
            state.last_lifecycle = now;
            if want_reconnect {
                // Silent reconnect; home status already updates via lifecycle.
                return start_connect(state, &s).and(Command::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    },
                    |_| Action::TickSession,
                ));
            }
            let _ = shared_engine().tick();
            state.sync_engine();
            state.last_lifecycle = state.snapshot.lifecycle;
            // Poll faster while connecting/authenticating so challenge sheets
            // appear promptly after OpenConnect posts a form.
            let fast_poll = state.snapshot.pending_auth.is_some()
                || matches!(
                    state.snapshot.lifecycle,
                    ConnectionLifecycle::Connecting
                        | ConnectionLifecycle::Authenticating
                        | ConnectionLifecycle::Establishing
                );
            let delay_ms = if fast_poll { 250 } else { 1_000 };
            Command::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                },
                |_| Action::TickSession,
            )
        }
        Action::SetChallengeField { name, value } => {
            state.challenge_values.insert(name, value);
            Command::none()
        }
        Action::SubmitChallenge => {
            let Some(challenge) = state.snapshot.pending_auth.clone() else {
                state.push_toast(tr_msg(
                    state.locale,
                    "当前没有待处理的认证表单",
                    "No authentication form is waiting",
                ));
                return Command::none();
            };
            // Require non-empty values for required interactive fields (including Unknown OTP).
            for field in &challenge.fields {
                if matches!(field.kind, hanyconnect_core::AuthFieldKind::Hidden) {
                    continue;
                }
                if !field.required {
                    continue;
                }
                let value = state
                    .challenge_values
                    .get(&field.name)
                    .map(|s| s.trim())
                    .unwrap_or("");
                if value.is_empty() {
                    state.push_toast(tr_msg(
                        state.locale,
                        "请填写动态口令/验证码后再点继续",
                        "Enter the OTP / SMS code, then tap Continue",
                    ));
                    return Command::none();
                }
            }
            let reply = AuthChallengeReply {
                id: challenge.id,
                values: state
                    .challenge_values
                    .iter()
                    .filter(|(_, value)| !value.trim().is_empty())
                    .map(|(name, value)| AuthFieldValue {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                cancelled: false,
            };
            if reply.values.is_empty() {
                state.push_toast(tr_msg(
                    state.locale,
                    "请填写动态口令/验证码后再点继续",
                    "Enter the OTP / SMS code, then tap Continue",
                ));
                return Command::none();
            }
            match shared_engine().submit_auth_challenge(reply) {
                Ok(()) => {
                    // Clear local draft; next challenge round re-seeds. No toast noise.
                    state.challenge_values.clear();
                    state.challenge_seed_id = None;
                    state.snapshot.pending_auth = None;
                    // Avoid re-hydrating the just-submitted challenge from disk.
                    if let Ok(mut snap) = shared_engine().snapshot() {
                        snap.pending_auth = None;
                        state.snapshot = snap;
                    }
                }
                Err(err) => state.push_toast(err.to_string()),
            }
            // Keep polling for the next MFA round or connect result.
            Command::none()
        }
        Action::CancelChallenge => {
            // Authentication runs in the UI process; abort its waiter. The
            // in-flight connect command reports the cancelled result.
            state.user_disconnect = true;
            let _ = shared_engine().cancel_auth_challenge();
            state.challenge_values.clear();
            state.challenge_seed_id = None;
            state.snapshot.pending_auth = None;
            Command::none()
        }
        Action::OpenEditor { id } => {
            state.editor_open = true;
            state.editor_show_advanced = false;
            state.draft = if let Some(id) = id {
                state
                    .snapshot
                    .connections
                    .iter()
                    .find(|connection| connection.id == id)
                    .cloned()
                    .unwrap_or_else(VpnConnection::new_draft)
            } else {
                VpnConnection::new_draft()
            };
            state.group_choices.clear();
            state.group_discovery_loading = false;
            state.group_discovery_error = None;
            let server = state.draft.server.trim().to_owned();
            if state.dry_run || !server_looks_ready(&server) {
                Command::none()
            } else {
                Command::perform(group_discovery_delay(), move |_| Action::DiscoverGroups {
                    server,
                })
            }
        }
        Action::SetDraftName(value) => {
            state.draft.name = value;
            Command::none()
        }
        Action::SetDraftServer(value) => {
            state.draft.server = value;
            state.group_choices.clear();
            state.group_discovery_loading = false;
            state.group_discovery_error = None;
            let server = state.draft.server.trim().to_owned();
            if state.dry_run || !server_looks_ready(&server) {
                Command::none()
            } else {
                Command::perform(group_discovery_delay(), move |_| Action::DiscoverGroups {
                    server,
                })
            }
        }
        Action::DiscoverGroups { server } => {
            if !state.editor_open
                || state.draft.server.trim() != server
                || !server_looks_ready(&server)
            {
                return Command::none();
            }
            state.group_discovery_loading = true;
            state.group_discovery_error = None;
            let profile = state.draft.clone();
            let result_server = server.clone();
            Command::perform(discover_groups(profile), move |result| {
                Action::GroupsDiscovered {
                    server: result_server,
                    result,
                }
            })
        }
        Action::GroupsDiscovered { server, result } => {
            if !state.editor_open || state.draft.server.trim() != server {
                return Command::none();
            }
            state.group_discovery_loading = false;
            match result {
                Ok(mut discovery) => {
                    discovery
                        .groups
                        .retain(|group| !group.name.trim().is_empty());
                    discovery
                        .groups
                        .dedup_by(|left, right| left.name == right.name);
                    let requested = state.draft.group.trim().to_owned();
                    let mut warning = None;
                    if let Some(group) = discovery.groups.iter().find(|group| {
                        group.name.eq_ignore_ascii_case(&requested)
                            || group.label.trim().eq_ignore_ascii_case(&requested)
                    }) {
                        // Persist/submit the protocol value, never the display label.
                        state.draft.group = group.name.clone();
                    } else {
                        let fallback = discovery
                            .selected
                            .as_deref()
                            .and_then(|selected| {
                                discovery.groups.iter().find(|group| group.name == selected)
                            })
                            .or_else(|| discovery.groups.first());
                        if let Some(group) = fallback {
                            state.draft.group = group.name.clone();
                            if !requested.is_empty() {
                                warning = Some(tr_msg(
                                    state.locale,
                                    "原分组不在服务器列表中，已切换到服务器默认分组",
                                    "The configured group is unavailable; using the server default",
                                ));
                            }
                        }
                    }
                    state.group_choices = discovery.groups;
                    state.group_discovery_error = warning;
                }
                Err(err) => {
                    state.group_choices.clear();
                    state.group_discovery_error = Some(format!(
                        "{}: {err}",
                        tr_msg(
                            state.locale,
                            "未能自动获取分组，可手动填写",
                            "Could not fetch groups; manual entry is available",
                        )
                    ));
                }
            }
            Command::none()
        }
        Action::SetDraftGroup(value) => {
            state.draft.group = value;
            Command::none()
        }
        Action::SetDraftUsername(value) => {
            state.draft.username = value;
            Command::none()
        }
        Action::SetDraftPassword(value) => {
            state.draft.password = value;
            Command::none()
        }
        Action::SetDraftProtocol(protocol) => {
            state.draft.protocol = protocol;
            Command::none()
        }
        Action::SetDraftAuthMethod(method) => {
            state.draft.auth_method = method;
            // SAML defaults to system-browser SSO (OpenConnect SSO-v2).
            if matches!(method, AuthMethod::Saml) {
                state.draft.external_browser_auth = true;
            }
            Command::none()
        }
        Action::SetDraftCertificate(value) => {
            state.draft.certificate = value;
            Command::none()
        }
        Action::SetDraftBackupServers(value) => {
            state.draft.backup_servers = value;
            Command::none()
        }
        Action::SetDraftStrictCertificateTrust(value) => {
            state.draft.strict_certificate_trust = value;
            Command::none()
        }
        Action::SetDraftBlockUntrustedServers(value) => {
            state.draft.block_untrusted_servers = value;
            Command::none()
        }
        Action::SetDraftAllowLocalLan(value) => {
            state.draft.allow_local_lan = value;
            Command::none()
        }
        Action::SetDraftForceGlobal(value) => {
            state.draft.force_global = value;
            Command::none()
        }
        Action::SetDraftUseDtls(value) => {
            state.draft.use_dtls = value;
            Command::none()
        }
        Action::SetDraftReportedOs(value) => {
            state.draft.reported_os = value;
            Command::none()
        }
        Action::SetDraftSni(value) => {
            state.draft.sni = value;
            Command::none()
        }
        Action::SetDraftRequirePfs(value) => {
            state.draft.require_pfs = value;
            Command::none()
        }
        Action::SetDraftDisableXmlPost(value) => {
            state.draft.disable_xml_post = value;
            Command::none()
        }
        Action::SetDraftDpdSeconds(value) => {
            state.draft.dpd_seconds = value.trim().parse().unwrap_or(0);
            Command::none()
        }
        Action::SetDraftSoftwareToken(value) => {
            state.draft.software_token = value;
            Command::none()
        }
        Action::SetDraftTokenString(value) => {
            state.draft.token_string = value;
            Command::none()
        }
        Action::SetDraftSplitTunnelMode(value) => {
            state.draft.split_tunnel_mode = value;
            Command::none()
        }
        Action::SetDraftSplitTunnelNetworks(value) => {
            state.draft.split_tunnel_networks = value;
            Command::none()
        }
        Action::SetDraftPrivateKey(value) => {
            state.draft.private_key = value;
            Command::none()
        }
        Action::SetDraftSecondaryCertificate(value) => {
            state.draft.secondary_certificate = value;
            Command::none()
        }
        Action::SetDraftSecondaryPrivateKey(value) => {
            state.draft.secondary_private_key = value;
            Command::none()
        }
        Action::SetDraftCaCertificate(value) => {
            state.draft.ca_certificate = value;
            Command::none()
        }
        Action::SetDraftKeyPassword(value) => {
            state.draft.key_password = value;
            Command::none()
        }
        Action::SetDraftSecondaryKeyPassword(value) => {
            state.draft.secondary_key_password = value;
            Command::none()
        }
        Action::SetDraftHttpProxy(value) => {
            state.draft.http_proxy = value;
            Command::none()
        }
        Action::SetDraftServerCertHash(value) => {
            state.draft.server_cert_hash = value;
            Command::none()
        }
        Action::SetDraftTrustedApplications(value) => {
            state.draft.trusted_applications = value;
            Command::none()
        }
        Action::SetDraftBlockedApplications(value) => {
            state.draft.blocked_applications = value;
            Command::none()
        }
        Action::SetDraftCsdWrapper(value) => {
            state.draft.csd_wrapper = value;
            Command::none()
        }
        Action::SetDraftUserAgent(value) => {
            state.draft.user_agent = value;
            Command::none()
        }
        Action::SetDraftClientVersion(value) => {
            state.draft.client_version = value;
            Command::none()
        }
        Action::SetDraftAllowInsecureCrypto(value) => {
            state.draft.allow_insecure_crypto = value;
            Command::none()
        }
        Action::SetEditorShowAdvanced(value) => {
            state.editor_show_advanced = value;
            Command::none()
        }
        Action::PickCertFile(kind) => {
            Command::perform(platform_callbacks::pick_cert_file(kind), move |result| {
                Action::CertFilePicked { kind, result }
            })
        }
        Action::CertFilePicked { kind, result } => {
            match result {
                Ok(path) => {
                    match kind {
                        platform_callbacks::CertFileKind::Certificate => {
                            state.draft.certificate = path;
                        }
                        platform_callbacks::CertFileKind::PrivateKey => {
                            state.draft.private_key = path;
                        }
                        platform_callbacks::CertFileKind::CaCertificate => {
                            state.draft.ca_certificate = path;
                        }
                    }
                    // Path shown in the form; no toast.
                }
                Err(err) if err.contains("cancelled") => {}
                Err(err) => state.push_toast(err),
            }
            Command::none()
        }
        Action::SetDraftConnectOnDemand(value) => {
            state.draft.connect_on_demand = value;
            Command::none()
        }
        Action::SetDraftExternalBrowserAuth(value) => {
            state.draft.external_browser_auth = value;
            Command::none()
        }
        Action::SetDraftFipsMode(value) => {
            state.draft.fips_mode = value;
            Command::none()
        }
        Action::SetDraftMtu(value) => {
            let trimmed = value.trim();
            state.draft.mtu = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
                0
            } else {
                trimmed.parse::<u32>().unwrap_or(0)
            };
            Command::none()
        }
        Action::SetDraftFavorite(favorite) => {
            state.draft.favorite = favorite;
            Command::none()
        }
        Action::SaveDraft => {
            let name = state.draft.name.trim().to_owned();
            let server = state.draft.server.trim().to_owned();
            if name.is_empty() || server.is_empty() {
                state.push_toast(s.form_required);
                return Command::none();
            }
            state.draft.name = name;
            state.draft.server = server;
            if state.draft.id.is_empty() {
                state.draft.id = format!(
                    "conn-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                );
            }
            match shared_engine().upsert_profile(state.draft.clone()) {
                Ok(()) => {
                    state.editor_open = false;
                    state.draft = VpnConnection::new_draft();
                    state.sync_engine();
                }
                Err(err) => state.push_toast(err.to_string()),
            }
            Command::none()
        }
        Action::DeleteConnection(id) => {
            match shared_engine().delete_profile(&id) {
                Ok(()) => {
                    state.push_toast(s.feedback_deleted);
                    state.sync_engine();
                }
                Err(err) => state.push_toast(err.to_string()),
            }
            Command::none()
        }
        Action::ToggleFavorite(id) => {
            if let Some(mut connection) = state
                .snapshot
                .connections
                .iter()
                .find(|connection| connection.id == id)
                .cloned()
            {
                connection.favorite = !connection.favorite;
                let _ = shared_engine().upsert_profile(connection);
                state.sync_engine();
            }
            Command::none()
        }
        Action::ClearDiagnostics => {
            if let Err(err) = shared_engine().clear_diagnostics() {
                state.push_toast(err.to_string());
            }
            state.sync_engine();
            Command::none()
        }
        Action::DismissToast(id) => {
            state.toasts.retain(|toast| toast.id != id);
            Command::none()
        }
    }
}

/// Shared connect path for user toggle and unexpected-drop auto-reconnect.
fn start_connect(state: &mut State, s: &crate::l10n::UiStrings) -> Command<Action> {
    let Some(active) = state.active_connection().cloned() else {
        state.push_toast(s.no_connection);
        return Command::none();
    };
    if active.server.trim().is_empty() {
        state.push_toast(s.form_required);
        return Command::none();
    }
    // Password / cert required for non-SAML auth (passwords are stored locally in test phase).
    if !state.dry_run
        && matches!(
            active.auth_method,
            AuthMethod::Password | AuthMethod::PasswordAndCertificate
        )
        && active.password.trim().is_empty()
    {
        state.push_toast(tr_msg(
            state.locale,
            "请先填写密码",
            "Enter the password first",
        ));
        return Command::none();
    }
    if !state.dry_run
        && matches!(
            active.auth_method,
            AuthMethod::Certificate | AuthMethod::PasswordAndCertificate
        )
        && active.certificate.trim().is_empty()
    {
        state.push_toast(tr_msg(
            state.locale,
            "请先选择客户端证书文件",
            "Select a client certificate file first",
        ));
        return Command::none();
    }
    state.user_disconnect = false;
    state.snapshot.lifecycle = ConnectionLifecycle::Connecting;
    state.snapshot.last_error = None;
    state.snapshot.stats = SessionStats::default();
    state.challenge_values.clear();
    state.challenge_seed_id = None;
    state.last_lifecycle = ConnectionLifecycle::Connecting;
    hanyconnect_core::clear_browser_open_pending();
    let dry_run = state.dry_run;
    Command::perform(engine_connect(active, dry_run), Action::ConnectionFinished)
}

async fn engine_connect(profile: VpnConnection, dry_run: bool) -> Result<SessionOutcome, String> {
    let engine = shared_engine();
    let options = engine
        .prepare_connect(ConnectRequest { profile, dry_run })
        .await
        .map_err(|err| err.to_string())?;
    let options_json = serde_json::to_string(&options).map_err(|err| err.to_string())?;

    // Platform VPN extension owns the TUN (paws-style, separate process).
    match platform_callbacks::request_start_vpn(options_json) {
        Ok(()) => Ok(SessionOutcome::PlatformStartRequested),
        // Host/unit paths without ArkTS callbacks may use dry-run only.
        Err(_err) if dry_run => {
            let _ = engine.set_platform_vpn_running(true);
            let snap = engine.snapshot().map_err(|e| e.to_string())?;
            Ok(SessionOutcome::Connected(snap.stats))
        }
        Err(err) => {
            let _ = engine.set_platform_vpn_failed(err.clone());
            Err(format!(
                "{err} (VPN extension callback missing — real connect needs platform shell)"
            ))
        }
    }
}

async fn engine_disconnect(dry_run: bool) -> Result<SessionOutcome, String> {
    let engine = shared_engine();
    match platform_callbacks::request_stop_vpn() {
        Ok(()) => {
            engine.disconnect().await.map_err(|err| err.to_string())?;
            Ok(SessionOutcome::Disconnected)
        }
        Err(_) if dry_run => {
            engine.disconnect().await.map_err(|err| err.to_string())?;
            let _ = engine.set_platform_vpn_running(false);
            Ok(SessionOutcome::Disconnected)
        }
        Err(err) => {
            // Still tear down native/local state even if stop callback is gone.
            let _ = engine.disconnect().await;
            let _ = engine.set_platform_vpn_running(false);
            Err(err)
        }
    }
}

async fn session_tick_delay() {
    tokio::time::sleep(Duration::from_secs(1)).await;
}

async fn group_discovery_delay() {
    // Debounce text edits; stale server values are rejected by the reducer
    // before any network request is started.
    tokio::time::sleep(Duration::from_millis(700)).await;
}

async fn discover_groups(
    profile: VpnConnection,
) -> Result<hanyconnect_core::AuthGroupDiscovery, String> {
    shared_engine()
        .discover_auth_groups(profile)
        .await
        .map_err(|err| err.to_string())
}

fn server_looks_ready(server: &str) -> bool {
    let server = server.trim();
    server.len() >= 3
        && !server.chars().any(char::is_whitespace)
        && !server.ends_with('.')
        && !server.ends_with('/')
}

fn detect_system_locale() -> UiLocale {
    std::env::var("HANYCONNECT_UI_LOCALE")
        .or_else(|_| std::env::var("HMETA_UI_LOCALE"))
        .map(|value| UiLocale::from_tag(&value))
        .unwrap_or_default()
}

fn detect_system_dark() -> bool {
    std::env::var("HANYCONNECT_SYSTEM_COLOR_MODE")
        .or_else(|_| std::env::var("HMETA_SYSTEM_COLOR_MODE"))
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        == Some(2)
}

fn tr_msg(locale: UiLocale, zh: &str, en: &str) -> String {
    match locale {
        UiLocale::ZhCn => zh.to_owned(),
        UiLocale::En => en.to_owned(),
    }
}

#[allow(dead_code)]
fn engine() -> std::sync::Arc<SessionEngine> {
    shared_engine()
}
