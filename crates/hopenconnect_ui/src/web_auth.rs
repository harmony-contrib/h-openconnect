//! In-process SAML / SSO web sign-in callback bridge for the UI process.
//!
//! OpenConnect invokes `platform_browser::open(uri)` on a blocking worker
//! thread during interactive SSO-v2 authentication. This module registers an
//! ArkTS callback via N-API `ThreadsafeFunction` so the UI host can open
//! upstream `@nsh/harmonyos` `NativeAuthSession` immediately and synchronously
//! report whether the session was accepted.

use napi_ohos::{
    bindgen_prelude::{Function, JsObjectValue, Object},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
    Error, Result, Status,
};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

type OpenBrowserCall<'a> = Function<'a, String, bool>;
type OpenBrowserThreadsafeFunction = ThreadsafeFunction<String, bool, String, Status, false>;
type OpenBrowserSlot = LazyLock<RwLock<Option<Arc<OpenBrowserThreadsafeFunction>>>>;

static EXTERNAL_BROWSER: OpenBrowserSlot = LazyLock::new(|| RwLock::new(None));

pub(crate) fn register_external_browser_handler(callbacks: Object<'static>) -> Result<()> {
    if !callbacks.has_named_property("openUrl")? {
        return Err(Error::from_reason(
            "external browser registration requires openUrl",
        ));
    }
    let open_url: OpenBrowserCall<'static> = callbacks.get_named_property("openUrl")?;
    let tsfn = Arc::new(
        open_url
            .build_threadsafe_function()
            .callee_handled::<false>()
            .build()?,
    );
    EXTERNAL_BROWSER
        .write()
        .map_err(|_| Error::from_reason("failed to store external browser callback"))?
        .replace(Arc::clone(&tsfn));

    let handler_tsfn = Arc::clone(&tsfn);
    hopenconnect_core::set_external_browser_handler(Some(Box::new(move |uri| {
        open_browser_url_with(&handler_tsfn, uri)
    })));
    Ok(())
}

pub(crate) fn clear_external_browser_handler() {
    hopenconnect_core::set_external_browser_handler(None);
    if let Ok(mut slot) = EXTERNAL_BROWSER.write() {
        *slot = None;
    }
}

fn open_browser_url_with(tsfn: &OpenBrowserThreadsafeFunction, uri: &str) -> bool {
    if uri.trim().is_empty() {
        return false;
    }
    let (tx, rx) = mpsc::sync_channel(1);
    let status = tsfn.call_with_return_value(
        uri.to_owned(),
        ThreadsafeFunctionCallMode::NonBlocking,
        move |result, _env| {
            let _ = tx.send(result.unwrap_or(false));
            Ok(())
        },
    );
    if status != Status::Ok {
        return false;
    }
    rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
}
