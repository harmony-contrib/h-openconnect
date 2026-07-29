use crate::auth_bridge::AuthInteraction;
use crate::error::{CoreError, CoreResult};
use crate::log_recording::{self, RecordedLogBuffer, RuntimeLogBuffer, MAX_IN_MEMORY_LOGS};
use crate::model::{
    AuthChallenge, AuthChallengeReply, AuthGroupDiscovery, ConnectRequest, ConnectionLifecycle,
    ConnectionProfile, DiagnosticEntry, NetworkSnapshot, SessionSnapshot, SessionStats, VpnOptions,
};
use crate::platform_ipc::{PlatformIpc, PlatformIpcError};
use crate::platform_state::{PlatformVpnState, SessionHandoff};
use crate::store::{Preferences, ProfileStore};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tracing::Level;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;

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
static RUNTIME_LOGS: LazyLock<Arc<Mutex<RuntimeLogBuffer>>> =
    LazyLock::new(|| Arc::new(Mutex::new(RuntimeLogBuffer::default())));
static INSTALL_RUNTIME_LOG_LAYER: Once = Once::new();

pub fn shared_engine() -> Arc<SessionEngine> {
    ENGINE
        .get_or_init(|| Arc::new(SessionEngine::new()))
        .clone()
}

struct Inner {
    home: PathBuf,
    snapshot: SessionSnapshot,
    store: Option<ProfileStore>,
    preferences: Preferences,
    connected_at: Option<Instant>,
    generation: u64,
    platform_vpn_running: bool,
    platform_vpn_starting: bool,
    platform_vpn_state_updated_at: u128,
    last_vpn_options: VpnOptions,
    logs: RecordedLogBuffer,
    platform_diagnostics: Vec<DiagnosticEntry>,
    #[cfg(feature = "native-anyconnect")]
    pending_native: Option<PendingNativeSession>,
    #[cfg(feature = "native-anyconnect")]
    running_native: Option<RunningNativeSession>,
}

pub struct SessionEngine {
    inner: Mutex<Inner>,
    platform_ipc: Mutex<Option<Arc<PlatformIpc>>>,
    lifecycle_tx: watch::Sender<ConnectionLifecycle>,
    auth: Arc<AuthInteraction>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformSharedMemoryFds {
    pub ashmem_fd: i32,
    pub notification_fd: i32,
}

impl SessionEngine {
    pub fn new() -> Self {
        install_runtime_log_layer();
        let (lifecycle_tx, _) = watch::channel(ConnectionLifecycle::Disconnected);
        let auth = AuthInteraction::shared();
        Self {
            inner: Mutex::new(Inner {
                home: PathBuf::from("."),
                snapshot: seed_snapshot(),
                store: None,
                preferences: Preferences::default(),
                connected_at: None,
                generation: 0,
                platform_vpn_running: false,
                platform_vpn_starting: false,
                platform_vpn_state_updated_at: 0,
                last_vpn_options: VpnOptions::default(),
                logs: RecordedLogBuffer::new("."),
                platform_diagnostics: Vec::new(),
                #[cfg(feature = "native-anyconnect")]
                pending_native: None,
                #[cfg(feature = "native-anyconnect")]
                running_native: None,
            }),
            platform_ipc: Mutex::new(None),
            lifecycle_tx,
            auth,
        }
    }

    pub fn initialize_platform_shared_memory(&self) -> CoreResult<PlatformSharedMemoryFds> {
        {
            let platform = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            if let Some(platform) = platform.as_ref() {
                let fds = platform.ui_fds().map_err(platform_ipc_error)?;
                return Ok(PlatformSharedMemoryFds {
                    ashmem_fd: fds.ashmem,
                    notification_fd: fds.notification,
                });
            }
        }

        let log_root = {
            let inner = self.lock()?;
            inner.home.clone()
        };
        log_recording::reset_recording(&log_root)?;
        if let Ok(mut logs) = RUNTIME_LOGS.lock() {
            logs.clear();
        }
        let (platform, fds) = PlatformIpc::create_ui().map_err(platform_ipc_error)?;
        {
            let mut slot = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            *slot = Some(platform);
        }
        let mut inner = self.lock()?;
        self.persist_platform_locked(&mut inner)?;
        Ok(PlatformSharedMemoryFds {
            ashmem_fd: fds.ashmem,
            notification_fd: fds.notification,
        })
    }

