//! Platform socket protect (keep CSTP/DTLS on the physical network).
//!
//! When wide on-link routes cover most of IPv4, any unprotected socket to the
//! VPN gateway would be routed into the TUN and loop. OpenConnect calls the
//! protect handler for each new control-plane fd; ArkTS registers a callback
//! that invokes `vpnConnection.protect(fd)`.

use std::sync::Mutex;

type ProtectFn = Box<dyn FnMut(i32) + Send>;

static PROTECT: Mutex<Option<ProtectFn>> = Mutex::new(None);

/// Install (or clear) the platform protect callback.
pub fn set_handler(handler: Option<ProtectFn>) {
    if let Ok(mut guard) = PROTECT.lock() {
        *guard = handler;
    }
}

/// Invoked from OpenConnect for each new socket that must bypass the tunnel.
pub fn invoke(fd: i32) {
    if fd < 0 {
        return;
    }
    if let Ok(mut guard) = PROTECT.lock() {
        if let Some(handler) = guard.as_mut() {
            handler(fd);
            crate::e2e::e2e_marker("socket_protect", format!("fd={fd}"));
        }
    }
}
