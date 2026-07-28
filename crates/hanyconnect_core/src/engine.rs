use crate::auth_bridge::AuthInteraction;
use crate::e2e::{e2e_marker, E2eConfig};
use crate::error::{CoreError, CoreResult};
use crate::model::{
    AuthChallenge, AuthChallengeReply, AuthGroupDiscovery, ConnectRequest, ConnectionLifecycle,
    ConnectionProfile, DiagnosticEntry, NetworkSnapshot, SessionSnapshot, SessionStats, VpnOptions,
};
use crate::platform_state::{PlatformVpnState, SessionHandoff};
use crate::store::ProfileStore;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

#[cfg(feature = "native-anyconnect")]
use crate::native_session::{
    authenticate as native_authenticate, resume_from_options, spawn_mainloop, PendingNativeSession,
    RunningNativeSession,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "native-anyconnect")]
const BACKEND: &str = "anyconnect-rs";
#[cfg(not(feature = "native-anyconnect"))]
const BACKEND: &str = "platform-orchestrator";

static ENGINE: OnceLock<Arc<SessionEngine>> = OnceLock::new();

pub fn shared_engine() -> Arc<SessionEngine> {
    ENGINE
        .get_or_init(|| Arc::new(SessionEngine::new()))
        .clone()
}

struct Inner {
    home: PathBuf,
    snapshot: SessionSnapshot,
    store: Option<ProfileStore>,
    connected_at: Option<Instant>,
    generation: u64,
    platform_vpn_running: bool,
    platform_vpn_starting: bool,
    last_vpn_options: VpnOptions,
    e2e: E2eConfig,
    #[cfg(feature = "native-anyconnect")]
    pending_native: Option<PendingNativeSession>,
    #[cfg(feature = "native-anyconnect")]
    running_native: Option<RunningNativeSession>,
}

pub struct SessionEngine {
    inner: Mutex<Inner>,
    lifecycle_tx: watch::Sender<ConnectionLifecycle>,
    auth: Arc<AuthInteraction>,
}

impl SessionEngine {
    pub fn new() -> Self {
        let (lifecycle_tx, _) = watch::channel(ConnectionLifecycle::Disconnected);
        let auth = AuthInteraction::shared();
        Self {
            inner: Mutex::new(Inner {
                home: PathBuf::from("."),
                snapshot: seed_snapshot(),
                store: None,
                connected_at: None,
                generation: 0,
                platform_vpn_running: false,
                platform_vpn_starting: false,
                last_vpn_options: VpnOptions::default(),
                e2e: E2eConfig::default(),
                #[cfg(feature = "native-anyconnect")]
                pending_native: None,
                #[cfg(feature = "native-anyconnect")]
                running_native: None,
            }),
            lifecycle_tx,
            auth,
        }
    }

    pub fn configure_home(&self, home: impl Into<PathBuf>) -> CoreResult<()> {
        let home = home.into();
        let store = ProfileStore::open(&home)?;
        // Seed demo profiles only on first run (file missing). An empty
        // connections.json is intentional (user deleted everything) and must
        // not re-inject mock Corporate HQ / Lab Network entries.
        let mut profiles = store.load()?;
        if !store.profiles_file_exists() {
            profiles = seed_profiles();
            store.save(&profiles)?;
        }
        let preferences = store.load_preferences().unwrap_or_default();
        let mut inner = self.lock()?;
        inner.home = home;
        inner.store = Some(store);
        inner.snapshot.connections = profiles;
        // Prefer persisted selection; fall back to first profile; never keep a
        // stale seed id (e.g. "demo-hq") that is not in the loaded list.
        let resolved_active = preferences
            .active_connection_id
            .filter(|id| inner.snapshot.connections.iter().any(|p| p.id == *id))
            .or_else(|| {
                inner
                    .snapshot
                    .active_connection_id
                    .clone()
                    .filter(|id| inner.snapshot.connections.iter().any(|p| p.id == *id))
            })
            .or_else(|| inner.snapshot.connections.first().map(|p| p.id.clone()));
        inner.snapshot.active_connection_id = resolved_active;
        self.persist_preferences_locked(&inner)?;
        // Absorb VPN-extension process state written under the same home.
        self.sync_platform_locked(&mut inner);
        e2e_marker("home_configured", inner.home.display().to_string());
        Ok(())
    }