    pub fn attach_platform_shared_memory(
        &self,
        ashmem_fd: i32,
        notification_fd: i32,
    ) -> CoreResult<()> {
        let platform =
            PlatformIpc::attach_vpn_raw(ashmem_fd, notification_fd).map_err(platform_ipc_error)?;
        let previous = {
            let mut slot = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            slot.replace(platform)
        };
        // The extension process may be reused after the UI process restarts.
        // Always bind the latest Want descriptors before checking VPN state.
        drop(previous);
        self.sync_platform_changes()
    }

    pub fn sync_platform_changes(&self) -> CoreResult<()> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        Ok(())
    }

    pub async fn wait_for_platform_change(&self, timeout: Duration) -> CoreResult<bool> {
        let Some(platform) = self.platform_ipc()? else {
            tokio::time::sleep(timeout).await;
            return Ok(false);
        };
        tokio::task::spawn_blocking(move || platform.wait_for_change(timeout))
            .await
            .map_err(|error| CoreError::msg(format!("platform subscription task failed: {error}")))?
            .map_err(platform_ipc_error)
    }

    fn platform_ipc(&self) -> CoreResult<Option<Arc<PlatformIpc>>> {
        self.platform_ipc
            .lock()
            .map(|platform| platform.clone())
            .map_err(|_| CoreError::msg("platform IPC lock poisoned"))
    }

    pub fn configure_home(&self, home: impl Into<PathBuf>) -> CoreResult<()> {
        let home = home.into();
        let store = ProfileStore::open(&home)?;
        // Production starts empty. Demo/test profiles must be created by test
        // fixtures instead of leaking into the real application.
        let profiles = store.load()?;
        if !store.profiles_file_exists() {
            store.save(&profiles)?;
        }
        let preferences = store.load_preferences().unwrap_or_default();
        let mut inner = self.lock()?;
        inner.home = home.clone();
        inner.logs = RecordedLogBuffer::new(home);
        inner.platform_diagnostics.clear();
        inner.store = Some(store);
        inner.preferences = preferences.clone();
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
        // Absorb a sibling-process frame when this process has already
        // attached to the current ashmem session.
        self.sync_platform_locked(&mut inner);
        Ok(())
    }

