//! Application-owned ArkTS bridge plugins and the Rust call surface.
//!
//! H-OpenConnect is a HarmonyOS-only app. Platform capabilities that the
//! openharmony-ability built-in plugins do not cover (system VPN extension
//! control, app color mode, log export with a pre-filled name, certificate
//! pick-and-copy) are implemented here as `hopenconnect.*` bridge plugins: the
//! ArkTS side owns the platform objects, Rust submits named N-API values and
//! awaits the outcome. The helpers below are the single call surface used by
//! the native UI.

mod cert_file;
mod color_mode;
mod export;
mod safe_area;
mod vpn;

use std::sync::{LazyLock, RwLock};

use arkit::openharmony_ability::{AsyncBridge, BridgeCallOptions, BridgePlugin, OpenHarmonyApp};
use openharmony_ability_plugin_url::UrlExt;

pub(crate) use self::cert_file::{CertFileRequest, CertFileResponse, HOpenCertFileBridgePlugin};
pub(crate) use self::color_mode::{ColorModeRequest, ColorModeResponse, HOpenColorModeBridgePlugin};
pub(crate) use self::export::{ExportTextRequest, ExportTextResponse, HOpenExportBridgePlugin};
pub(crate) use self::safe_area::{initial_safe_area, InitialSafeArea, HOpenSafeAreaBridgePlugin};
pub(crate) use self::vpn::{VpnStartRequest, VpnStartResponse, VpnStopRequest, VpnStopResponse, HOpenVpnBridgePlugin};

/// Rust-side handle of the current Ability session, installed by `init`.
static INNER_APP: LazyLock<RwLock<Option<OpenHarmonyApp>>> = LazyLock::new(|| RwLock::new(None));

pub(crate) fn set_app(app: OpenHarmonyApp) {
    *INNER_APP
        .write()
        .expect("INNER_APP write lock must not fail") = Some(app);
}

fn current_app() -> std::result::Result<OpenHarmonyApp, String> {
    INNER_APP
        .read()
        .expect("INNER_APP read lock must not fail")
        .as_ref()
        .cloned()
        .ok_or_else(|| "OpenHarmony app not initialized".to_owned())
}

async fn call_async<P, R, S>(action: &str, request: R) -> std::result::Result<S, String>
where
    P: BridgePlugin<Mode = AsyncBridge>,
    R: arkit::openharmony_ability::BridgeNapiType,
    S: arkit::openharmony_ability::BridgeNapiType,
{
    let app = current_app()?;
    let bridge = app.bridge().map_err(|err| err.to_string())?;
    bridge
        .call_async::<P, R, S>(action, request, BridgeCallOptions::default())
        .await
        .map_err(|err| err.to_string())
}

/// Fire-and-forget outbound plugin call (no observable outcome for the UI).
fn spawn_call<P, R, S>(action: &'static str, request: R)
where
    P: BridgePlugin<Mode = AsyncBridge>,
    R: arkit::openharmony_ability::BridgeNapiType + 'static,
    S: arkit::openharmony_ability::BridgeNapiType + 'static,
{
    let app = match current_app() {
        Ok(app) => app,
        Err(_) => return,
    };
    let bridge = match app.bridge() {
        Ok(bridge) => bridge,
        Err(_) => return,
    };
    let task = async move {
        let _ = bridge
            .call_async::<P, R, S>(action, request, BridgeCallOptions::default())
            .await;
    };
    arkit::napi_ohos::bindgen_prelude::spawn(task);
}

/// Certificate file role for the system document picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CertFileKind {
    Certificate,
    PrivateKey,
    CaCertificate,
}

impl CertFileKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Certificate => "certificate",
            Self::PrivateKey => "private_key",
            Self::CaCertificate => "ca_certificate",
        }
    }
}

/// Ask ArkTS to start the system VPN extension. Fire-and-forget: the ArkTS side
/// owns the full orchestration and the UI observes the outcome through the
/// lifecycle / platform event stream.
pub(crate) fn request_start_vpn(options_json: String) -> std::result::Result<(), String> {
    let app = current_app()?;
    let bridge = app.bridge().map_err(|err| err.to_string())?;
    let task = async move {
        let _ = bridge
            .call_async::<HOpenVpnBridgePlugin, VpnStartRequest, VpnStartResponse>(
                "start-vpn",
                VpnStartRequest { options_json },
                BridgeCallOptions::default(),
            )
            .await;
    };
    arkit::napi_ohos::bindgen_prelude::spawn(task);
    Ok(())
}

/// Ask ArkTS to stop the system VPN extension. Fire-and-forget.
pub(crate) fn request_stop_vpn() -> std::result::Result<(), String> {
    let app = current_app()?;
    let bridge = app.bridge().map_err(|err| err.to_string())?;
    let task = async move {
        let _ = bridge
            .call_async::<HOpenVpnBridgePlugin, VpnStopRequest, VpnStopResponse>(
                "stop-vpn",
                VpnStopRequest {},
                BridgeCallOptions::default(),
            )
            .await;
    };
    arkit::napi_ohos::bindgen_prelude::spawn(task);
    Ok(())
}

/// Fire-and-forget color mode application. The native UI owns the preference
/// state; failures are logged by the ArkTS side and never surface to the UI.
pub(crate) fn set_color_mode(color_mode: i32) -> std::result::Result<(), String> {
    spawn_call::<HOpenColorModeBridgePlugin, ColorModeRequest, ColorModeResponse>(
        "set-color-mode",
        ColorModeRequest { mode: color_mode },
    );
    Ok(())
}

/// Open a SAML/SSO or generic external URL through the system link opener.
///
/// Fire-and-forget: OpenConnect's SSO loop is a blocking native thread, so we
/// only queue the open and let the ArkTS plugin report failures.
pub(crate) fn open_external_browser(uri: String) -> std::result::Result<(), String> {
    if uri.trim().is_empty() {
        return Err("empty browser uri".to_owned());
    }
    let app = current_app()?;
    let task = async move {
        let _ = app.open_url(uri).await;
    };
    arkit::napi_ohos::bindgen_prelude::spawn(task);
    Ok(())
}

/// Open an external URL, awaiting the platform acknowledgement so the caller
/// can surface a toast on failure.
pub(crate) async fn open_external_url(url: String) -> std::result::Result<(), String> {
    if url.trim().is_empty() {
        return Err("empty external URL".to_owned());
    }
    let app = current_app()?;
    app.open_url(url).await.map_err(|err| err.to_string())
}

/// Export the log archive through the system document picker with a pre-filled
/// suggested name.
pub(crate) async fn export_log(
    suggested_name: String,
    content: String,
) -> std::result::Result<(), String> {
    call_async::<HOpenExportBridgePlugin, ExportTextRequest, ExportTextResponse>(
        "export-text",
        ExportTextRequest {
            suggested_name,
            content,
        },
    )
    .await?;
    Ok(())
}

/// Ask ArkTS to open the system document picker, copy the selected certificate
/// into the app sandbox and return the real filesystem path.
pub(crate) async fn pick_cert_file(kind: CertFileKind) -> std::result::Result<String, String> {
    let response = call_async::<HOpenCertFileBridgePlugin, CertFileRequest, CertFileResponse>(
        "pick-cert",
        CertFileRequest {
            kind: kind.as_str().to_owned(),
        },
    )
    .await?;
    let path = response.path.trim().to_owned();
    if path.is_empty() {
        Err("file selection cancelled".to_owned())
    } else {
        Ok(path)
    }
}
