//! Thin N-API binding shell for H-OpenConnect.
//!
//! NSH (`@nsh/harmonyos` + `ui/nsh/`) owns all presentation. This cdylib binds
//! `hopenconnect_app` for the UI host and `hopenconnect_core` for the HarmonyOS
//! VPN Extension process.

use hopenconnect_core::{shared_engine, ConnectRequest};
use napi_derive_ohos::napi;
use napi_ohos::{bindgen_prelude::Object, Error, Result, Status};
use std::os::fd::AsRawFd;
use std::sync::{Arc, LazyLock};

mod socket_protect;
mod vpn_handoff;
mod web_auth;

static UI_WAIT_CANCEL: LazyLock<tokio::sync::watch::Sender<u64>> =
    LazyLock::new(|| tokio::sync::watch::channel(0).0);

fn to_napi_error(err: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

fn app_error(err: hopenconnect_app::AppError) -> Error {
    Error::from_reason(err.to_string())
}

fn parse_positive_u64(value: &str, label: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("{label} must be an unsigned decimal integer"),
        ));
    }
    let parsed = value.parse::<u64>().map_err(|_| {
        Error::new(
            Status::InvalidArg,
            format!("{label} is outside the supported range"),
        )
    })?;
    if parsed == 0 {
        return Err(Error::new(
            Status::InvalidArg,
            format!("{label} must be greater than zero"),
        ));
    }
    Ok(parsed)
}

/// Host gateway owning one application command & observation session.
#[napi]
pub struct UiCommandSession {
    application: Arc<hopenconnect_app::Application>,
}

#[napi]
impl UiCommandSession {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            application: Arc::new(hopenconnect_app::Application::shared()),
        }
    }

    #[napi]
    pub async fn dispatch(&self, request_json: String) -> Result<String> {
        self.application
            .dispatch_json(&request_json)
            .await
            .map_err(app_error)
    }

    #[napi]
    pub async fn query_state(&self, request_json: String) -> Result<String> {
        let application = Arc::clone(&self.application);
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(application.query_state_json(&request_json))
        })
        .await
        .map_err(to_napi_error)?
        .map_err(app_error)
    }

    #[napi]
    pub fn subscribe_state(&self, request_json: String) -> Result<String> {
        self.application
            .subscribe_state_json(&request_json)
            .map_err(app_error)
    }

    #[napi]
    pub fn observe_state(&self, critical_only: bool) -> Result<String> {
        self.application
            .observe_state_json(critical_only)
            .map_err(app_error)
    }

    #[napi]
    pub fn tick_session(&self) -> Result<String> {
        let outcome = self.application.tick_session().map_err(app_error)?;
        serde_json::to_string(&outcome).map_err(to_napi_error)
    }

    #[napi]
    pub fn close(&self) {
        self.application.close();
    }
}