    pub fn snapshot(&self) -> CoreResult<SessionSnapshot> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        self.refresh_stats_locked(&mut inner);
        inner.snapshot.pending_auth = self.auth.pending();
        Ok(inner.snapshot.clone())
    }

    /// Pending interactive auth form, if the OpenConnect worker is blocked on UI.
    pub fn pending_auth(&self) -> Option<AuthChallenge> {
        self.auth.pending()
    }

    /// Submit field values for the current challenge (unblocks the auth worker).
    pub fn submit_auth_challenge(&self, reply: AuthChallengeReply) -> CoreResult<()> {
        self.auth.submit(reply)?;
        e2e_marker("auth_challenge_submitted", "ok");
        Ok(())
    }

    /// Cancel the current challenge (and the in-flight connect).
    pub fn cancel_auth_challenge(&self) -> CoreResult<()> {
        self.auth.abort();
        if let Ok(mut inner) = self.lock() {
            inner.snapshot.pending_auth = None;
        }
        e2e_marker("auth_challenge_cancelled", "ok");
        Ok(())
    }

    pub fn snapshot_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string(&self.snapshot()?)?)
    }

    pub fn set_profiles(&self, profiles: Vec<ConnectionProfile>) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if let Some(store) = &inner.store {
            store.save(&profiles)?;
        }
        inner.snapshot.connections = profiles;
        Ok(())
    }

    pub fn upsert_profile(&self, profile: ConnectionProfile) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if let Some(existing) = inner
            .snapshot
            .connections
            .iter_mut()
            .find(|item| item.id == profile.id)
        {
            *existing = profile;
        } else {
            // New profiles become the active selection (AnyConnect-like).
            let new_id = profile.id.clone();
            inner.snapshot.connections.insert(0, profile);
            if inner.snapshot.active_connection_id.is_none() {
                inner.snapshot.active_connection_id = Some(new_id);
            }
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        e2e_marker("profile_saved", "ok");
        Ok(())
    }

    /// Toggle full-tunnel routing on the active profile.
    ///
    /// Persists immediately. When a session is up, also rewrites
    /// `last_vpn_options` / session handoff so a reconnect (or extension
    /// restart) picks up `0.0.0.0/0` without re-auth for credentials.
    /// System VPN routes only change after the extension recreates the TUN.
    pub fn set_active_force_global(&self, force_global: bool) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        let id = inner
            .snapshot
            .active_connection_id
            .clone()
            .ok_or_else(|| CoreError::msg("no active connection"))?;
        let profile = inner
            .snapshot
            .connections
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| CoreError::msg(format!("unknown profile {id}")))?;
        profile.force_global = force_global;
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;

        let session_live = inner.snapshot.lifecycle.is_active()
            || inner.platform_vpn_running
            || inner.platform_vpn_starting;
        if session_live {
            // Preserve cookie / credentials while rewriting route policy.
            let cookie = inner.last_vpn_options.cookie.clone();
            let server = inner.last_vpn_options.server.clone();
            let username = inner.last_vpn_options.username.clone();
            let password = inner.last_vpn_options.password.clone();
            let group = inner.last_vpn_options.group.clone();
            let accept_untrusted = inner.last_vpn_options.accept_untrusted;
            let profile = inner
                .snapshot
                .connections
                .iter()
                .find(|item| item.id == id)
                .cloned();
            if let Some(profile) = profile {
                let mut rebuilt = VpnOptions::from_network(&inner.snapshot.network, &profile);
                // Keep addresses/DNS/MTU from the live session when the network
                // snapshot is incomplete (common right after cookie resume).
                if rebuilt.addresses.is_empty() {
                    rebuilt.addresses = inner.last_vpn_options.addresses.clone();
                }
                if rebuilt.dns_addresses.is_empty() {
                    rebuilt.dns_addresses = inner.last_vpn_options.dns_addresses.clone();
                }
                if rebuilt.mtu == 0 {
                    rebuilt.mtu = inner.last_vpn_options.mtu;
                }
                rebuilt.cookie = cookie;
                rebuilt.server = server;
                rebuilt.username = username;
                rebuilt.password = password;
                rebuilt.group = group;
                rebuilt.accept_untrusted = accept_untrusted;
                rebuilt.force_global = force_global;
                rebuilt.apply_force_global();
                inner.last_vpn_options = rebuilt;
            } else {
                inner.last_vpn_options.force_global = force_global;
                inner.last_vpn_options.apply_force_global();
            }
            let handoff = SessionHandoff {
                options: inner.last_vpn_options.clone(),
                network: inner.snapshot.network.clone(),
                updated_at: PlatformVpnState::now_nanos(),
            };
            let _ = handoff.save(&inner.home);
            let first_route = inner.last_vpn_options.routes.first().cloned();
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("force_global={force_global} routes={first_route:?}"),
            );
        }
        e2e_marker(
            "force_global_set",
            format!("value={force_global} live={session_live}"),
        );
        Ok(session_live)
    }

    pub fn delete_profile(&self, id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        let existed = inner.snapshot.connections.iter().any(|item| item.id == id);
        if !existed {
            // Idempotent: UI may re-fire; never panic on a missing id.
            e2e_marker("profile_deleted", format!("missing:{id}"));
            return Ok(());
        }
        inner.snapshot.connections.retain(|item| item.id != id);
        if inner.snapshot.active_connection_id.as_deref() == Some(id) {
            inner.snapshot.active_connection_id =
                inner.snapshot.connections.first().map(|p| p.id.clone());
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        e2e_marker("profile_deleted", id);
        Ok(())
    }

    pub fn select_profile(&self, id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if !inner.snapshot.connections.iter().any(|item| item.id == id) {
            return Err(CoreError::msg(format!("unknown profile {id}")));
        }
        if inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active() {
            return Err(CoreError::msg(
                "disconnect before changing the active profile",
            ));
        }
        inner.snapshot.active_connection_id = Some(id.to_owned());
        self.persist_preferences_locked(&inner)?;
        e2e_marker("profile_selected", id);
        Ok(())
    }

    pub fn apply_e2e_config(&self, config: E2eConfig) -> CoreResult<String> {
        let mut inner = self.lock()?;
        if let Some(server) = config.server.clone() {
            let mut profile = ConnectionProfile::new_draft();
            profile.id = "e2e-profile".to_owned();
            profile.name = config
                .name
                .clone()
                .unwrap_or_else(|| "E2E Connection".to_owned());
            profile.server = server;
            profile.group = config.group.clone().unwrap_or_default();
            profile.username = config.username.clone().unwrap_or_default();
            profile.password = config.password.clone().unwrap_or_default();
            // Local/lab headends (e.g. scripts/dev-ocserv.sh) use self-signed
            // certs; reject-by-default would always fail obtain_cookie.
            profile.strict_certificate_trust = false;
            profile.block_untrusted_servers = false;
            if let Some(existing) = inner
                .snapshot
                .connections
                .iter_mut()
                .find(|item| item.id == profile.id)
            {
                *existing = profile.clone();
            } else {
                inner.snapshot.connections.insert(0, profile.clone());
            }
            inner.snapshot.active_connection_id = Some(profile.id);
            if let Some(store) = &inner.store {
                store.save(&inner.snapshot.connections)?;
            }
        }
        inner.e2e = config.clone();
        e2e_marker(
            "e2e_config_applied",
            format!(
                "server={} auto={} dryRun={}",
                config.server.as_deref().unwrap_or(""),
                config.auto_connect,
                config.dry_run
            ),
        );
        Ok(serde_json::to_string(&config)?)
    }

    pub fn e2e_config(&self) -> CoreResult<E2eConfig> {
        Ok(self.lock()?.e2e.clone())
    }

    pub fn active_profile(&self) -> CoreResult<Option<ConnectionProfile>> {
        let inner = self.lock()?;
        let id = inner.snapshot.active_connection_id.clone();
        Ok(id.and_then(|id| {
            inner
                .snapshot
                .connections
                .iter()
                .find(|item| item.id == id)
                .cloned()
        }))
    }

    /// Read the initial authentication form for a server so the editor can
    /// present the exact group values advertised by the headend.
    pub async fn discover_auth_groups(
        &self,
        profile: ConnectionProfile,
    ) -> CoreResult<AuthGroupDiscovery> {
        #[cfg(feature = "native-anyconnect")]
        {
            tokio::task::spawn_blocking(move || {
                crate::native_session::discover_auth_groups(&profile)
            })
            .await
            .map_err(|err| CoreError::msg(format!("group discovery task failed: {err}")))?
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let _ = profile;
            Err(CoreError::msg(
                "authentication group discovery requires native AnyConnect support",
            ))
        }
    }

    pub fn default_vpn_options_json(&self) -> CoreResult<String> {
        let inner = self.lock()?;
        Ok(serde_json::to_string(&inner.last_vpn_options)?)
    }

    pub fn set_platform_vpn_starting(&self, starting: bool) -> CoreResult<()> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        // Extension already marked running — do not regress to "starting".
        if starting && inner.platform_vpn_running {
            return Ok(());
        }
        inner.platform_vpn_starting = starting;
        if starting {
            inner.platform_vpn_running = false;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
            self.push_diag_locked(&mut inner, "info", "platform VPN extension starting");
            e2e_marker("platform_vpn_starting", "true");
        }
        self.persist_platform_locked(&inner)?;
        Ok(())
    }

    pub fn set_platform_vpn_running(&self, running: bool) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            let stop = self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(5));
        }
        Ok(())
    }

    pub fn set_platform_vpn_failed(&self, error: String) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            let stop = self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(2));
        }
        Ok(())
    }

    #[cfg(feature = "native-anyconnect")]
    fn apply_platform_vpn_running_locked(
        &self,
        inner: &mut Inner,
        running: bool,
    ) -> CoreResult<Option<RunningNativeSession>> {
        inner.platform_vpn_running = running;
        inner.platform_vpn_starting = false;
        if running {
            if !inner.snapshot.lifecycle.is_active() {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                inner.connected_at = Some(Instant::now());
            }
            self.push_diag_locked(inner, "info", "platform VPN TUN is up");
            e2e_marker("platform_vpn_running", "true");
            e2e_marker("session_connected", BACKEND);
            return Ok(None);
        }
        let stop = if matches!(
            inner.snapshot.lifecycle,
            ConnectionLifecycle::Connected
                | ConnectionLifecycle::Establishing
                | ConnectionLifecycle::Disconnecting
        ) {
            let session = inner.running_native.take();
            inner.pending_native = None;
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            inner.snapshot.stats = SessionStats::default();
            e2e_marker("platform_vpn_running", "false");
            e2e_marker("session_disconnected", "platform");
            session
        } else {
            None
        };
        Ok(stop)
    }

    #[cfg(not(feature = "native-anyconnect"))]
    fn apply_platform_vpn_running_locked(
        &self,
        inner: &mut Inner,
        running: bool,
    ) -> CoreResult<()> {
        inner.platform_vpn_running = running;
        inner.platform_vpn_starting = false;
        if running {
            if !inner.snapshot.lifecycle.is_active() {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                inner.connected_at = Some(Instant::now());
            }
            self.push_diag_locked(inner, "info", "platform VPN TUN is up");
            e2e_marker("platform_vpn_running", "true");
            e2e_marker("session_connected", BACKEND);
        } else if matches!(
            inner.snapshot.lifecycle,
            ConnectionLifecycle::Connected
                | ConnectionLifecycle::Establishing
                | ConnectionLifecycle::Disconnecting
        ) {
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            inner.snapshot.stats = SessionStats::default();
            e2e_marker("platform_vpn_running", "false");
            e2e_marker("session_disconnected", "platform");
        }
        Ok(())
    }

    #[cfg(feature = "native-anyconnect")]
    fn apply_platform_vpn_failed_locked(
        &self,
        inner: &mut Inner,
        error: String,
    ) -> CoreResult<Option<RunningNativeSession>> {
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        let session = inner.running_native.take();
        inner.pending_native = None;
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(error.clone()));
        self.push_diag_locked(inner, "error", error.clone());
        e2e_marker("platform_vpn_failed", error);
        Ok(session)
    }

    #[cfg(not(feature = "native-anyconnect"))]
    fn apply_platform_vpn_failed_locked(&self, inner: &mut Inner, error: String) -> CoreResult<()> {
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(error.clone()));
        self.push_diag_locked(inner, "error", error.clone());
        e2e_marker("platform_vpn_failed", error);
        Ok(())
    }

    pub fn expire_platform_vpn_start(&self) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        // VPN extension may have already written running=true to the shared file.
        self.sync_platform_locked(&mut inner);
        if !inner.platform_vpn_starting || inner.platform_vpn_running {
            return Ok(false);
        }
        #[cfg(feature = "native-anyconnect")]
        {
            if inner.running_native.is_some() {
                return Ok(false);
            }
        }
        inner.platform_vpn_starting = false;
        #[cfg(feature = "native-anyconnect")]
        {
            inner.pending_native = None;
        }
        self.set_lifecycle_locked(
            &mut inner,
            ConnectionLifecycle::Failed,
            Some("VPN extension startup timed out".to_owned()),
        );
        e2e_marker("platform_vpn_timeout", "true");
        self.persist_platform_locked(&inner)?;
        Ok(true)
    }

    /// Authenticate (anyconnect-rs when enabled) and produce VPN options.
    /// The Harmony shell must then start VpnExtensionAbility and pass the TUN fd
    /// to [`SessionEngine::attach_tun`].
    pub async fn prepare_connect(&self, request: ConnectRequest) -> CoreResult<VpnOptions> {
        let generation = {
            let mut inner = self.lock()?;
            if inner.snapshot.lifecycle.is_busy() {
                return Err(CoreError::msg("connection already in progress"));
            }
            #[cfg(feature = "native-anyconnect")]
            if inner.running_native.is_some() || inner.pending_native.is_some() {
                return Err(CoreError::msg("native session already active"));
            }
            // Fresh interactive-auth session for this connect attempt.
            self.auth.begin_session();
            inner.snapshot.pending_auth = None;
            // Drop stale platform-vpn-state.json from a previous session so the
            // UI does not flash "connected" before the extension is up.
            inner.platform_vpn_running = false;
            inner.platform_vpn_starting = false;
            SessionHandoff::clear(&inner.home);
            let _ = PlatformVpnState {
                starting: false,
                running: false,
                owner_pid: std::process::id(),
                lifecycle: ConnectionLifecycle::Connecting,
                last_error: None,
                assigned_ip: String::new(),
                gateway: String::new(),
                mtu: 0,
                network: NetworkSnapshot::default(),
                stats: SessionStats::default(),
                updated_at: PlatformVpnState::now_nanos(),
            }
            .save(&inner.home);
            inner.generation += 1;
            let generation = inner.generation;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connecting, None);
            inner.snapshot.stats = SessionStats::default();
            self.push_diag_locked(
                &mut inner,
                "info",
                format!(
                    "prepare connect to {} (backend={BACKEND}, dry_run={})",
                    request.profile.server, request.dry_run
                ),
            );
            e2e_marker(
                "connect_prepare",
                format!(
                    "server={} dryRun={} backend={BACKEND}",
                    request.profile.server, request.dry_run
                ),
            );
            generation
        };

        let result = if request.dry_run {
            self.prepare_dry_run(&request.profile).await
        } else {
            self.prepare_native(&request.profile).await
        };

        let mut inner = self.lock()?;
        if inner.generation != generation {
            return Err(CoreError::msg("stale connect generation"));
        }
        match result {
            Ok(prepared) => {
                inner.snapshot.pending_auth = None;
                inner.snapshot.network = prepared.network.clone();
                inner.last_vpn_options = prepared.options.clone();
                inner.snapshot.stats.assigned_ip = prepared
                    .network
                    .address
                    .clone()
                    .unwrap_or_else(|| "pending".to_owned());
                inner.snapshot.stats.gateway = prepared
                    .network
                    .gateway
                    .clone()
                    .unwrap_or_else(|| request.profile.server.clone());
                inner.snapshot.stats.mtu = if prepared.network.mtu > 0 {
                    prepared.network.mtu as u32
                } else {
                    prepared.options.mtu
                };
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.pending_native = prepared.pending;
                }
                inner.platform_vpn_starting = true;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
                self.push_diag_locked(
                    &mut inner,
                    "info",
                    format!(
                        "auth ready; assigned={:?} mtu={}",
                        prepared.network.address, prepared.options.mtu
                    ),
                );
                e2e_marker(
                    "connect_auth_ok",
                    format!(
                        "ip={:?} cookie={}",
                        prepared.network.address.clone().unwrap_or_default(),
                        prepared.options.cookie.is_some()
                    ),
                );
                // Write full options (incl. cookie) for the VPN-extension process.
                // Want parameters are size-limited and may drop large cookies.
                let handoff = SessionHandoff {
                    options: prepared.options.clone(),
                    network: prepared.network.clone(),
                    updated_at: PlatformVpnState::now_nanos(),
                };
                handoff.save(&inner.home)?;
                e2e_marker(
                    "session_handoff_saved",
                    format!(
                        "cookie={} server={}",
                        handoff.options.cookie.is_some(),
                        handoff.options.server.as_deref().unwrap_or("")
                    ),
                );
                self.persist_platform_locked(&inner)?;
                // Prefer compact options for Want (network only is enough for TUN create).
                Ok(prepared.options)
            }
            Err(err) => {
                let message = err.to_string();
                self.auth.abort();
                inner.snapshot.pending_auth = None;
                inner.platform_vpn_starting = false;
                inner.platform_vpn_running = false;
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.pending_native = None;
                }
                self.set_lifecycle_locked(
                    &mut inner,
                    ConnectionLifecycle::Failed,
                    Some(message.clone()),
                );
                self.push_diag_locked(&mut inner, "error", message.clone());
                e2e_marker("connect_auth_failed", message.clone());
                // Persist so device file pull / next snapshot sees Failed + lastError
                // (previously left lifecycle stuck at "connecting").
                let _ = self.persist_platform_locked(&inner);
                let _ = write_last_error(&inner.home, &message);
                Err(err)
            }
        }
    }

    /// VPN-extension process: resume the UI-authenticated cookie, establish
    /// CSTP, keep the Client as `pending_native`, and return the live network
    /// configuration used to create the system TUN.
    pub async fn prepare_in_extension(&self, options_json: &str) -> CoreResult<String> {
        #[cfg(feature = "native-anyconnect")]
        {
            let home = self.lock()?.home.clone();
            let mut options = SessionHandoff::load(&home).map(|h| h.options);
            if options.is_none() {
                if let Ok(parsed) = serde_json::from_str::<VpnOptions>(options_json) {
                    if parsed.server.is_some() || parsed.cookie.is_some() {
                        options = Some(parsed);
                    }
                }
            }
            let options = options.ok_or_else(|| {
                CoreError::msg("extension prepare: no session handoff (connect from UI first)")
            })?;
            e2e_marker(
                "extension_prepare_start",
                format!(
                    "server={} user={} pass={}",
                    options.server.as_deref().unwrap_or(""),
                    options.username.as_deref().unwrap_or(""),
                    options
                        .password
                        .as_ref()
                        .map(|p| !p.is_empty())
                        .unwrap_or(false)
                ),
            );
            let pending = tokio::task::spawn_blocking(move || resume_from_options(&options))
                .await
                .map_err(|err| CoreError::msg(format!("extension prepare join: {err}")))??;

            let mut inner = self.lock()?;
            inner.snapshot.network = pending.network.clone();
            inner.last_vpn_options = pending.options.clone();
            inner.snapshot.stats.assigned_ip = pending
                .network
                .address
                .clone()
                .unwrap_or_else(|| "pending".to_owned());
            inner.snapshot.stats.gateway = pending.network.gateway.clone().unwrap_or_default();
            inner.snapshot.stats.mtu = if pending.network.mtu > 0 {
                pending.network.mtu as u32
            } else {
                pending.options.mtu
            };
            let json = serde_json::to_string(&pending.options)?;
            // Keep Client alive until attach_tun(fd) in this same process.
            inner.pending_native = Some(pending);
            inner.platform_vpn_starting = true;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
            self.persist_platform_locked(&inner)?;
            e2e_marker("extension_prepare_ok", "pending_client_ready");
            Ok(json)
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let _ = options_json;
            Err(CoreError::msg(
                "prepare_in_extension requires native-anyconnect",
            ))
        }
    }

    /// Hand the platform TUN fd to OpenConnect and start the packet mainloop.
    ///
    /// On HarmonyOS the VPN extension runs in a **separate process**.
    /// [`SessionEngine::prepare_in_extension`] resumes the authenticated cookie
    /// in that process before this method attaches the platform TUN.
    pub async fn attach_tun(&self, fd: i32, options_json: &str) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        {
            let options_json = options_json.to_owned();
            let pending = {
                let mut inner = self.lock()?;
                inner.pending_native.take()
            };

            let pending = if let Some(pending) = pending {
                e2e_marker("attach_tun_pending", "same_process");
                pending
            } else {
                // Prefer on-disk handoff (full cookie) over Want JSON (may truncate).
                let home = self.lock()?.home.clone();
                let from_file = SessionHandoff::load(&home);
                let mut options = from_file.as_ref().map(|h| h.options.clone());
                if options.is_none() {
                    if let Ok(parsed) = serde_json::from_str::<VpnOptions>(&options_json) {
                        if parsed.server.is_some() || parsed.cookie.is_some() {
                            options = Some(parsed);
                        }
                    }
                }
                let Some(options) = options else {
                    let mut inner = self.lock()?;
                    self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connected, None);
                    inner.connected_at = Some(Instant::now());
                    inner.platform_vpn_running = true;
                    inner.platform_vpn_starting = false;
                    e2e_marker("attach_tun_platform_only", format!("fd={fd}"));
                    e2e_marker("session_connected", "platform-only");
                    self.persist_platform_locked(&inner)?;
                    return Ok(());
                };
                e2e_marker(
                    "attach_tun_resume",
                    format!(
                        "cookie={} server={} from_file={}",
                        options.cookie.is_some(),
                        options.server.as_deref().unwrap_or(""),
                        from_file.is_some()
                    ),
                );
                tokio::task::spawn_blocking(move || resume_from_options(&options))
                    .await
                    .map_err(|err| CoreError::msg(format!("resume worker join failed: {err}")))??
            };

            let network = pending.network.clone();
            let options = pending.options.clone();
            let running = tokio::task::spawn_blocking(move || spawn_mainloop(pending, fd))
                .await
                .map_err(|err| CoreError::msg(format!("attach worker join failed: {err}")))??;

            let mut inner = self.lock()?;
            inner.snapshot.network = network.clone();
            inner.last_vpn_options = options;
            inner.snapshot.stats.assigned_ip = network
                .address
                .clone()
                .unwrap_or_else(|| "pending".to_owned());
            inner.snapshot.stats.gateway = network.gateway.clone().unwrap_or_default();
            inner.snapshot.stats.mtu = if network.mtu > 0 {
                network.mtu as u32
            } else {
                inner.snapshot.stats.mtu
            };
            running.seed_network_meta(
                inner.snapshot.stats.assigned_ip.clone(),
                inner.snapshot.stats.gateway.clone(),
                inner.snapshot.stats.mtu,
            );
            inner.running_native = Some(running);
            inner.platform_vpn_running = true;
            inner.platform_vpn_starting = false;
            inner.connected_at = Some(Instant::now());
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connected, None);
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("OpenConnect mainloop attached to TUN fd {fd}"),
            );
            e2e_marker("session_connected", "anyconnect-rs");
            e2e_marker("attach_tun_ok", format!("fd={fd}"));
            // Critical: UI process reads this file to leave "establishing".
            self.persist_platform_locked(&inner)?;
            return Ok(());
        }

        #[cfg(not(feature = "native-anyconnect"))]
        {
            let _ = options_json;
            let mut inner = self.lock()?;
            inner.platform_vpn_running = true;
            inner.platform_vpn_starting = false;
            inner.connected_at = Some(Instant::now());
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connected, None);
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform TUN fd {fd} accepted (no native OpenConnect)"),
            );
            e2e_marker("attach_tun_platform_only", format!("fd={fd}"));
            e2e_marker("session_connected", "platform");
            self.persist_platform_locked(&inner)?;
            Ok(())
        }
    }

    pub async fn disconnect(&self) -> CoreResult<()> {
        {
            let mut inner = self.lock()?;
            if matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnected) {
                return Ok(());
            }
            // Unblock any auth worker waiting on a challenge form.
            self.auth.abort();
            inner.snapshot.pending_auth = None;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnecting, None);
            self.push_diag_locked(&mut inner, "info", "disconnect requested");
            e2e_marker("disconnect_requested", "true");
            inner.generation += 1;
        }

        #[cfg(feature = "native-anyconnect")]
        {
            let running = {
                let mut inner = self.lock()?;
                let pending = inner.pending_native.take();
                drop(pending);
                inner.running_native.take()
            };
            if let Some(running) = running {
                running.cancel();
                let join_result =
                    tokio::task::spawn_blocking(move || running.join(Duration::from_secs(8))).await;
                match join_result {
                    Ok(Ok(())) => e2e_marker("native_mainloop_stopped", "ok"),
                    Ok(Err(err)) => e2e_marker("native_mainloop_stopped", err.to_string()),
                    Err(err) => e2e_marker("native_mainloop_stopped", format!("join:{err}")),
                }
            }
        }

        // Platform stop is asynchronous; mark disconnected if extension already gone.
        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let mut inner = self.lock()?;
            if !inner.platform_vpn_running
                || matches!(
                    inner.snapshot.lifecycle,
                    ConnectionLifecycle::Disconnecting | ConnectionLifecycle::Connected
                )
            {
                inner.platform_vpn_running = false;
                inner.platform_vpn_starting = false;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
                inner.connected_at = None;
                inner.snapshot.stats = SessionStats::default();
                inner.snapshot.network = NetworkSnapshot::default();
                e2e_marker("session_disconnected", "local");
                SessionHandoff::clear(&inner.home);
                let _ = self.persist_platform_locked(&inner);
            }
        }
        Ok(())
    }

    /// Called when the mainloop thread exits unexpectedly (peer hangup, error).
    pub fn on_native_session_ended(&self, error: Option<String>) -> CoreResult<()> {
        let mut inner = self.lock()?;
        #[cfg(feature = "native-anyconnect")]
        {
            if let Some(running) = inner.running_native.take() {
                // Thread already finished; do not join again from here.
                drop(running);
            }
            inner.pending_native = None;
        }
        inner.platform_vpn_running = false;
        inner.platform_vpn_starting = false;
        if let Some(message) = error {
            self.set_lifecycle_locked(
                &mut inner,
                ConnectionLifecycle::Failed,
                Some(message.clone()),
            );
            self.push_diag_locked(&mut inner, "error", message.clone());
            e2e_marker("session_ended_error", message);
        } else if !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnected) {
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
            e2e_marker("session_ended", "clean");
        }
        inner.connected_at = None;
        self.persist_platform_locked(&inner)?;
        Ok(())
    }

    pub fn tick(&self) -> CoreResult<SessionSnapshot> {
        let mut inner = self.lock()?;
        // UI process: pick up Connected/Failed written by the VPN extension process.
        self.sync_platform_locked(&mut inner);
        self.refresh_stats_locked(&mut inner);

        #[cfg(feature = "native-anyconnect")]
        {
            if let Some(running) = inner.running_native.as_ref() {
                if running.is_finished() {
                    // Mainloop exited; surface disconnect on next tick.
                    let finished = inner.running_native.take();
                    if let Some(finished) = finished {
                        match finished.join(Duration::from_millis(1)) {
                            Ok(()) => {
                                self.set_lifecycle_locked(
                                    &mut inner,
                                    ConnectionLifecycle::Disconnected,
                                    None,
                                );
                                inner.platform_vpn_running = false;
                                inner.connected_at = None;
                                e2e_marker("session_ended", "mainloop");
                                let _ = self.persist_platform_locked(&inner);
                            }
                            Err(err) => {
                                let message = err.to_string();
                                self.set_lifecycle_locked(
                                    &mut inner,
                                    ConnectionLifecycle::Failed,
                                    Some(message.clone()),
                                );
                                inner.platform_vpn_running = false;
                                inner.connected_at = None;
                                e2e_marker("session_ended_error", message);
                                let _ = self.persist_platform_locked(&inner);
                            }
                        }
                    }
                } else {
                    running.request_stats();
                    let mut traffic = running.traffic_snapshot();
                    traffic.assigned_ip = inner.snapshot.stats.assigned_ip.clone();
                    traffic.gateway = inner.snapshot.stats.gateway.clone();
                    traffic.mtu = inner.snapshot.stats.mtu;
                    if let Some(started) = inner.connected_at {
                        traffic.connected_seconds = started.elapsed().as_secs();
                    }
                    inner.snapshot.stats = traffic;
                    // Publish telemetry for the UI process.
                    let _ = self.persist_platform_locked(&inner);
                }
                return Ok(inner.snapshot.clone());
            }
        }

        if inner.snapshot.lifecycle.is_active() {
            // Dry-run / platform-only: synthetic traffic so the stats page moves.
            let stats = &mut inner.snapshot.stats;
            stats.bytes_sent = stats.bytes_sent.saturating_add(4096);
            stats.bytes_received = stats.bytes_received.saturating_add(16384);
            stats.packets_sent = stats.packets_sent.saturating_add(8);
            stats.packets_received = stats.packets_received.saturating_add(24);
        }
        Ok(inner.snapshot.clone())
    }

    pub fn subscribe_lifecycle(&self) -> watch::Receiver<ConnectionLifecycle> {
        self.lifecycle_tx.subscribe()
    }

    async fn prepare_dry_run(&self, profile: &ConnectionProfile) -> CoreResult<PreparedConnect> {
        e2e_marker("backend_dry_run", profile.server.clone());
        tokio::time::sleep(Duration::from_millis(400)).await;
        if profile.server.contains("invalid") {
            return Err(CoreError::msg(
                "dry-run failure: server host contains 'invalid'",
            ));
        }
        let network = NetworkSnapshot {
            address: Some("10.64.12.48".to_owned()),
            netmask: Some("255.255.255.0".to_owned()),
            address_v6: None,
            netmask_v6: None,
            gateway: Some(profile.server.clone()),
            dns: vec!["1.1.1.1".to_owned()],
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
        let options = VpnOptions::from_network(&network, profile);
        Ok(PreparedConnect {
            network,
            options,
            #[cfg(feature = "native-anyconnect")]
            pending: None,
        })
    }

    async fn prepare_native(&self, profile: &ConnectionProfile) -> CoreResult<PreparedConnect> {
        #[cfg(feature = "native-anyconnect")]
        {
            {
                let mut inner = self.lock()?;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Authenticating, None);
                self.push_diag_locked(
                    &mut inner,
                    "info",
                    "authenticate in UI process and hand off the resulting cookie",
                );
            }
            let profile = profile.clone();
            let interaction = Arc::clone(&self.auth);
            let pending = tokio::task::spawn_blocking(move || {
                native_authenticate(&profile, Some(interaction))
            })
            .await
            .map_err(|err| CoreError::msg(format!("authentication worker join: {err}")))??;
            let network = pending.network.clone();
            let options = pending.options.clone();
            Ok(PreparedConnect {
                network,
                options,
                pending: Some(pending),
            })
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            e2e_marker("backend_platform", profile.server.clone());
            let url = profile.server_url();
            if url.is_empty() {
                return Err(CoreError::msg("server address is empty"));
            }
            {
                let mut inner = self.lock()?;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Authenticating, None);
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
            if profile.server.contains("invalid") {
                return Err(CoreError::msg("server host rejected"));
            }
            let network = NetworkSnapshot {
                address: Some("10.64.12.48".to_owned()),
                netmask: Some("255.255.255.255".to_owned()),
                address_v6: None,
                netmask_v6: None,
                gateway: Some(profile.server.clone()),
                dns: vec!["1.1.1.1".to_owned(), "8.8.8.8".to_owned()],
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
            let options = VpnOptions::from_network(&network, profile);
            e2e_marker(
                "backend_platform_ready",
                format!("url={url} mtu={}", options.mtu),
            );
            Ok(PreparedConnect { network, options })
        }
    }

    fn refresh_stats_locked(&self, inner: &mut Inner) {
        if let Some(started) = inner.connected_at {
            if inner.snapshot.lifecycle.is_active() {
                inner.snapshot.stats.connected_seconds = started.elapsed().as_secs();
            }
        }
    }

    /// Read platform VPN flags written by the sibling process (UI ↔ extension).
    fn sync_platform_locked(&self, inner: &mut Inner) {
        let Some(file) = PlatformVpnState::load(&inner.home) else {
            return;
        };

        #[cfg(feature = "native-anyconnect")]
        let local_mainloop = inner.running_native.is_some();
        #[cfg(not(feature = "native-anyconnect"))]
        let local_mainloop = false;

        let was_running = inner.platform_vpn_running;
        let was_starting = inner.platform_vpn_starting;
        let owner_alive = file.owner_is_alive();
        let running = local_mainloop || (file.running && owner_alive);
        let starting = !running && file.starting && owner_alive;
        inner.platform_vpn_starting = starting;
        inner.platform_vpn_running = running;

        if running {
            if !file.assigned_ip.is_empty() {
                inner.snapshot.stats.assigned_ip = file.assigned_ip.clone();
            }
            if !file.gateway.is_empty() {
                inner.snapshot.stats.gateway = file.gateway.clone();
            }
            if file.mtu > 0 {
                inner.snapshot.stats.mtu = file.mtu;
            }
            if file.network.address.is_some() || !file.network.dns.is_empty() {
                inner.snapshot.network = file.network.clone();
            }
            // Prefer richer stats from the extension (bytes counters).
            if file.stats.bytes_sent > inner.snapshot.stats.bytes_sent
                || file.stats.bytes_received > inner.snapshot.stats.bytes_received
            {
                let mut stats = file.stats.clone();
                if stats.assigned_ip.is_empty() {
                    stats.assigned_ip = inner.snapshot.stats.assigned_ip.clone();
                }
                if stats.gateway.is_empty() {
                    stats.gateway = inner.snapshot.stats.gateway.clone();
                }
                if stats.mtu == 0 {
                    stats.mtu = inner.snapshot.stats.mtu;
                }
                inner.snapshot.stats = stats;
            }
            if !inner.snapshot.lifecycle.is_active()
                && !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnecting)
            {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                if inner.connected_at.is_none() {
                    inner.connected_at = Some(Instant::now());
                }
            }
            if !was_running {
                e2e_marker("platform_vpn_sync", "running");
            }
        } else if starting {
            if !inner.snapshot.lifecycle.is_active()
                && !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Failed)
            {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Establishing, None);
            }
        } else if matches!(file.lifecycle, ConnectionLifecycle::Failed) || file.last_error.is_some()
        {
            if !matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Failed | ConnectionLifecycle::Disconnected
            ) {
                self.set_lifecycle_locked(
                    inner,
                    ConnectionLifecycle::Failed,
                    file.last_error.clone(),
                );
                e2e_marker(
                    "platform_vpn_sync",
                    format!("failed:{}", file.last_error.as_deref().unwrap_or("")),
                );
            }
        } else if (was_running || was_starting)
            && matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Connected | ConnectionLifecycle::Establishing
            )
        {
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            e2e_marker("platform_vpn_sync", "stopped");
        }
    }

    fn persist_platform_locked(&self, inner: &Inner) -> CoreResult<()> {
        let state = PlatformVpnState {
            starting: inner.platform_vpn_starting,
            running: inner.platform_vpn_running || {
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.running_native.is_some()
                }
                #[cfg(not(feature = "native-anyconnect"))]
                {
                    false
                }
            },
            owner_pid: std::process::id(),
            lifecycle: inner.snapshot.lifecycle,
            last_error: inner.snapshot.last_error.clone(),
            assigned_ip: inner.snapshot.stats.assigned_ip.clone(),
            gateway: inner.snapshot.stats.gateway.clone(),
            mtu: inner.snapshot.stats.mtu,
            network: inner.snapshot.network.clone(),
            stats: inner.snapshot.stats.clone(),
            updated_at: PlatformVpnState::now_nanos(),
        };
        state.save(&inner.home)?;
        Ok(())
    }

    fn set_lifecycle_locked(
        &self,
        inner: &mut Inner,
        lifecycle: ConnectionLifecycle,
        error: Option<String>,
    ) {
        inner.snapshot.lifecycle = lifecycle;
        inner.snapshot.last_error = error;
        let _ = self.lifecycle_tx.send(lifecycle);
    }

    fn push_diag_locked(&self, inner: &mut Inner, level: &str, message: impl Into<String>) {
        inner.snapshot.diagnostics.insert(
            0,
            DiagnosticEntry {
                level: level.to_owned(),
                message: message.into(),
                timestamp: now_timestamp(),
            },
        );
        if inner.snapshot.diagnostics.len() > 64 {
            inner.snapshot.diagnostics.truncate(64);
        }
    }

    fn persist_preferences_locked(&self, inner: &Inner) -> CoreResult<()> {
        let Some(store) = inner.store.as_ref() else {
            return Ok(());
        };
        store.save_preferences(&crate::store::Preferences {
            active_connection_id: inner.snapshot.active_connection_id.clone(),
        })
    }

    fn lock(&self) -> CoreResult<std::sync::MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|_| CoreError::msg("session engine lock poisoned"))
    }
}

