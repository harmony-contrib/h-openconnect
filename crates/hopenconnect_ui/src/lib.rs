use arkit::entry;
use arkit::openharmony_ability::OpenHarmonyApp;
use arkit::prelude::Element;
use hopenconnect_core::{shared_engine, ConnectRequest};
use napi_derive_ohos::napi;
use napi_ohos::{bindgen_prelude::Object, Error, Result, Status};
use std::os::fd::AsRawFd;

mod bridge;
mod connection_qr;
mod i18n;
mod locale;
mod log_filter;
mod model;
mod socket_protect;
mod state;
mod time_format;
mod view;
mod virtual_identity;
mod vpn_handoff;

#[entry(plugins = [
    bridge::HOpenUrlBridgePlugin,
    bridge::HOpenVpnBridgePlugin,
    bridge::HOpenColorModeBridgePlugin,
    bridge::HOpenExportBridgePlugin,
    bridge::HOpenScanBridgePlugin,
    bridge::HOpenCertFileBridgePlugin,
    bridge::HOpenSafeAreaBridgePlugin,
])]
fn app(handle: OpenHarmonyApp) -> Element {
    let initial_safe_area = bridge::initial_safe_area(&handle);
    bridge::set_app(handle);
    // Interactive authentication runs in the UI process before a platform VPN
    // attempt or ashmem channel exists. Register the direct system-browser
    // path here; the extension-to-UI ashmem path remains the fallback for a
    // later protocol reauthentication initiated by the extension process.
    hopenconnect_core::set_external_browser_handler(Some(Box::new(|uri| {
        bridge::open_external_browser_blocking(uri.to_owned()).is_ok()
    })));
    view::App(initial_safe_area)
}

fn to_napi_error(err: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
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
    std::env::set_var("HOPENCONNECT_UI_LOCALE", locale);
    Ok(())
}

#[napi]
pub fn configure_system_color_mode(color_mode: i32) -> Result<()> {
    std::env::set_var("HOPENCONNECT_SYSTEM_COLOR_MODE", color_mode.to_string());
    Ok(())
}

/// Register ics-style per-fd protect: OpenConnect → `vpnConnection.protect(fd)`.
///
/// Pass `{ protectSocket: async (fd: number) => Promise<void> }`. Native waits
/// for completion before OpenConnect calls connect(2).
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
    // Real OpenConnect is the default when the binary is built with
    // `native-anyconnect`. Explicit HOPENCONNECT_DRY_RUN=1 keeps the development
    // mock path available without adding commands to production abilities.
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