impl Default for UiCommandSession {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for UiCommandSession {
    fn drop(&mut self) {
        self.application.close();
    }
}

#[napi]
pub fn query_ui_preferences() -> Result<String> {
    let prefs = hopenconnect_app::preferences_snapshot(&shared_engine()).map_err(app_error)?;
    serde_json::to_string(&prefs).map_err(to_napi_error)
}

#[napi]
pub async fn wait_for_ui_revision(after_revision: String) -> Result<String> {
    let after = after_revision
        .parse::<u64>()
        .map_err(|_| Error::from_reason("invalid UI revision"))?;
    let mut cancel = UI_WAIT_CANCEL.subscribe();
    let application = hopenconnect_app::Application::shared();
    let revision = tokio::select! {
        revision = application.wait_for_revision(after) => revision.map_err(app_error)?,
        _ = cancel.changed() => shared_engine().runtime_revisions().revision,
    };
    Ok(revision.to_string())
}

#[napi]
pub fn cancel_ui_revision_wait() {
    UI_WAIT_CANCEL.send_modify(|generation| *generation = generation.wrapping_add(1));
}

/// Complete local disconnect cleanup after platform VPN stop finishes.
#[napi]
pub async fn finalize_disconnect() -> Result<()> {
    shared_engine().disconnect().await.map_err(to_napi_error)
}

/// Register the UI process's SAML / web sign-in callback (`{ openUrl: (uri: string) => boolean }`).
#[napi]
pub fn register_external_browser_handler(callbacks: Object<'static>) -> Result<()> {
    web_auth::register_external_browser_handler(callbacks)
}

#[napi]
pub fn clear_external_browser_handler() {
    web_auth::clear_external_browser_handler();
}

/// Consume a pending cross-process SAML browser open request from the VPN Extension, if any.
#[napi]
pub fn take_pending_browser_open() -> Result<String> {
    match hopenconnect_core::take_browser_open_pending() {
        Some(request) => serde_json::to_string(&request).map_err(to_napi_error),
        None => Ok(String::new()),
    }
}

#[napi]
pub fn configure_app_home(home_dir: String) -> Result<()> {
    std::env::set_var("HOPENCONNECT_HOME", &home_dir);
    // anyconnect-sys owns the native OpenConnect progress sink and uses this
    // compatibility variable for its durable diagnostic log.
    std::env::set_var("HANYCONNECT_HOME", &home_dir);
    shared_engine()
        .configure_home(home_dir)
        .map_err(to_napi_error)
}

/// VPN-extension process entry. Persistent profiles stay in the app-private
/// directory; authenticated handoff and live lifecycle state use ashmem.
#[napi]
pub fn configure_app_home_for_extension(home_dir: String) -> Result<()> {
    std::env::set_var("HOPENCONNECT_HOME", &home_dir);
    std::env::set_var("HANYCONNECT_HOME", &home_dir);
    shared_engine()
        .configure_home(home_dir)
        .map_err(to_napi_error)
}

#[napi]
pub fn configure_platform_identity(
    os_full_name: String,
    display_version: String,
    sdk_api_version: String,
    device_type: String,
    app_version: String,
    unique_id: String,
) {
    hopenconnect_core::configure_platform_identity(
        os_full_name,
        display_version,
        sdk_api_version,
        device_type,
        app_version,
        unique_id,
    );
}

#[napi]
pub fn initialize_platform_shared_memory() -> Result<String> {
    let fds = shared_engine()
        .initialize_platform_shared_memory()
        .map_err(to_napi_error)?;
    Ok(format!("{},{}", fds.ashmem_fd, fds.notification_fd))
}

#[napi]
pub fn attach_platform_shared_memory(ashmem_fd: i32, notification_fd: i32) -> Result<()> {
    shared_engine()
        .attach_platform_shared_memory(ashmem_fd, notification_fd)
        .map_err(to_napi_error)
}

#[napi]
pub fn prepare_platform_vpn_handoff(attempt_id: String) -> Result<String> {
    vpn_handoff::prepare(&attempt_id).map_err(to_napi_error)
}

#[napi]
pub async fn send_platform_vpn_handoff(
    ashmem_fd: i32,
    notification_fd: i32,
    attempt_id: String,
    options_json: String,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        vpn_handoff::send(ashmem_fd, notification_fd, attempt_id, options_json)
    })
    .await
    .map_err(to_napi_error)?
    .map_err(to_napi_error)
}

/// Receive the one-time kernel FD transfer in the VPN process, verify it
/// against the durable owner journal and ashmem transaction, then install the
/// shared IPC binding before returning the non-secret request metadata.
#[napi]
pub async fn receive_and_attach_platform_vpn_handoff(token: String) -> Result<String> {
    let received = tokio::task::spawn_blocking(move || vpn_handoff::receive(&token))
        .await
        .map_err(to_napi_error)?
        .map_err(to_napi_error)?;
    let ashmem_fd = received.ashmem.as_raw_fd();
    let notification_fd = received.notification.as_raw_fd();
    shared_engine()
        .validate_platform_vpn_start_request(
            ashmem_fd,
            notification_fd,
            &received.payload.attempt_id,
        )
        .map_err(to_napi_error)?;
    shared_engine()
        .attach_platform_shared_memory(ashmem_fd, notification_fd)
        .map_err(to_napi_error)?;
    serde_json::to_string(&received.payload).map_err(to_napi_error)
}

