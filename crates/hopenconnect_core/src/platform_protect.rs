//! Platform socket protect (keep CSTP/DTLS on the physical network).
//!
//! When wide on-link routes cover most of IPv4, any unprotected socket to the
//! VPN gateway would be routed into the TUN and loop. OpenConnect calls the
//! protect handler for each new control-plane fd; ArkTS registers a callback
//! that invokes `vpnConnection.protect(fd)`.

use crate::{CoreError, CoreResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

type ProtectFn = dyn Fn(i32) -> CoreResult<()> + Send + Sync;

struct ProtectRegistration {
    generation: u64,
    handler: Arc<ProtectFn>,
    error: Mutex<Option<String>>,
}

#[derive(Clone)]
pub(crate) struct ProtectMonitor(Arc<ProtectRegistration>);

static PROTECT: RwLock<Option<Arc<ProtectRegistration>>> = RwLock::new(None);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Install (or clear) the platform protect callback.
pub fn set_handler(handler: Option<Box<ProtectFn>>) -> u64 {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut guard) = PROTECT.write() {
        *guard = handler.map(|handler| {
            Arc::new(ProtectRegistration {
                generation,
                handler: Arc::from(handler),
                error: Mutex::new(None),
            })
        });
    }
    generation
}

/// Invoked from OpenConnect for each new socket that must bypass the tunnel.
pub fn invoke(fd: i32) {
    if fd < 0 {
        return;
    }
    // Clone under the registry lock and execute outside it: the ArkTS Promise
    // may take seconds, while disconnect/rebind must still be able to clear or
    // replace the callback.
    let registration = PROTECT
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(Arc::clone));
    if let Some(registration) = registration {
        ProtectMonitor(registration).invoke(fd);
    }
}

/// Start a protected network operation and clear only this handler generation's
/// previous error. A late callback from an old handler cannot poison it.
pub(crate) fn begin_operation() -> CoreResult<Option<ProtectMonitor>> {
    let registration = PROTECT
        .read()
        .map_err(|_| CoreError::msg("socket protect registry lock poisoned"))?
        .as_ref()
        .map(Arc::clone);
    let Some(registration) = registration else {
        if cfg!(target_env = "ohos") {
            return Err(CoreError::msg(
                "platform socket protect handler is not registered",
            ));
        }
        return Ok(None);
    };
    registration
        .error
        .lock()
        .map_err(|_| CoreError::msg("socket protect error lock poisoned"))?
        .take();
    Ok(Some(ProtectMonitor(registration)))
}

impl ProtectMonitor {
    pub(crate) fn generation(&self) -> u64 {
        self.0.generation
    }

    pub(crate) fn take_error(&self) -> CoreResult<()> {
        let error = self
            .0
            .error
            .lock()
            .map_err(|_| CoreError::msg("socket protect error lock poisoned"))?
            .take();
        match error {
            Some(error) => Err(CoreError::msg(format!(
                "platform socket protection failed (registration {}): {error}",
                self.generation()
            ))),
            None => Ok(()),
        }
    }

    /// Invoke the callback captured for this VPN generation. Reconnects from a
    /// detached old OpenConnect worker must never consult the mutable global
    /// registry and accidentally protect their fd through a newer connection.
    pub(crate) fn invoke(&self, fd: i32) {
        if fd < 0 {
            return;
        }
        if let Err(error) = (self.0.handler)(fd) {
            if let Ok(mut last_error) = self.0.error.lock() {
                *last_error = Some(error.to_string());
            }
        }
    }

    pub(crate) fn clear_error(&self) -> CoreResult<()> {
        self.0
            .error
            .lock()
            .map_err(|_| CoreError::msg("socket protect error lock poisoned"))?
            .take();
        Ok(())
    }
}

pub(crate) fn finish_operation(monitor: Option<&ProtectMonitor>) -> CoreResult<()> {
    monitor.map_or(Ok(()), ProtectMonitor::take_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn replacement_does_not_wait_for_or_inherit_old_callback_error() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = Arc::new(Mutex::new(release_rx));
        set_handler(Some(Box::new(move |_fd| {
            entered_tx.send(()).expect("report callback entry");
            release_rx
                .lock()
                .expect("lock callback release")
                .recv()
                .expect("release callback");
            Err(CoreError::msg("old registration failed"))
        })));
        let old_monitor = begin_operation().expect("old monitor").expect("handler");

        let callback = std::thread::spawn(|| invoke(7));
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("old callback entered");

        // This write would deadlock if invoke held the registry lock while the
        // platform callback/Promise was outstanding.
        set_handler(Some(Box::new(|_fd| Ok(()))));
        let new_monitor = begin_operation().expect("new monitor").expect("handler");
        release_tx.send(()).expect("release old callback");
        callback.join().expect("old callback thread");

        assert!(new_monitor.take_error().is_ok());
        assert!(old_monitor
            .take_error()
            .expect_err("old failure remains on old generation")
            .to_string()
            .contains("old registration failed"));
        set_handler(None);
    }
}
