use napi_ohos::{
    bindgen_prelude::{
        spawn as spawn_napi_future, CallbackContext, Function, JsObjectValue, Object, Promise,
        PromiseRaw, Unknown,
    },
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
    Error, Result, Status,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::oneshot;

type SetColorModeCall<'a> = Function<'a, i32, Unknown<'a>>;
type SetColorModeThreadsafeFunction = ThreadsafeFunction<i32, Unknown<'static>, i32, Status, false>;
type SetColorModeSlot = LazyLock<RwLock<Option<Arc<SetColorModeThreadsafeFunction>>>>;

type VpnStartCall<'a> = Function<'a, String, Unknown<'a>>;
type VpnStartThreadsafeFunction =
    ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type VpnStartSlot = LazyLock<RwLock<Option<Arc<VpnStartThreadsafeFunction>>>>;

type VpnStopCall<'a> = Function<'a, (), Unknown<'a>>;
type VpnStopThreadsafeFunction = ThreadsafeFunction<(), Unknown<'static>, (), Status, false>;
type VpnStopSlot = LazyLock<RwLock<Option<Arc<VpnStopThreadsafeFunction>>>>;

/// Keep OpenConnect CSTP/DTLS sockets off the OpenHarmony tunnel.
type SocketProtectCall<'a> = Function<'a, i32, Promise<()>>;
type SocketProtectThreadsafeFunction = ThreadsafeFunction<i32, Promise<()>, i32, Status, false>;
type SocketProtectSlot = LazyLock<RwLock<Option<Arc<SocketProtectThreadsafeFunction>>>>;

/// Open system browser for SAML / SSO (uri string).
type OpenBrowserCall<'a> = Function<'a, String, Unknown<'a>>;
type OpenBrowserThreadsafeFunction =
    ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type OpenBrowserSlot = LazyLock<RwLock<Option<Arc<OpenBrowserThreadsafeFunction>>>>;

/// Generic external URL opened from the application UI.
type OpenExternalUrlCall<'a> = Function<'a, String, Unknown<'a>>;
type OpenExternalUrlThreadsafeFunction =
    ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type OpenExternalUrlSlot = LazyLock<RwLock<Option<Arc<OpenExternalUrlThreadsafeFunction>>>>;

/// Document picker request: JSON `{ id, kind }`.
type PickFileCall<'a> = Function<'a, String, Unknown<'a>>;
type PickFileThreadsafeFunction =
    ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type PickFileSlot = LazyLock<RwLock<Option<Arc<PickFileThreadsafeFunction>>>>;

type ExportLogCall<'a> = Function<'a, String, Unknown<'a>>;
type ExportLogThreadsafeFunction =
    ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type ExportLogSlot = LazyLock<RwLock<Option<Arc<ExportLogThreadsafeFunction>>>>;

static SET_COLOR_MODE: SetColorModeSlot = LazyLock::new(|| RwLock::new(None));
static REQUEST_START_VPN: VpnStartSlot = LazyLock::new(|| RwLock::new(None));
static REQUEST_STOP_VPN: VpnStopSlot = LazyLock::new(|| RwLock::new(None));
static SOCKET_PROTECT: SocketProtectSlot = LazyLock::new(|| RwLock::new(None));
static OPEN_BROWSER: OpenBrowserSlot = LazyLock::new(|| RwLock::new(None));
static OPEN_EXTERNAL_URL: OpenExternalUrlSlot = LazyLock::new(|| RwLock::new(None));
static PICK_FILE: PickFileSlot = LazyLock::new(|| RwLock::new(None));
static EXPORT_LOG: ExportLogSlot = LazyLock::new(|| RwLock::new(None));
static PICK_FILE_SEQ: AtomicU64 = AtomicU64::new(1);
static PICK_FILE_WAITERS: LazyLock<Mutex<HashMap<u64, oneshot::Sender<Option<String>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn register_platform_callbacks(callbacks: Object<'static>) -> Result<()> {
    if callbacks.has_named_property("setColorMode")? {
        let set_color_mode: SetColorModeCall<'static> =
            callbacks.get_named_property("setColorMode")?;
        let tsfn = set_color_mode
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        SET_COLOR_MODE
            .write()
            .map_err(|_| Error::from_reason("failed to store color mode callback"))?
            .replace(Arc::new(tsfn));
    }
    if callbacks.has_named_property("requestStartVpn")? {
        let request_start_vpn: VpnStartCall<'static> =
            callbacks.get_named_property("requestStartVpn")?;
        let tsfn = request_start_vpn
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        REQUEST_START_VPN
            .write()
            .map_err(|_| Error::from_reason("failed to store VPN start callback"))?
            .replace(Arc::new(tsfn));
    }
    if callbacks.has_named_property("requestStopVpn")? {
        let request_stop_vpn: VpnStopCall<'static> =
            callbacks.get_named_property("requestStopVpn")?;
        let tsfn = request_stop_vpn
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        REQUEST_STOP_VPN
            .write()
            .map_err(|_| Error::from_reason("failed to store VPN stop callback"))?
            .replace(Arc::new(tsfn));
    }
    if callbacks.has_named_property("protectSocket")? {
        let protect_socket: SocketProtectCall<'static> =
            callbacks.get_named_property("protectSocket")?;
        let tsfn = protect_socket
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        SOCKET_PROTECT
            .write()
            .map_err(|_| Error::from_reason("failed to store socket protect callback"))?
            .replace(Arc::new(tsfn));
        // OpenConnect protect_socket_handler → ArkTS vpnConnection.protect(fd)
        hopenconnect_core::set_socket_protect_handler(Some(Box::new(|fd| {
            let _ = protect_socket_fd(fd);
        })));
    }
    if callbacks.has_named_property("openExternalBrowser")? {
        let open_browser: OpenBrowserCall<'static> =
            callbacks.get_named_property("openExternalBrowser")?;
        let tsfn = open_browser
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        OPEN_BROWSER
            .write()
            .map_err(|_| Error::from_reason("failed to store open browser callback"))?
            .replace(Arc::new(tsfn));
        // OpenConnect external_browser_handler → ArkTS system browser
        hopenconnect_core::set_external_browser_handler(Some(Box::new(|uri| {
            open_external_browser(uri.to_owned()).is_ok()
        })));
    }
    if callbacks.has_named_property("openExternalUrl")? {
        let open_external_url: OpenExternalUrlCall<'static> =
            callbacks.get_named_property("openExternalUrl")?;
        let tsfn = open_external_url
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        OPEN_EXTERNAL_URL
            .write()
            .map_err(|_| Error::from_reason("failed to store external URL callback"))?
            .replace(Arc::new(tsfn));
    }
    if callbacks.has_named_property("pickCertFile")? {
        let pick_file: PickFileCall<'static> = callbacks.get_named_property("pickCertFile")?;
        let tsfn = pick_file
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        PICK_FILE
            .write()
            .map_err(|_| Error::from_reason("failed to store pick file callback"))?
            .replace(Arc::new(tsfn));
    }
    if callbacks.has_named_property("exportLog")? {
        let export_log: ExportLogCall<'static> = callbacks.get_named_property("exportLog")?;
        let tsfn = export_log
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?;
        EXPORT_LOG
            .write()
            .map_err(|_| Error::from_reason("failed to store log export callback"))?
            .replace(Arc::new(tsfn));
    }
    Ok(())
}

/// Call platform `vpnConnection.protect(fd)` and wait for its Promise.
///
/// OpenHarmony exposes socket protection as a Promise. OpenConnect starts
/// `connect(2)` immediately after this callback returns, so merely scheduling
/// the Promise introduces a race with an already-active VPN. Waiting here
/// preserves the OpenConnect ordering: protection completes first.
pub(crate) fn protect_socket_fd(fd: i32) -> Result<()> {
    if fd < 0 {
        return Ok(());
    }
    let tsfn = SOCKET_PROTECT
        .read()
        .map_err(|_| Error::from_reason("failed to read socket protect callback"))?
        .as_ref()
        .map(Arc::clone);
    let Some(tsfn) = tsfn else {
        return Ok(());
    };
    let (completion_tx, completion_rx) = mpsc::sync_channel(1);
    let status = tsfn.call_with_return_value(
        fd,
        ThreadsafeFunctionCallMode::NonBlocking,
        move |promise_result, _env| {
            match promise_result {
                Ok(promise) => {
                    let _ = spawn_napi_future(async move {
                        let result = promise.await.map_err(|err| err.to_string());
                        let _ = completion_tx.send(result);
                    });
                }
                Err(err) => {
                    let _ = completion_tx.send(Err(err.to_string()));
                }
            }
            Ok(())
        },
    );
    if status != Status::Ok {
        return Err(Error::from_reason(format!(
            "call socket protect failed with status: {status:?}"
        )));
    }

    match completion_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(reason)) => Err(Error::from_reason(format!(
            "platform socket protect failed: {reason}"
        ))),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(Error::from_reason("platform socket protect timed out"))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::from_reason(
            "platform socket protect completion channel closed",
        )),
    }
}

