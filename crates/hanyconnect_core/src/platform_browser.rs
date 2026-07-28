//! Platform external browser opener (SAML / SSO).
//!
//! OpenConnect SSO-v2 listens on `http://[::1]:29786/api/sso/...` then asks the
//! host to open `sso_login` in an external browser.
//!
//! Dual-process HarmonyOS:
//! - Full auth runs in the VPN **extension** process (no UI Ability).
//! - Opening the system browser needs the **UI** process (`startAbility`).
//! - When no in-process handler is registered, we write
//!   `browser-request.json` under `HANYCONNECT_HOME` for the UI to poll.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

type OpenBrowserFn = Box<dyn FnMut(&str) -> bool + Send>;

static OPEN_BROWSER: Mutex<Option<OpenBrowserFn>> = Mutex::new(None);

const REQUEST_FILE: &str = "browser-request.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOpenRequest {
    pub uri: String,
    pub requested_at_ms: u64,
}

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
        crate::e2e::e2e_marker(
            "external_browser",
            format!("mode=in_process uri_len={}", uri.len()),
        );
        return true;
    }

    // 2) Cross-process: extension → UI via filesDir.
    let ok = queue_for_ui(uri);
    crate::e2e::e2e_marker(
        "external_browser",
        format!("mode=file_queue ok={ok} uri_len={}", uri.len()),
    );
    ok
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HANYCONNECT_HOME").map(PathBuf::from)
}

fn request_path(home: &Path) -> PathBuf {
    home.join(REQUEST_FILE)
}

fn queue_for_ui(uri: &str) -> bool {
    let Some(home) = home_dir() else {
        return false;
    };
    let _ = std::fs::create_dir_all(&home);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let req = BrowserOpenRequest {
        uri: uri.to_owned(),
        requested_at_ms: now,
    };
    let Ok(json) = serde_json::to_string_pretty(&req) else {
        return false;
    };
    let path = request_path(&home);
    let tmp = home.join(format!("{REQUEST_FILE}.tmp"));
    if std::fs::write(&tmp, json).is_err() {
        return false;
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

/// UI poll: take a pending browser-open request, if any.
pub fn take_pending() -> Option<BrowserOpenRequest> {
    let home = home_dir()?;
    let path = request_path(&home);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    serde_json::from_str(&text).ok()
}

/// Clear any stale request (disconnect / new session).
pub fn clear_pending() {
    if let Some(home) = home_dir() {
        let _ = std::fs::remove_file(request_path(&home));
    }
}
