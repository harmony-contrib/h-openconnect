//! Per-fd ICS socket protect for the VPN **extension** process.
//!
//! OpenConnect runs its CSTP/DTLS sockets on the physical network before the
//! TUN is up, so every control socket must be protected with
//! `vpnConnection.protect(fd)` on the ArkTS side. This module is used by the
//! extension process, which does not host the Arkit UI bridge, so it keeps the
//! direct `ThreadsafeFunction` contract instead of a bridge plugin.

use napi_ohos::{
    bindgen_prelude::{spawn as spawn_napi_future, Function, JsObjectValue, Object, Promise},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
    Error, Result, Status,
};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

/// Keep OpenConnect CSTP/DTLS sockets off the OpenHarmony tunnel.
type SocketProtectCall<'a> = Function<'a, i32, Promise<()>>;
type SocketProtectThreadsafeFunction = ThreadsafeFunction<i32, Promise<()>, i32, Status, false>;
type SocketProtectSlot = LazyLock<RwLock<Option<Arc<SocketProtectThreadsafeFunction>>>>;

static SOCKET_PROTECT: SocketProtectSlot = LazyLock::new(|| RwLock::new(None));

pub(crate) fn register_socket_protect(callbacks: Object<'static>) -> Result<()> {
    if !callbacks.has_named_property("protectSocket")? {
        return Ok(());
    }
    let protect_socket: SocketProtectCall<'static> = callbacks.get_named_property("protectSocket")?;
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
    Ok(())
}

pub(crate) fn clear_socket_protect() {
    hopenconnect_core::set_socket_protect_handler(None);
    if let Ok(mut slot) = SOCKET_PROTECT.write() {
        *slot = None;
    }
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
                    std::mem::drop(spawn_napi_future(async move {
                        let result = promise.await.map_err(|err| err.to_string());
                        let _ = completion_tx.send(result);
                    }));
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