/// Open the system browser for a SAML/SSO URI (non-blocking).
pub(crate) fn open_external_browser(uri: String) -> std::result::Result<(), String> {
    if uri.trim().is_empty() {
        return Err("empty browser uri".to_owned());
    }
    let tsfn = OPEN_BROWSER
        .read()
        .map_err(|_| "failed to read open browser callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "openExternalBrowser callback is not registered".to_owned())?;
    let status = tsfn.call(uri, ThreadsafeFunctionCallMode::NonBlocking);
    if status == Status::Ok {
        Ok(())
    } else {
        Err(format!(
            "call openExternalBrowser failed with status: {status:?}"
        ))
    }
}

pub(crate) async fn open_external_url(url: String) -> std::result::Result<(), String> {
    if url.trim().is_empty() {
        return Err("empty external URL".to_owned());
    }
    let tsfn = OPEN_EXTERNAL_URL
        .read()
        .map_err(|_| "failed to read external URL callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "external URL callback is not registered".to_owned())?;
    invoke_string_void_callback(tsfn, url, "external URL").await
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

/// Ask ArkTS to open DocumentViewPicker and copy the selected file into the app sandbox.
///
/// Completes when [`complete_file_pick`] is called from ArkTS (or times out).
pub(crate) async fn pick_cert_file(kind: CertFileKind) -> std::result::Result<String, String> {
    let tsfn = PICK_FILE
        .read()
        .map_err(|_| "failed to read pick file callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "pickCertFile callback is not registered".to_owned())?;

    let id = PICK_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    {
        let mut waiters = PICK_FILE_WAITERS
            .lock()
            .map_err(|_| "pick file waiters lock poisoned".to_owned())?;
        waiters.insert(id, tx);
    }

    let request = serde_json::json!({
        "id": id,
        "kind": kind.as_str(),
    })
    .to_string();
    let status = tsfn.call(request, ThreadsafeFunctionCallMode::NonBlocking);
    if status != Status::Ok {
        let mut waiters = PICK_FILE_WAITERS
            .lock()
            .map_err(|_| "pick file waiters lock poisoned".to_owned())?;
        waiters.remove(&id);
        return Err(format!("call pickCertFile failed with status: {status:?}"));
    }

    match tokio::time::timeout(Duration::from_secs(180), rx).await {
        Ok(Ok(Some(path))) if !path.trim().is_empty() => Ok(path),
        Ok(Ok(Some(_))) | Ok(Ok(None)) => Err("file selection cancelled".to_owned()),
        Ok(Err(_)) => Err("file selection channel closed".to_owned()),
        Err(_) => {
            if let Ok(mut waiters) = PICK_FILE_WAITERS.lock() {
                waiters.remove(&id);
            }
            Err("file selection timed out".to_owned())
        }
    }
}

/// Called from ArkTS after DocumentViewPicker finishes (path may be null/empty on cancel).
pub(crate) fn complete_file_pick(request_id: u64, path: Option<String>) {
    let waiter = PICK_FILE_WAITERS
        .lock()
        .ok()
        .and_then(|mut waiters| waiters.remove(&request_id));
    if let Some(tx) = waiter {
        let cleaned = path.and_then(|p| {
            let t = p.trim().to_owned();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
        let _ = tx.send(cleaned);
    }
}

pub(crate) async fn export_log(
    suggested_name: String,
    content: String,
) -> std::result::Result<(), String> {
    let tsfn = EXPORT_LOG
        .read()
        .map_err(|_| "failed to read log export callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "log export callback is not registered".to_owned())?;
    let request = serde_json::json!({
        "suggestedName": suggested_name,
        "content": content,
    })
    .to_string();
    invoke_string_void_callback(tsfn, request, "log export").await
}

async fn invoke_string_void_callback(
    tsfn: Arc<ExportLogThreadsafeFunction>,
    value: String,
    label: &'static str,
) -> std::result::Result<(), String> {
    let (tx, rx) = oneshot::channel::<Result<()>>();
    let status = tsfn.call_with_return_value(value, ThreadsafeFunctionCallMode::NonBlocking, {
        move |result, _| {
            match result {
                Ok(value) => {
                    let tx_cell = Rc::new(Cell::new(Some(tx)));
                    let tx_in_catch = tx_cell.clone();
                    let promise = unsafe { value.cast::<PromiseRaw<'static, ()>>() }?;
                    promise
                        .then(move |_ctx| {
                            if let Some(sender) = tx_cell.replace(None) {
                                let _ = sender.send(Ok(()));
                            }
                            Ok(())
                        })?
                        .catch(move |ctx: CallbackContext<Unknown>| {
                            if let Some(sender) = tx_in_catch.replace(None) {
                                let _ = sender.send(Err(ctx.value.into()));
                            }
                            Ok(())
                        })?;
                }
                Err(error) => {
                    let _ = tx.send(Err(error));
                }
            }
            Ok(())
        }
    });
    if status != Status::Ok {
        return Err(format!(
            "call {label} callback failed with status: {status:?}"
        ));
    }
    rx.await
        .map_err(|_| format!("{label} callback receiver dropped"))?
        .map_err(|error| error.to_string())
}

pub(crate) fn set_color_mode(color_mode: i32) -> Result<()> {
    let tsfn = SET_COLOR_MODE
        .read()
        .map_err(|_| Error::from_reason("failed to read color mode callback"))?
        .as_ref()
        .map(Arc::clone);
    let Some(tsfn) = tsfn else {
        return Ok(());
    };
    let status = tsfn.call(color_mode, ThreadsafeFunctionCallMode::NonBlocking);
    if status == Status::Ok {
        Ok(())
    } else {
        Err(Error::from_reason(format!(
            "call color mode callback failed with status: {status:?}"
        )))
    }
}

pub(crate) fn request_start_vpn(options_json: String) -> std::result::Result<(), String> {
    let tsfn = REQUEST_START_VPN
        .read()
        .map_err(|_| "failed to read VPN start callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "VPN start callback is not registered".to_owned())?;
    let status = tsfn.call(options_json, ThreadsafeFunctionCallMode::NonBlocking);
    if status == Status::Ok {
        Ok(())
    } else {
        Err(format!(
            "call VPN start callback failed with status: {status:?}"
        ))
    }
}

pub(crate) fn request_stop_vpn() -> std::result::Result<(), String> {
    let tsfn = REQUEST_STOP_VPN
        .read()
        .map_err(|_| "failed to read VPN stop callback".to_owned())?
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "VPN stop callback is not registered".to_owned())?;
    let status = tsfn.call((), ThreadsafeFunctionCallMode::NonBlocking);
    if status == Status::Ok {
        Ok(())
    } else {
        Err(format!(
            "call VPN stop callback failed with status: {status:?}"
        ))
    }
}
