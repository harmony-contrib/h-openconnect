use crate::auth_bridge::AuthInteraction;
use crate::error::{CoreError, CoreResult};
use crate::log_recording::{self, RecordedLogBuffer, RuntimeLogBuffer, MAX_IN_MEMORY_LOGS};
use crate::model::{
    AuthChallenge, AuthChallengeReply, AuthGroupDiscovery, ConnectRequest, ConnectionLifecycle,
    ConnectionProfile, DiagnosticEntry, NetworkSnapshot, SessionSnapshot, SessionStats, VpnOptions,
};
use crate::platform_ipc::{PlatformIpc, PlatformIpcError};
use crate::platform_state::{
    BrowserOpenRequest, PlatformStartOutcome, PlatformVpnState, SessionHandoff,
};
use crate::store::{Preferences, ProfileStore};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tracing::Level;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;

mod connection;
mod logging;
mod platform;
mod profiles;

use logging::{install_runtime_log_layer, merge_platform_logs, merged_logs};

#[cfg(feature = "native-anyconnect")]
use crate::native_session::{
    authenticate as native_authenticate, resume_from_options, spawn_mainloop, PendingNativeSession,
    RunningNativeSession,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLATFORM_VPN_START_DEADLINE: Duration = Duration::from_secs(120);

#[cfg(feature = "native-anyconnect")]
const BACKEND: &str = "anyconnect-rs";
const OBSOLETE_CROSS_PROCESS_FILES: &[&str] = &[
    "session-handoff.json",
    "browser-request.json",
    "platform-vpn-state.json",
];
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
    platform_start_sequence: u64,
    platform_start_attempt_id: String,
    platform_start_outcome: PlatformStartOutcome,
    platform_extension_attached: bool,
    platform_session_handoff: Option<SessionHandoff>,
    platform_browser_request: Option<BrowserOpenRequest>,
    platform_browser_request_sequence: u64,
    last_platform_browser_request_id: String,
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
    platform_start_tx: watch::Sender<PlatformStartEvent>,
    auth: Arc<AuthInteraction>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformSharedMemoryFds {
    pub ashmem_fd: i32,
    pub notification_fd: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PlatformStartEvent {
    attempt_id: String,
    outcome: PlatformStartOutcome,
    extension_attached: bool,
    error: Option<String>,
}

impl SessionEngine {
    pub fn new() -> Self {
        install_runtime_log_layer();
        let (lifecycle_tx, _) = watch::channel(ConnectionLifecycle::Disconnected);
        let (platform_start_tx, _) = watch::channel(PlatformStartEvent::default());
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
                platform_start_sequence: 0,
                platform_start_attempt_id: String::new(),
                platform_start_outcome: PlatformStartOutcome::Idle,
                platform_extension_attached: false,
                platform_session_handoff: None,
                platform_browser_request: None,
                platform_browser_request_sequence: 0,
                last_platform_browser_request_id: String::new(),
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
            platform_start_tx,
            auth,
        }
    }
}

impl SessionEngine {
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

fn sanitized_want_options(options: &VpnOptions) -> VpnOptions {
    VpnOptions {
        addresses: options.addresses.clone(),
        routes: options.routes.clone(),
        excluded_routes: options.excluded_routes.clone(),
        dns_addresses: options.dns_addresses.clone(),
        search_domains: options.search_domains.clone(),
        mtu: options.mtu,
        allow_bypass: options.allow_bypass,
        force_global: options.force_global,
        trusted_applications: options.trusted_applications.clone(),
        blocked_applications: options.blocked_applications.clone(),
        ..VpnOptions::default()
    }
}

fn remove_obsolete_cross_process_files(home: &std::path::Path) -> CoreResult<()> {
    for file_name in OBSOLETE_CROSS_PROCESS_FILES {
        let path = home.join(file_name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CoreError::msg(format!(
                    "remove obsolete cross-process file {}: {error}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn consume_platform_browser_request_locked(
    inner: &mut Inner,
    request: Option<BrowserOpenRequest>,
) -> Option<BrowserOpenRequest> {
    let request = request.filter(|request| {
        request.is_valid_for(&inner.platform_start_attempt_id)
            && request.request_id != inner.last_platform_browser_request_id
    })?;
    inner.last_platform_browser_request_id = request.request_id.clone();
    Some(request)
}

fn acknowledge_platform_browser_request_locked(inner: &mut Inner, ack: Option<&str>) -> bool {
    let acknowledged = ack.is_some_and(|ack| {
        inner
            .platform_browser_request
            .as_ref()
            .is_some_and(|request| request.request_id == ack)
    });
    if acknowledged {
        inner.platform_browser_request = None;
    }
    acknowledged
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
mod tests;