impl Default for SessionEngine {
    fn default() -> Self {
        Self::new()
    }
}

struct PreparedConnect {
    network: NetworkSnapshot,
    options: VpnOptions,
    #[cfg(feature = "native-anyconnect")]
    pending: Option<PendingNativeSession>,
}

fn seed_snapshot() -> SessionSnapshot {
    // Pre-home bootstrap only. Real selection is restored in configure_home
    // from preferences.json; do not hard-pin demo-hq here or a restart will
    // re-select the mock profile even after the user chose something else.
    SessionSnapshot {
        lifecycle: ConnectionLifecycle::Disconnected,
        active_connection_id: None,
        connections: Vec::new(),
        stats: SessionStats::default(),
        network: NetworkSnapshot::default(),
        last_error: None,
        diagnostics: vec![DiagnosticEntry {
            level: "info".to_owned(),
            message: format!("session engine ready (backend={BACKEND})"),
            timestamp: now_timestamp(),
        }],
        app_version: APP_VERSION.to_owned(),
        sdk_ready: cfg!(feature = "native-anyconnect"),
        anyconnect_version: {
            #[cfg(feature = "native-anyconnect")]
            {
                Some(anyconnect::version())
            }
            #[cfg(not(feature = "native-anyconnect"))]
            {
                None
            }
        },
        backend: BACKEND.to_owned(),
        pending_auth: None,
    }
}

