use arkit::entry;
use arkit::openharmony_ability::OpenHarmonyApp;
use arkit::prelude::Element;
use hopenconnect_core::{shared_engine, ConnectRequest};
use napi_derive_ohos::napi;
use napi_ohos::{bindgen_prelude::Object, Error, Result, Status};

mod bridge;
mod i18n;
mod locale;
mod log_filter;
mod model;
mod socket_protect;
mod state;
mod time_format;
mod view;

#[entry(plugins = [
    bridge::HOpenUrlBridgePlugin,
    bridge::HOpenVpnBridgePlugin,
    bridge::HOpenColorModeBridgePlugin,
    bridge::HOpenExportBridgePlugin,
    bridge::HOpenCertFileBridgePlugin,
    bridge::HOpenSafeAreaBridgePlugin,
])]
fn app(handle: OpenHarmonyApp) -> Element {
    let initial_safe_area = bridge::initial_safe_area(&handle);
    bridge::set_app(handle);
    view::App(initial_safe_area)
}

fn to_napi_error(err: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

#[napi]
pub fn configure_app_home(home_dir: String) -> Result<()> {
    std::env::set_var("HOPENCONNECT_HOME", &home_dir);
    shared_engine()
        .configure_home(home_dir)
        .map_err(to_napi_error)
}

/// VPN-extension process entry. Persistent profiles stay in the app-private
/// directory; authenticated handoff and live lifecycle state use ashmem.
#[napi]
pub fn configure_app_home_for_extension(home_dir: String) -> Result<()> {
    std::env::set_var("HOPENCONNECT_HOME", &home_dir);
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
pub fn begin_platform_vpn_start() -> Result<String> {
    shared_engine()
        .begin_platform_vpn_start()
        .map_err(to_napi_error)
}

#[napi]
pub fn bind_platform_vpn_start(attempt_id: String) -> Result<()> {
    shared_engine()
        .bind_platform_vpn_start(&attempt_id)
        .map_err(to_napi_error)
}

#[napi]
pub async fn await_platform_vpn_start_attachment(
    attempt_id: String,
    timeout_ms: u32,
) -> Result<bool> {
    shared_engine()
        .await_platform_vpn_start_attachment(
            &attempt_id,
            std::time::Duration::from_millis(u64::from(timeout_ms)),
        )
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
pub fn set_platform_vpn_running(running: bool) -> Result<()> {
    shared_engine()
        .set_platform_vpn_running(running)
        .map_err(to_napi_error)
}

#[napi]
pub fn set_platform_vpn_starting(starting: bool) -> Result<()> {
    shared_engine()
        .set_platform_vpn_starting(starting)
        .map_err(to_napi_error)
}

#[napi]
pub fn set_platform_vpn_failed(error: String) -> Result<()> {
    shared_engine()
        .set_platform_vpn_failed(error)
        .map_err(to_napi_error)
}

#[napi]
pub fn expire_platform_vpn_start() -> Result<bool> {
    shared_engine()
        .expire_platform_vpn_start()
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

#[napi]
pub async fn stop_vpn() -> Result<()> {
    shared_engine().disconnect().await.map_err(to_napi_error)?;
    shared_engine()
        .set_platform_vpn_running(false)
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