    pub fn snapshot(&self) -> CoreResult<SessionSnapshot> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        self.refresh_stats_locked(&mut inner);
        inner.logs.sync_session();
        if let Ok(mut runtime_logs) = RUNTIME_LOGS.lock() {
            runtime_logs.sync(inner.logs.root());
        }
        inner.snapshot.diagnostics = if inner.logs.enabled() {
            merge_platform_logs(merged_logs(&inner.logs), &inner.platform_diagnostics)
        } else {
            Vec::new()
        };
        inner.snapshot.pending_auth = self.auth.pending();
        Ok(inner.snapshot.clone())
    }

    pub fn appearance_preferences(&self) -> CoreResult<(String, String)> {
        let inner = self.lock()?;
        Ok((
            inner.preferences.language.clone(),
            inner.preferences.theme.clone(),
        ))
    }

    pub fn set_appearance_preferences(&self, language: &str, theme: &str) -> CoreResult<()> {
        if !matches!(language, "system" | "zh-CN" | "en") {
            return Err(CoreError::msg("unsupported language preference"));
        }
        if !matches!(theme, "system" | "light" | "dark") {
            return Err(CoreError::msg("unsupported theme preference"));
        }
        let mut inner = self.lock()?;
        inner.preferences.language = language.to_owned();
        inner.preferences.theme = theme.to_owned();
        self.persist_preferences_locked(&inner)
    }

    /// Pending interactive auth form, if the OpenConnect worker is blocked on UI.
    pub fn pending_auth(&self) -> Option<AuthChallenge> {
        self.auth.pending()
    }

    /// Submit field values for the current challenge (unblocks the auth worker).
    pub fn submit_auth_challenge(&self, reply: AuthChallengeReply) -> CoreResult<()> {
        self.auth.submit(reply)?;
        Ok(())
    }

    /// Cancel the current challenge (and the in-flight connect).
    pub fn cancel_auth_challenge(&self) -> CoreResult<()> {
        self.auth.abort();
        if let Ok(mut inner) = self.lock() {
            inner.snapshot.pending_auth = None;
        }
        Ok(())
    }

    pub fn snapshot_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string(&self.snapshot()?)?)
    }

    pub fn clear_diagnostics(&self) -> CoreResult<()> {
        let mut inner = self.lock()?;
        inner.logs.clear();
        inner.platform_diagnostics.clear();
        inner.snapshot.diagnostics.clear();
        if let Ok(mut logs) = RUNTIME_LOGS.lock() {
            logs.clear();
        }
        for name in [
            "openconnect-progress.log",
            "openconnect-progress.log.1",
            "last-connect-error.txt",
        ] {
            let _ = std::fs::remove_file(inner.home.join(name));
        }
        Ok(())
    }

    pub fn log_recording_status(&self) -> CoreResult<crate::LogRecordingStatus> {
        let inner = self.lock()?;
        log_recording::recording_status(&inner.home)
    }

    pub fn set_log_recording_enabled(
        &self,
        enabled: bool,
    ) -> CoreResult<crate::LogRecordingStatus> {
        let mut inner = self.lock()?;
        let root = inner.home.clone();
        let was_enabled = log_recording::recording_status(&root)?.enabled;
        if was_enabled == enabled {
            return log_recording::recording_status(&root);
        }

        if enabled {
            inner.logs.clear();
            inner.platform_diagnostics.clear();
            inner.snapshot.diagnostics.clear();
            if let Ok(mut logs) = RUNTIME_LOGS.lock() {
                logs.clear();
            }
            log_recording::set_recording_enabled(&root, true)?;
            inner.logs.sync_session();
            self.push_diag_locked(&mut inner, "info", "log recording enabled");
        } else {
            self.push_diag_locked(&mut inner, "info", "log recording disabled");
            log_recording::set_recording_enabled(&root, false)?;
            inner.logs.sync_session();
            inner.platform_diagnostics.clear();
            inner.snapshot.diagnostics.clear();
            if let Ok(mut logs) = RUNTIME_LOGS.lock() {
                logs.clear();
            }
        }
        log_recording::recording_status(&root)
    }

    pub fn read_log_archive(&self, file_name: &str) -> CoreResult<String> {
        let inner = self.lock()?;
        log_recording::read_archive(&inner.home, file_name)
    }

    pub fn delete_log_archive(&self, file_name: &str) -> CoreResult<crate::LogRecordingStatus> {
        let inner = self.lock()?;
        log_recording::delete_archive(&inner.home, file_name)?;
        log_recording::recording_status(&inner.home)
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
        profile.validate().map_err(CoreError::msg)?;
        let mut inner = self.lock()?;
        let previous = inner
            .snapshot
            .connections
            .iter()
            .find(|item| item.id == profile.id)
            .cloned();
        let edits_live_profile = inner.snapshot.active_connection_id.as_deref()
            == Some(profile.id.as_str())
            && (inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active());
        if edits_live_profile {
            return Err(CoreError::msg(
                "disconnect before editing the active connection",
            ));
        }
        if let Some(existing) = inner
            .snapshot
            .connections
            .iter_mut()
            .find(|item| item.id == profile.id)
        {
            *existing = profile;
        } else {
            // New profiles become active unless another profile owns a live session.
            let new_id = profile.id.clone();
            inner.snapshot.connections.insert(0, profile);
            if !inner.snapshot.lifecycle.is_busy() && !inner.snapshot.lifecycle.is_active() {
                inner.snapshot.active_connection_id = Some(new_id);
            }
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        if let Some(previous) = previous {
            cleanup_unreferenced_profile_files(&inner, &previous);
        }
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
        Ok(session_live)
    }

    pub fn delete_profile(&self, id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if inner.snapshot.active_connection_id.as_deref() == Some(id)
            && (inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active())
        {
            return Err(CoreError::msg(
                "disconnect before deleting the active connection",
            ));
        }
        let removed = inner
            .snapshot
            .connections
            .iter()
            .find(|item| item.id == id)
            .cloned();
        let Some(removed) = removed else {
            // Idempotent: UI may re-fire; never panic on a missing id.
            return Ok(());
        };
        inner.snapshot.connections.retain(|item| item.id != id);
        if inner.snapshot.active_connection_id.as_deref() == Some(id) {
            inner.snapshot.active_connection_id =
                inner.snapshot.connections.first().map(|p| p.id.clone());
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        cleanup_unreferenced_profile_files(&inner, &removed);
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
        Ok(())
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
        }
        self.persist_platform_locked(&mut inner)?;
        Ok(())
    }

    pub fn set_platform_vpn_running(&self, running: bool) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            let stop = self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&mut inner)?;
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
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&mut inner)?;
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
        } else if matches!(
            inner.snapshot.lifecycle,
            ConnectionLifecycle::Connected
                | ConnectionLifecycle::Establishing
                | ConnectionLifecycle::Disconnecting
        ) {
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            inner.snapshot.stats = SessionStats::default();
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
        Ok(session)
    }

    #[cfg(not(feature = "native-anyconnect"))]
    fn apply_platform_vpn_failed_locked(&self, inner: &mut Inner, error: String) -> CoreResult<()> {
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(error.clone()));
        self.push_diag_locked(inner, "error", error.clone());
        Ok(())
    }

    pub fn expire_platform_vpn_start(&self) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        // VPN extension may already have published running=true to ashmem.
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
        self.persist_platform_locked(&mut inner)?;
        Ok(true)
    }

    /// Authenticate (anyconnect-rs when enabled) and produce VPN options.
    /// The Harmony shell must then start VpnExtensionAbility and pass the TUN fd
    /// to [`SessionEngine::attach_tun`].
    pub async fn prepare_connect(&self, request: ConnectRequest) -> CoreResult<VpnOptions> {
        request.profile.validate().map_err(CoreError::msg)?;
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
            // Reset the UI lane for a fresh ashmem session attempt.
            inner.platform_vpn_running = false;
            inner.platform_vpn_starting = false;
            SessionHandoff::clear(&inner.home);
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
            self.persist_platform_locked(&mut inner)?;
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
                // Write full options (incl. cookie) for the VPN-extension process.
                // Want parameters are size-limited and may drop large cookies.
                let handoff = SessionHandoff {
                    options: prepared.options.clone(),
                    network: prepared.network.clone(),
                    updated_at: PlatformVpnState::now_nanos(),
                };
                handoff.save(&inner.home)?;
                self.persist_platform_locked(&mut inner)?;
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
                // Publish the terminal state so the UI does not remain in
                // "connecting" when authentication or negotiation fails.
                let _ = self.persist_platform_locked(&mut inner);
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
            self.persist_platform_locked(&mut inner)?;
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
                let options = options.ok_or_else(|| {
                    CoreError::msg("attach TUN refused: authenticated session handoff is missing")
                })?;
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
            // Critical: the UI consumes this ashmem frame to leave
            // "establishing".
            self.persist_platform_locked(&mut inner)?;
            SessionHandoff::clear(&inner.home);
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
            self.persist_platform_locked(&mut inner)?;
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
                let _ =
                    tokio::task::spawn_blocking(move || running.join(Duration::from_secs(8))).await;
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
                SessionHandoff::clear(&inner.home);
                let _ = self.persist_platform_locked(&mut inner);
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
        } else if !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnected) {
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
        }
        inner.connected_at = None;
        self.persist_platform_locked(&mut inner)?;
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
                                let _ = self.persist_platform_locked(&mut inner);
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
                                let _ = self.persist_platform_locked(&mut inner);
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
                    let _ = self.persist_platform_locked(&mut inner);
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

    /// Read the newest sibling-process frame from the opposite ashmem lane.
    fn sync_platform_locked(&self, inner: &mut Inner) {
        let Some(platform) = self.platform_ipc().ok().flatten() else {
            return;
        };
        let envelope = match platform.read_remote() {
            Ok(Some(envelope)) => envelope,
            Ok(None) => return,
            Err(error) => {
                self.push_diag_locked(
                    inner,
                    "warn",
                    format!("read platform shared memory failed: {error}"),
                );
                return;
            }
        };
        let Some(remote) = envelope
            .state
            .filter(|state| state.updated_at > inner.platform_vpn_state_updated_at)
        else {
            return;
        };

        #[cfg(feature = "native-anyconnect")]
        let local_mainloop = inner.running_native.is_some();
        #[cfg(not(feature = "native-anyconnect"))]
        let local_mainloop = false;

        let was_running = inner.platform_vpn_running;
        let was_starting = inner.platform_vpn_starting;
        let running = local_mainloop || remote.running;
        let starting = !running && remote.starting;
        inner.platform_vpn_starting = starting;
        inner.platform_vpn_running = running;
        inner.platform_vpn_state_updated_at = remote.updated_at;
        inner.platform_diagnostics = remote.diagnostics.clone();

        if running {
            if !remote.assigned_ip.is_empty() {
                inner.snapshot.stats.assigned_ip = remote.assigned_ip.clone();
            }
            if !remote.gateway.is_empty() {
                inner.snapshot.stats.gateway = remote.gateway.clone();
            }
            if remote.mtu > 0 {
                inner.snapshot.stats.mtu = remote.mtu;
            }
            if remote.network.address.is_some() || !remote.network.dns.is_empty() {
                inner.snapshot.network = remote.network.clone();
            }
            // Prefer richer stats from the extension (bytes counters).
            if remote.stats.bytes_sent > inner.snapshot.stats.bytes_sent
                || remote.stats.bytes_received > inner.snapshot.stats.bytes_received
            {
                let mut stats = remote.stats.clone();
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
        } else if starting {
            if !inner.snapshot.lifecycle.is_active()
                && !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Failed)
            {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Establishing, None);
            }
        } else if matches!(remote.lifecycle, ConnectionLifecycle::Failed)
            || remote.last_error.is_some()
        {
            if !matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Failed | ConnectionLifecycle::Disconnected
            ) {
                self.set_lifecycle_locked(
                    inner,
                    ConnectionLifecycle::Failed,
                    remote.last_error.clone(),
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
        }
    }

    fn persist_platform_locked(&self, inner: &mut Inner) -> CoreResult<()> {
        // Device SystemTime can be coarser than nanoseconds. Always advance the
        // revision so a terminal state cannot be discarded as a duplicate.
        inner.platform_vpn_state_updated_at = PlatformVpnState::now_nanos()
            .max(inner.platform_vpn_state_updated_at.saturating_add(1));
        let Some(platform) = self.platform_ipc()? else {
            return Ok(());
        };
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
            lifecycle: inner.snapshot.lifecycle,
            last_error: inner.snapshot.last_error.clone(),
            assigned_ip: inner.snapshot.stats.assigned_ip.clone(),
            gateway: inner.snapshot.stats.gateway.clone(),
            mtu: inner.snapshot.stats.mtu,
            network: inner.snapshot.network.clone(),
            stats: inner.snapshot.stats.clone(),
            diagnostics: merged_logs(&inner.logs),
            updated_at: inner.platform_vpn_state_updated_at,
        };
        platform.publish_state(state).map_err(platform_ipc_error)
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
        inner.logs.push(DiagnosticEntry {
            level: level.to_owned(),
            message: message.into(),
            timestamp: now_timestamp(),
        });
        if inner.logs.enabled() {
            inner.snapshot.diagnostics = merged_logs(&inner.logs);
        }
    }

    fn persist_preferences_locked(&self, inner: &Inner) -> CoreResult<()> {
        let Some(store) = inner.store.as_ref() else {
            return Ok(());
        };
        let mut preferences = inner.preferences.clone();
        preferences.active_connection_id = inner.snapshot.active_connection_id.clone();
        store.save_preferences(&preferences)
    }

    fn lock(&self) -> CoreResult<std::sync::MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|_| CoreError::msg("session engine lock poisoned"))
    }
}