fn seed_profiles() -> Vec<ConnectionProfile> {
    let mut hq = ConnectionProfile::new_draft();
    hq.id = "demo-hq".to_owned();
    hq.name = "Corporate HQ".to_owned();
    hq.server = "vpn.example.com".to_owned();
    hq.group = "Employees".to_owned();
    hq.username = "demo.user".to_owned();
    hq.backup_servers = "vpn-backup.example.com".to_owned();
    hq.strict_certificate_trust = true;
    hq.block_untrusted_servers = true;
    hq.allow_local_lan = true;
    hq.mtu = 1400;
    hq.favorite = true;

    let mut lab = ConnectionProfile::new_draft();
    lab.id = "demo-lab".to_owned();
    lab.name = "Lab Network".to_owned();
    lab.server = "lab-vpn.example.com".to_owned();
    lab.group = "Engineering".to_owned();
    lab.username = "lab.user".to_owned();
    lab.protocol = crate::model::ProtocolKind::Ipsec;
    lab.auth_method = crate::model::AuthMethod::PasswordAndCertificate;
    lab.certificate = "Lab Client Cert".to_owned();
    lab.strict_certificate_trust = true;
    lab.block_untrusted_servers = true;
    lab.connect_on_demand = true;
    vec![hq, lab]
}

