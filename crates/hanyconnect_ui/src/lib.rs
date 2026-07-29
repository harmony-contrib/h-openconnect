use arkit::entry;
use arkit::prelude::Element;
use hanyconnect_core::{shared_engine, ConnectRequest, E2eConfig};
use napi_derive_ohos::napi;
use napi_ohos::{bindgen_prelude::Object, Error, Result, Status};

mod l10n;
mod model;
mod platform_callbacks;
mod state;
mod view;

#[entry]
fn app() -> Element {
    view::App()
}

fn to_napi_error(err: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

#[napi]
pub fn configure_app_home(home_dir: String) -> Result<()> {
    std::env::set_var("HANYCONNECT_HOME", &home_dir);
    shared_engine()
        .configure_home(home_dir)
        .map_err(to_napi_error)
}

/// VPN-extension process entry. UI and extension use the same paws-aligned
/// shared state directory.
#[napi]
pub fn configure_app_home_for_extension(home_dir: String) -> Result<()> {
    std::env::set_var("HANYCONNECT_HOME", &home_dir);
    shared_engine()
        .configure_home(home_dir)
        .map_err(to_napi_error)
}

#[napi]
pub fn configure_ui_locale(locale: String) -> Result<()> {
    std::env::set_var("HANYCONNECT_UI_LOCALE", locale);
    Ok(())
}

#[napi]
pub fn configure_system_color_mode(color_mode: i32) -> Result<()> {
    std::env::set_var("HANYCONNECT_SYSTEM_COLOR_MODE", color_mode.to_string());
    Ok(())
}

#[napi]
pub fn register_platform_callbacks(callbacks: Object<'static>) -> Result<()> {
    platform_callbacks::register_platform_callbacks(callbacks)
}

/// Complete a document-picker request started by `pickCertFile` (ArkTS → native).
///
/// `path` may be empty / omitted when the user cancelled the picker.
#[napi]
pub fn complete_file_pick(request_id: u32, path: Option<String>) -> Result<()> {
    platform_callbacks::complete_file_pick(u64::from(request_id), path);
    Ok(())
}

/// Register ics-style per-fd protect: OpenConnect → `vpnConnection.protect(fd)`.
///
/// Pass `{ protectSocket: async (fd: number) => Promise<void> }`. Native waits
/// for completion before OpenConnect calls connect(2).
#[napi]
pub fn register_socket_protect(callbacks: Object<'static>) -> Result<()> {
    platform_callbacks::register_platform_callbacks(callbacks)
}

#[napi]
pub fn clear_socket_protect() -> Result<()> {
    hanyconnect_core::set_socket_protect_handler(None);
    Ok(())
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
    // `native-anyconnect`. Explicit HANYCONNECT_DRY_RUN=1 keeps UI/E2E mock path.
    match std::env::var("HANYCONNECT_DRY_RUN") {
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

#[napi]
pub fn apply_e2e_config(config_json: String) -> Result<String> {
    let config: E2eConfig = serde_json::from_str(&config_json).map_err(to_napi_error)?;
    if config.dry_run {
        std::env::set_var("HANYCONNECT_DRY_RUN", "1");
    } else {
        std::env::set_var("HANYCONNECT_DRY_RUN", "0");
    }
    shared_engine()
        .apply_e2e_config(config)
        .map_err(to_napi_error)
}

#[napi]
pub async fn e2e_connect_active() -> Result<String> {
    let engine = shared_engine();
    let profile = engine
        .active_profile()
        .map_err(to_napi_error)?
        .ok_or_else(|| to_napi_error("no active profile for e2e connect"))?;
    let dry_run = dry_run_from_env();
    let options = engine
        .prepare_connect(ConnectRequest { profile, dry_run })
        .await
        .map_err(to_napi_error)?;
    let options_json = serde_json::to_string(&options).map_err(to_napi_error)?;
    if dry_run {
        engine
            .set_platform_vpn_running(true)
            .map_err(to_napi_error)?;
        return engine.snapshot_json().map_err(to_napi_error);
    }
    // Ask platform to start VPN extension when callbacks exist.
    match platform_callbacks::request_start_vpn(options_json) {
        Ok(()) => {}
        Err(err) => return Err(to_napi_error(err)),
    }
    engine.snapshot_json().map_err(to_napi_error)
}

#[napi]
pub async fn e2e_disconnect() -> Result<String> {
    let _ = platform_callbacks::request_stop_vpn();
    let engine = shared_engine();
    engine.disconnect().await.map_err(to_napi_error)?;
    let _ = engine.set_platform_vpn_running(false);
    engine.snapshot_json().map_err(to_napi_error)
}

/// Submit answers for the current OpenConnect auth challenge (multi-round MFA).
#[napi]
pub fn submit_auth_challenge(reply_json: String) -> Result<()> {
    let reply: hanyconnect_core::AuthChallengeReply =
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
