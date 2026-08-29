use super::*;

pub(super) async fn set_log_recording_and_snapshot(
    enabled: bool,
) -> Result<LogRecordingChangeResult, String> {
    let engine = shared_engine();
    let status = engine
        .set_log_recording_enabled(enabled)
        .map_err(|error| error.to_string())?;
    let snapshot = engine.snapshot().map_err(|error| error.to_string())?;
    Ok(LogRecordingChangeResult { snapshot, status })
}

pub(super) async fn export_log_archive(file_name: String) -> Result<String, String> {
    let content = shared_engine()
        .read_log_archive(&file_name)
        .map_err(|error| error.to_string())?;
    bridge::export_log(file_name.clone(), content).await?;
    Ok(file_name)
}

pub(super) async fn delete_log_archive(
    file_name: String,
) -> Result<LogArchiveDeleteResult, String> {
    let status = shared_engine()
        .delete_log_archive(&file_name)
        .map_err(|error| error.to_string())?;
    Ok(LogArchiveDeleteResult { file_name, status })
}

/// Shared connect path for user toggle and unexpected-drop auto-reconnect.
pub(super) fn start_connect(state: &mut State) -> Command<Action> {
    let Some(active) = state.active_connection().cloned() else {
        state.push_toast(translate_ui(state.locale, tr::no_connection()));
        return Command::none();
    };
    if active.server.trim().is_empty() {
        state.push_toast(translate_ui(state.locale, tr::form_required()));
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
        state.push_toast(translate_ui(state.locale, tr::toast_enter_password()));
        return Command::none();
    }
    if !state.dry_run
        && matches!(
            active.auth_method,
            AuthMethod::Certificate | AuthMethod::PasswordAndCertificate
        )
        && active.certificate.trim().is_empty()
    {
        state.push_toast(translate_ui(state.locale, tr::toast_select_cert()));
        return Command::none();
    }
    state.user_disconnect = false;
    state.snapshot.lifecycle = ConnectionLifecycle::Connecting;
    state.snapshot.last_error = None;
    state.snapshot.stats = SessionStats::default();
    state.challenge_values.clear();
    state.challenge_seed_id = None;
    state.last_lifecycle = ConnectionLifecycle::Connecting;
    hopenconnect_core::clear_browser_open_pending();
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
    match bridge::request_start_vpn(options_json) {
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
                "{err} (connection callback missing — real connect needs platform shell)"
            ))
        }
    }
}

pub(super) async fn engine_disconnect(dry_run: bool) -> Result<SessionOutcome, String> {
    let engine = shared_engine();
    match bridge::request_stop_vpn() {
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

pub(super) async fn session_tick_delay(timeout: Duration) {
    // Platform start transactions own the ashmem notification waiter while
    // pending. Regular UI refreshes only need a bounded timer because tick()
    // synchronizes the latest frame before producing each snapshot.
    tokio::time::sleep(timeout).await;
}

pub(super) async fn group_discovery_delay() {
    // Debounce text edits; stale server values are rejected by the reducer
    // before any network request is started.
    tokio::time::sleep(Duration::from_millis(700)).await;
}

pub(super) async fn discover_groups(
    profile: VpnConnection,
) -> Result<hopenconnect_core::AuthGroupDiscovery, String> {
    shared_engine()
        .discover_auth_groups(profile)
        .await
        .map_err(|err| err.to_string())
}

pub(super) fn server_looks_ready(server: &str) -> bool {
    let server = server.trim();
    server.len() >= 3
        && !server.chars().any(char::is_whitespace)
        && !server.ends_with('.')
        && !server.ends_with('/')
}