/// Check that a Want still names the current UI transaction without changing
/// the Extension's existing IPC binding or native-session owner.
#[napi]
pub fn validate_platform_vpn_start_request(
    ashmem_fd: i32,
    notification_fd: i32,
    attempt_id: String,
) -> Result<()> {
    shared_engine()
        .validate_platform_vpn_start_request(ashmem_fd, notification_fd, &attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub fn acknowledge_terminal_platform_vpn_start_delivery(
    ashmem_fd: i32,
    notification_fd: i32,
    attempt_id: String,
) -> Result<bool> {
    shared_engine()
        .acknowledge_terminal_platform_vpn_start_delivery(ashmem_fd, notification_fd, &attempt_id)
        .map_err(to_napi_error)
}

/// Block until the peer process publishes a platform frame (or the wait is
/// cancelled). Fully event driven: parks on the notification socket, never
/// polls a timeout.
#[napi]
pub async fn wait_for_platform_change_event() -> Result<bool> {
    shared_engine()
        .wait_for_platform_change_event()
        .await
        .map_err(to_napi_error)
}

/// Wake the in-process waiter parked in `wait_for_platform_change_event`.
#[napi]
pub fn cancel_platform_change_wait() {
    shared_engine().cancel_platform_change_wait();
}

#[napi]
pub fn sync_platform_changes() -> Result<()> {
    shared_engine()
        .sync_platform_changes()
        .map_err(to_napi_error)
}

#[napi]
pub fn advance_platform_vpn_intent() -> Result<String> {
    shared_engine()
        .advance_platform_vpn_intent()
        .map(|epoch| epoch.to_string())
        .map_err(to_napi_error)
}

#[napi]
pub fn is_platform_vpn_intent_current(intent_epoch: String) -> Result<bool> {
    shared_engine()
        .is_platform_vpn_intent_current(parse_positive_u64(&intent_epoch, "VPN intent epoch")?)
        .map_err(to_napi_error)
}

#[napi]
pub fn is_platform_vpn_stop_current(intent_epoch: String, attempt_id: String) -> Result<bool> {
    shared_engine()
        .is_platform_vpn_stop_current(
            parse_positive_u64(&intent_epoch, "VPN intent epoch")?,
            &attempt_id,
        )
        .map_err(to_napi_error)
}

#[napi]
pub fn begin_platform_vpn_os_stop(intent_epoch: String, attempt_id: String) -> Result<bool> {
    shared_engine()
        .begin_platform_vpn_os_stop(
            parse_positive_u64(&intent_epoch, "VPN intent epoch")?,
            &attempt_id,
        )
        .map_err(to_napi_error)
}

#[napi]
pub fn complete_platform_vpn_os_stop(intent_epoch: String, attempt_id: String) -> Result<bool> {
    shared_engine()
        .complete_platform_vpn_os_stop(
            parse_positive_u64(&intent_epoch, "VPN intent epoch")?,
            &attempt_id,
        )
        .map_err(to_napi_error)
}

#[napi]
pub fn fail_platform_vpn_os_stop(intent_epoch: String, attempt_id: String) -> Result<bool> {
    shared_engine()
        .fail_platform_vpn_os_stop(
            parse_positive_u64(&intent_epoch, "VPN intent epoch")?,
            &attempt_id,
        )
        .map_err(to_napi_error)
}

#[napi]
pub fn begin_platform_vpn_start_for_intent(intent_epoch: String) -> Result<String> {
    shared_engine()
        .begin_platform_vpn_start_for_intent(parse_positive_u64(&intent_epoch, "VPN intent epoch")?)
        .map_err(to_napi_error)
}

#[napi]
pub fn claim_current_platform_vpn_stop(intent_epoch: String) -> Result<String> {
    shared_engine()
        .claim_current_platform_vpn_stop(parse_positive_u64(&intent_epoch, "VPN intent epoch")?)
        .map_err(to_napi_error)
}

#[napi]
pub fn request_platform_vpn_stop(attempt_id: String) -> Result<bool> {
    shared_engine()
        .request_platform_vpn_stop(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub fn bind_platform_vpn_start(attempt_id: String) -> Result<String> {
    shared_engine()
        .bind_platform_vpn_start(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub fn complete_platform_vpn_cleanup(attempt_id: String) -> Result<bool> {
    shared_engine()
        .complete_platform_vpn_cleanup(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub async fn await_platform_vpn_stop(attempt_id: String) -> Result<bool> {
    shared_engine()
        .await_platform_vpn_stop(&attempt_id)
        .await
        .map_err(to_napi_error)
}

#[napi]
pub async fn recover_platform_vpn_cleanup_after_confirmed_stop(attempt_id: String) -> Result<bool> {
    shared_engine()
        .recover_platform_vpn_cleanup_after_confirmed_stop(&attempt_id)
        .await
        .map_err(to_napi_error)
}

#[napi]
pub async fn await_platform_vpn_start(attempt_id: String) -> Result<String> {
    let outcome = shared_engine()
        .await_platform_vpn_start(&attempt_id)
        .await
        .map_err(to_napi_error)?;
    Ok(match outcome {
        hopenconnect_core::PlatformStartOutcome::Connected => "connected",
        hopenconnect_core::PlatformStartOutcome::Failed => "failed",
        hopenconnect_core::PlatformStartOutcome::Cancelled => "cancelled",
        hopenconnect_core::PlatformStartOutcome::Idle => "idle",
        hopenconnect_core::PlatformStartOutcome::Pending => "pending",
    }
    .to_owned())
}

#[napi]
pub fn fail_platform_vpn_start(attempt_id: String, error: String) -> Result<bool> {
    shared_engine()
        .fail_platform_vpn_start(&attempt_id, error)
        .map_err(to_napi_error)
}

#[napi]
pub fn fail_unattached_platform_vpn_start(attempt_id: String, error: String) -> Result<bool> {
    shared_engine()
        .fail_unattached_platform_vpn_start(&attempt_id, error)
        .map_err(to_napi_error)
}

#[napi]
pub fn cancel_platform_vpn_start(attempt_id: String) -> Result<bool> {
    shared_engine()
        .cancel_platform_vpn_start(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub fn configure_ui_locale(locale: String) -> Result<()> {
    std::env::set_var("HOPENCONNECT_UI_LOCALE", &locale);
    if hopenconnect_app::set_system_locale(locale).map_err(app_error)? {
        cancel_ui_revision_wait();
    }
    Ok(())
}

#[napi]
pub fn configure_system_color_mode(color_mode: i32) -> Result<()> {
    std::env::set_var("HOPENCONNECT_SYSTEM_COLOR_MODE", color_mode.to_string());
    if hopenconnect_app::set_system_color_mode(color_mode).map_err(app_error)? {
        cancel_ui_revision_wait();
    }
    Ok(())
}

/// Register ics-style per-fd protect: OpenConnect → `vpnConnection.protect(fd)`.
#[napi]
pub fn register_socket_protect(callbacks: Object<'static>) -> Result<()> {
    socket_protect::register_socket_protect(callbacks)
}

#[napi]
pub fn clear_socket_protect() -> Result<()> {
    socket_protect::clear_socket_protect();
    Ok(())
}

#[napi]
pub fn secure_private_file(path: String) -> Result<()> {
    hopenconnect_core::secure_private_file(path).map_err(to_napi_error)
}

#[napi]
pub fn query_session() -> Result<String> {
    shared_engine().snapshot_json().map_err(to_napi_error)
}

#[napi]
pub fn default_vpn_options() -> Result<String> {
    shared_engine()
        .default_vpn_options_json()
        .map_err(to_napi_error)
}

#[napi]
pub fn set_platform_vpn_starting(attempt_id: String, starting: bool) -> Result<bool> {
    shared_engine()
        .set_platform_vpn_starting_for_attempt(&attempt_id, starting)
        .map_err(to_napi_error)
}

#[napi]
pub fn set_platform_vpn_failed(attempt_id: String, error: String) -> Result<bool> {
    shared_engine()
        .set_platform_vpn_failed_for_attempt(&attempt_id, error)
        .map_err(to_napi_error)
}

fn dry_run_from_env() -> bool {
    match std::env::var("HOPENCONNECT_DRY_RUN") {
        Ok(value) => value == "1" || value.eq_ignore_ascii_case("true"),
        Err(_) => {
            #[cfg(feature = "native-anyconnect")]
            {
                false
            }
            #[cfg(not(feature = "native-anyconnect"))]
            {
                true
            }
        }
    }
}

#[napi]
pub async fn prepare_vpn() -> Result<String> {
    let engine = shared_engine();
    let profile = engine
        .active_profile()
        .map_err(to_napi_error)?
        .ok_or_else(|| to_napi_error("no active profile"))?;
    let dry_run = dry_run_from_env();
    let options = engine
        .prepare_connect(ConnectRequest { profile, dry_run })
        .await
        .map_err(to_napi_error)?;
    serde_json::to_string(&options).map_err(to_napi_error)
}

/// Called from the VPN-extension process only: re-auth with handoff credentials
/// and return fresh VpnOptions (addresses/DNS) before TUN create.
#[napi]
pub async fn prepare_vpn_in_extension(options_json: String) -> Result<String> {
    shared_engine()
        .prepare_in_extension(&options_json)
        .await
        .map_err(to_napi_error)
}

#[napi]
pub async fn start_vpn(fd: i32, options_json: String) -> Result<()> {
    shared_engine()
        .attach_tun(fd, &options_json)
        .await
        .map_err(to_napi_error)
}

/// VPN-extension heartbeat: reconcile cross-process terminal state, refresh
/// native statistics, and publish a fresh Extension lane frame.
#[napi]
pub fn extension_tick(attempt_id: String) -> Result<String> {
    shared_engine()
        .extension_tick(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub async fn stop_vpn(attempt_id: String) -> Result<bool> {
    shared_engine()
        .disconnect_platform_attempt(&attempt_id)
        .await
        .map_err(to_napi_error)
}

/// Submit answers for the current OpenConnect auth challenge (multi-round MFA).
#[napi]
pub fn submit_auth_challenge(reply_json: String) -> Result<()> {
    let reply: hopenconnect_core::AuthChallengeReply =
        serde_json::from_str(&reply_json).map_err(to_napi_error)?;
    shared_engine()
        .submit_auth_challenge(reply)
        .map_err(to_napi_error)
}

#[napi]
pub fn cancel_auth_challenge() -> Result<()> {
    shared_engine()
        .cancel_auth_challenge()
        .map_err(to_napi_error)
}

#[napi]
pub fn pending_auth_challenge() -> Result<String> {
    let pending = shared_engine().pending_auth();
    serde_json::to_string(&pending).map_err(to_napi_error)
}
