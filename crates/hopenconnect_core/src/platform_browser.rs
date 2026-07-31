//! Platform external browser opener (SAML / SSO).
//!
//! OpenConnect SSO-v2 listens on `http://[::1]:29786/api/sso/...` then asks the
//! host to open `sso_login` in an external browser.
//!
//! Dual-process HarmonyOS:
//! - Full auth runs in the VPN **extension** process (no UI Ability).
//! - Opening the system browser needs the **UI** process (`startAbility`).
//! - When no in-process handler is registered, the request is published through
//!   the Extension-owned ashmem lane for the UI to consume exactly once.

use std::sync::Mutex;

pub use crate::platform_state::BrowserOpenRequest;

type OpenBrowserFn = Box<dyn FnMut(&str) -> bool + Send>;

static OPEN_BROWSER: Mutex<Option<OpenBrowserFn>> = Mutex::new(None);

/// Install (or clear) the in-process platform external-browser callback (UI).
pub fn set_handler(handler: Option<OpenBrowserFn>) {
    if let Ok(mut guard) = OPEN_BROWSER.lock() {
        *guard = handler;
    }
}

/// Invoked from OpenConnect when an SSO URL must open outside the app.
///
/// Returns `true` when the platform accepted the request (browser launched, or
/// a cross-process request was queued for the UI). Returning `false` makes
/// OpenConnect fail the SSO step with "Failed to spawn external browser".
pub fn open(uri: &str) -> bool {
    if uri.trim().is_empty() {
        return false;
    }

    // 1) Same-process handler (UI process, if auth ever runs there).
    let in_process = if let Ok(mut guard) = OPEN_BROWSER.lock() {
        if let Some(handler) = guard.as_mut() {
            handler(uri)
        } else {
            false
        }
    } else {
        false
    };
    if in_process {
        return true;
    }

    // 2) Cross-process: extension → UI via ashmem.
    crate::shared_engine()
        .queue_platform_browser_open_request(uri.to_owned())
        .is_ok()
}

/// UI poll: take a pending browser-open request, if any.
pub fn take_pending() -> Option<BrowserOpenRequest> {
    crate::shared_engine()
        .take_platform_browser_open_request()
        .ok()
        .flatten()
}

/// Clear any stale request (disconnect / new session).
pub fn clear_pending() {
    let _ = crate::shared_engine().clear_platform_browser_open_request();
}