fn platform_ipc_error(error: PlatformIpcError) -> CoreError {
    CoreError::msg(error.to_string())
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
        diagnostics: Vec::new(),
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

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

fn install_runtime_log_layer() {
    INSTALL_RUNTIME_LOG_LAYER.call_once(|| {
        let subscriber = tracing_subscriber::registry().with(HAnyConnectLogLayer {
            logs: RUNTIME_LOGS.clone(),
        });
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

struct HAnyConnectLogLayer {
    logs: Arc<Mutex<RuntimeLogBuffer>>,
}

impl<S> Layer<S> for HAnyConnectLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !is_vpn_log_target(event.metadata().target()) {
            return;
        }
        let level = match *event.metadata().level() {
            Level::TRACE | Level::DEBUG => "debug",
            Level::INFO => "info",
            Level::WARN => "warning",
            Level::ERROR => "error",
        };
        let mut visitor = LogMessageVisitor::default();
        event.record(&mut visitor);
        if let Ok(mut logs) = self.logs.lock() {
            logs.capture(DiagnosticEntry {
                level: level.to_owned(),
                message: visitor.finish(event.metadata().target()),
                timestamp: now_timestamp(),
            });
        }
    }
}

fn is_vpn_log_target(target: &str) -> bool {
    target.starts_with("hanyconnect_core")
        || target.starts_with("anyconnect")
        || target.starts_with("openconnect")
}

#[derive(Default)]
struct LogMessageVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl LogMessageVisitor {
    fn finish(self, fallback: &str) -> String {
        let mut message = self.message.unwrap_or_else(|| fallback.to_owned());
        if !self.fields.is_empty() {
            message.push_str(" · ");
            message.push_str(&self.fields.join(", "));
        }
        message
    }
}

impl tracing::field::Visit for LogMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(value.trim_matches('"').to_owned());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

fn merged_logs(state_logs: &[DiagnosticEntry]) -> Vec<DiagnosticEntry> {
    let state_start = state_logs.len().saturating_sub(MAX_IN_MEMORY_LOGS);
    let mut logs = state_logs[state_start..].to_vec();
    let remaining = MAX_IN_MEMORY_LOGS.saturating_sub(logs.len());
    if let Ok(runtime_logs) = RUNTIME_LOGS.lock() {
        let runtime_start = runtime_logs.len().saturating_sub(remaining);
        logs.extend(runtime_logs.entries().skip(runtime_start).cloned());
    }
    logs
}

fn merge_platform_logs(
    mut local: Vec<DiagnosticEntry>,
    platform: &[DiagnosticEntry],
) -> Vec<DiagnosticEntry> {
    for entry in platform {
        if !local.iter().any(|existing| {
            existing.level == entry.level
                && existing.message == entry.message
                && existing.timestamp == entry.timestamp
        }) {
            local.push(entry.clone());
        }
    }
    if local.len() > MAX_IN_MEMORY_LOGS {
        local.drain(..local.len() - MAX_IN_MEMORY_LOGS);
    }
    local
}

fn write_last_error(home: &std::path::Path, message: &str) -> CoreResult<()> {
    let path = home.join("last-connect-error.txt");
    let content = format!(
        "{}\n{}\n",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        message
    );
    crate::private_fs::write_atomic_private(&path, content.as_bytes())
}

fn cleanup_unreferenced_profile_files(inner: &Inner, removed: &ConnectionProfile) {
    let cert_root = inner.home.join("certs");
    let candidates = [
        removed.certificate.as_str(),
        removed.private_key.as_str(),
        removed.secondary_certificate.as_str(),
        removed.secondary_private_key.as_str(),
        removed.ca_certificate.as_str(),
    ];
    for candidate in candidates {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            continue;
        }
        let path = std::path::Path::new(candidate);
        if path.parent() != Some(cert_root.as_path())
            || profile_path_is_referenced(&inner.snapshot.connections, candidate)
        {
            continue;
        }
        let _ = std::fs::remove_file(path);
    }
}

fn profile_path_is_referenced(profiles: &[ConnectionProfile], candidate: &str) -> bool {
    profiles.iter().any(|profile| {
        [
            profile.certificate.as_str(),
            profile.private_key.as_str(),
            profile.secondary_certificate.as_str(),
            profile.secondary_private_key.as_str(),
            profile.ca_certificate.as_str(),
        ]
        .contains(&candidate)
    })
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
        profile.name = "Invalid".to_owned();
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
    #[cfg(feature = "native-anyconnect")]
    async fn attach_tun_without_pending_is_rejected() {
        let engine = SessionEngine::new();
        let dir = tempfile::tempdir().unwrap();
        engine.configure_home(dir.path()).unwrap();
        let error = engine.attach_tun(3, "{}").await.unwrap_err();
        assert!(error.to_string().contains("handoff"));
    }

    #[test]
    fn platform_vpn_state_revision_is_strictly_monotonic() {
        let dir = tempfile::tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
        let future_revision = PlatformVpnState::now_nanos().saturating_add(3_600_000_000_000);
        {
            let mut inner = engine.lock().unwrap();
            inner.platform_vpn_state_updated_at = future_revision;
        }

        engine.set_platform_vpn_starting(true).unwrap();

        let revision = engine.lock().unwrap().platform_vpn_state_updated_at;
        assert_eq!(revision, future_revision + 1);
    }

    #[test]
    fn selected_profile_survives_restart() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
        let mut first = ConnectionProfile::new_draft();
        first.id = "first".to_owned();
        first.name = "First".to_owned();
        first.server = "vpn-a.example.test".to_owned();
        engine.upsert_profile(first).unwrap();
        let mut second_profile = ConnectionProfile::new_draft();
        second_profile.id = "second".to_owned();
        second_profile.name = "Second".to_owned();
        second_profile.server = "vpn-b.example.test".to_owned();
        let second = second_profile.id.clone();
        engine.upsert_profile(second_profile).unwrap();
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
    fn production_store_starts_empty_and_stays_empty() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();
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
    fn log_recording_is_opt_in_and_archives_the_enabled_session() {
        let dir = tempdir().unwrap();
        let engine = SessionEngine::new();
        engine.configure_home(dir.path()).unwrap();

        assert!(!engine.log_recording_status().unwrap().enabled);
        assert!(engine.snapshot().unwrap().diagnostics.is_empty());

        let status = engine.set_log_recording_enabled(true).unwrap();
        assert!(status.enabled);
        {
            let mut inner = engine.lock().unwrap();
            engine.push_diag_locked(&mut inner, "warning", "recorded session event");
        }
        assert!(engine
            .snapshot()
            .unwrap()
            .diagnostics
            .iter()
            .any(|entry| entry.message == "recorded session event"));

        let stopped = engine.set_log_recording_enabled(false).unwrap();
        assert!(!stopped.enabled);
        assert!(engine.snapshot().unwrap().diagnostics.is_empty());
        assert_eq!(stopped.archives.len(), 1);
        let file_name = stopped.archives[0].file_name.clone();
        let content = engine.read_log_archive(&file_name).unwrap();
        assert!(content.contains("log recording enabled"));
        assert!(content.contains("recorded session event"));
        assert!(content.contains("log recording disabled"));

        let deleted = engine.delete_log_archive(&file_name).unwrap();
        assert!(deleted.archives.is_empty());
    }

    #[test]
    fn configure_home_ignores_legacy_platform_state_file() {
        let dir = tempdir().unwrap();
        let legacy = PlatformVpnState {
            starting: true,
            running: true,
            lifecycle: ConnectionLifecycle::Connected,
            last_error: None,
            assigned_ip: "10.1.2.3".to_owned(),
            gateway: "vpn.example.com".to_owned(),
            mtu: 1400,
            network: NetworkSnapshot::default(),
            stats: SessionStats::default(),
            diagnostics: Vec::new(),
            updated_at: PlatformVpnState::now_nanos(),
        };
        std::fs::write(
            dir.path().join("platform-vpn-state.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();

        let restarted = SessionEngine::new();
        restarted.configure_home(dir.path()).unwrap();
        assert_eq!(
            restarted.snapshot().unwrap().lifecycle,
            ConnectionLifecycle::Disconnected
        );
    }

    #[test]
    fn session_handoff_remains_private_file_payload() {
        let dir = tempdir().unwrap();
        SessionHandoff {
            options: crate::model::VpnOptions {
                cookie: Some("session-cookie".to_owned()),
                ..crate::model::VpnOptions::default()
            },
            network: NetworkSnapshot::default(),
            updated_at: PlatformVpnState::now_nanos(),
        }
        .save(dir.path())
        .unwrap();

        let loaded = SessionHandoff::load(dir.path()).unwrap();
        assert_eq!(loaded.options.cookie.as_deref(), Some("session-cookie"));
    }
}
