//! Structured log markers for device / host E2E scripts.
//!
//! Scripts grep hilog / stdout for `HAnyConnectE2E` lines.

use serde::{Deserialize, Serialize};

pub const E2E_TAG: &str = "HAnyConnectE2E";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct E2eConfig {
    pub server: Option<String>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Explicit opt-in used only by device tests against a private/self-signed
    /// lab headend. The production profile default remains strict trust.
    pub accept_untrusted: bool,
    pub auto_connect: bool,
    pub dry_run: bool,
    pub expect_connected: bool,
    pub expect_failure: bool,
}

pub fn e2e_marker(event: &str, detail: impl AsRef<str>) {
    let detail = detail.as_ref();
    // Host tests read stderr. On device, hilog often drops process stderr;
    // always append a durable line under HANYCONNECT_HOME when set.
    let line = format!("[{E2E_TAG}] event={event} detail={detail}");
    eprintln!("{line}");
    tracing::info!(target: "hanyconnect_e2e", "{E2E_TAG} event={event} detail={detail}");
    #[cfg(target_env = "ohos")]
    {
        eprintln!("HAnyConnectE2E {event} {detail}");
    }
    if let Ok(home) = std::env::var("HANYCONNECT_HOME") {
        use std::io::Write;
        let path = std::path::Path::new(&home).join("e2e-markers.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}