fn now_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn write_last_error(home: &std::path::Path, message: &str) -> CoreResult<()> {
    use std::io::Write;
    std::fs::create_dir_all(home)?;
    let path = home.join("last-connect-error.txt");
    let mut file = std::fs::File::create(path)?;
    writeln!(
        file,
        "{}\n{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        message
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProtocolKind;
    use tempfile::tempdir;

    #[tokio::test]
    async fn dry_run_prepare_connect_succeeds() {
        let engine = SessionEngine::new();
        let dir = tempdir().unwrap();
        engine.configure_home(dir.path()).unwrap();
        let mut profile = ConnectionProfile::new_draft();
        profile.id = "t1".to_owned();
        profile.name = "Test".to_owned();
        profile.server = "vpn.example.com".to_owned();
        profile.protocol = ProtocolKind::Ssl;
        let options = engine
            .prepare_connect(ConnectRequest {
                profile,
                dry_run: true,
            })
            .await
            .unwrap();
        assert!(!options.addresses.is_empty());
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.lifecycle, ConnectionLifecycle::Establishing);
    }

    #[tokio::test]
    async fn dry_run_invalid_server_fails() {
        let engine = SessionEngine::new();
        let dir = tempdir().unwrap();
        engine.configure_home(dir.path()).unwrap();
        let mut profile = ConnectionProfile::new_draft();
        profile.server = "invalid.example".to_owned();
        let err = engine
            .prepare_connect(ConnectRequest {
                profile,
                dry_run: true,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[tokio::test]
    async fn attach_tun_without_pending_marks_connected() {
        let engine = SessionEngine::new();
        let dir = tempfile::tempdir().unwrap();
        engine.configure_home(dir.path()).unwrap();
        engine.attach_tun(3, "{}").await.unwrap();
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.lifecycle, ConnectionLifecycle::Connected);
    }

    #[test]
    fn platform_vpn_state_is_shared_between_ui_and_extension_handles() {
        let dir = tempfile::tempdir().unwrap();
        let ui = SessionEngine::new();
        let extension = SessionEngine::new();
        ui.configure_home(dir.path()).unwrap();
        extension.configure_home(dir.path()).unwrap();

        ui.set_platform_vpn_starting(true).unwrap();
        let ext_snap = extension.snapshot().unwrap();
        assert_eq!(ext_snap.lifecycle, ConnectionLifecycle::Establishing);

        extension.set_platform_vpn_running(true).unwrap();
        let ui_snap = ui.snapshot().unwrap();
        assert_eq!(ui_snap.lifecycle, ConnectionLifecycle::Connected);
        assert!(!ui.expire_platform_vpn_start().unwrap());

        extension.set_platform_vpn_running(false).unwrap();
        let ui_snap = ui.snapshot().unwrap();
        assert_eq!(ui_snap.lifecycle, ConnectionLifecycle::Disconnected);
    }

    #[test]
    fn selected_profile_survives_restart() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
        let second = engine
            .snapshot()
            .unwrap()
            .connections
            .get(1)
            .expect("seed has two demos")
            .id
            .clone();
        engine.select_profile(&second).unwrap();
        assert_eq!(
            engine.snapshot().unwrap().active_connection_id.as_deref(),
            Some(second.as_str())
        );

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        assert_eq!(
            restarted
                .snapshot()
                .unwrap()
                .active_connection_id
                .as_deref(),
            Some(second.as_str())
        );
    }

    #[test]
    fn deleting_all_profiles_does_not_reseed_demos() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
        let ids: Vec<String> = engine
            .snapshot()
            .unwrap()
            .connections
            .into_iter()
            .map(|p| p.id)
            .collect();
        for id in ids {
            engine.delete_profile(&id).unwrap();
        }
        assert!(engine.snapshot().unwrap().connections.is_empty());
        // Idempotent delete must not panic.
        engine.delete_profile("missing-id").unwrap();

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        assert!(
            restarted.snapshot().unwrap().connections.is_empty(),
            "empty store must not re-inject mock Corporate HQ / Lab Network"
        );
    }

    #[test]
    fn configure_home_preserves_live_platform_starting_state() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
        let platform = PlatformVpnState {
            starting: true,
            running: false,
            owner_pid: std::process::id(),
            lifecycle: ConnectionLifecycle::Establishing,
            last_error: None,
            assigned_ip: String::new(),
            gateway: String::new(),
            mtu: 0,
            network: NetworkSnapshot::default(),
            stats: SessionStats::default(),
            updated_at: PlatformVpnState::now_nanos(),
        };
        platform.save(dir.path()).unwrap();

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        let snap = restarted.snapshot().unwrap();
        assert_eq!(snap.lifecycle, ConnectionLifecycle::Establishing);
        assert!(snap.last_error.is_none());
        let file = PlatformVpnState::load(dir.path()).unwrap();
        assert!(file.starting);
        assert!(!file.running);
        assert_eq!(file.lifecycle, ConnectionLifecycle::Establishing);
    }

    #[test]
    fn configure_home_preserves_platform_connected_state_and_handoff() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();

        let platform = PlatformVpnState {
            starting: false,
            running: true,
            owner_pid: std::process::id(),
            lifecycle: ConnectionLifecycle::Connected,
            last_error: None,
            assigned_ip: "10.1.2.3".to_owned(),
            gateway: "vpn.example.com".to_owned(),
            mtu: 1400,
            network: NetworkSnapshot::default(),
            stats: SessionStats::default(),
            updated_at: PlatformVpnState::now_nanos(),
        };
        platform.save(dir.path()).unwrap();
        SessionHandoff {
            options: crate::model::VpnOptions::default(),
            network: NetworkSnapshot::default(),
            updated_at: PlatformVpnState::now_nanos(),
        }
        .save(dir.path())
        .unwrap();

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        let snap = restarted.snapshot().unwrap();
        assert_eq!(snap.lifecycle, ConnectionLifecycle::Connected);
        assert!(snap.last_error.is_none());
        assert!(snap.pending_auth.is_none());
        assert_eq!(snap.stats.assigned_ip, "10.1.2.3");
        assert!(PlatformVpnState::load(dir.path()).unwrap().running);
        assert!(SessionHandoff::load(dir.path()).is_some());
    }

    #[test]
    fn configure_home_discards_active_state_from_dead_extension() {
        let dir = tempdir().unwrap();
        let platform = PlatformVpnState {
            starting: true,
            running: false,
            owner_pid: u32::MAX,
            lifecycle: ConnectionLifecycle::Establishing,
            last_error: None,
            assigned_ip: String::new(),
            gateway: String::new(),
            mtu: 0,
            network: NetworkSnapshot::default(),
            stats: SessionStats::default(),
            updated_at: PlatformVpnState::now_nanos(),
        };
        platform.save(dir.path()).unwrap();

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        let snap = restarted.snapshot().unwrap();
        assert_eq!(snap.lifecycle, ConnectionLifecycle::Disconnected);
        assert!(snap.last_error.is_none());
    }
}
