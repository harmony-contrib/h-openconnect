//! Cross-process VPN payloads used by the paws-aligned ashmem bridge.
//!
//! HarmonyOS runs `EntryAbility` and `VpnExtensionAbility` in different
//! processes. Live state, authenticated session handoff, and one-shot platform
//! requests all stay in the app-owned ashmem region and never touch disk.

use crate::model::{ConnectionLifecycle, DiagnosticEntry, NetworkSnapshot, SessionStats};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformStartOutcome {
    #[default]
    Idle,
    Pending,
    Connected,
    Failed,
    Cancelled,
}

const SESSION_HANDOFF_MAX_AGE_NANOS: u128 = 10 * 60 * 1_000_000_000;
const BROWSER_REQUEST_MAX_AGE_MILLIS: u64 = 2 * 60 * 1_000;

/// UI-authenticated cookie and connection parameters for the isolated VPN
/// Extension process. The attempt id prevents a late Extension request from
/// consuming credentials prepared for a newer connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandoff {
    pub attempt_id: String,
    pub options: crate::model::VpnOptions,
    pub network: crate::model::NetworkSnapshot,
    #[serde(default)]
    pub updated_at: u128,
}

impl SessionHandoff {
    pub fn is_valid_for(&self, attempt_id: &str) -> bool {
        !attempt_id.is_empty()
            && self.attempt_id == attempt_id
            && self.updated_at > 0
            && PlatformVpnState::now_nanos().saturating_sub(self.updated_at)
                <= SESSION_HANDOFF_MAX_AGE_NANOS
    }
}

/// One-shot Extension-to-UI request to open an SSO URL in the system browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOpenRequest {
    pub request_id: String,
    pub attempt_id: String,
    pub uri: String,
    pub requested_at_ms: u64,
}

impl BrowserOpenRequest {
    pub fn is_valid_for(&self, attempt_id: &str) -> bool {
        let now = PlatformVpnState::now_millis();
        !self.request_id.is_empty()
            && !attempt_id.is_empty()
            && self.attempt_id == attempt_id
            && self.requested_at_ms > 0
            && now.saturating_sub(self.requested_at_ms) <= BROWSER_REQUEST_MAX_AGE_MILLIS
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformVpnState {
    /// Identity and exactly-once outcome of the current platform start request.
    /// These fields prevent a late extension response from completing a newer
    /// request after the system authorization flow has rebound the ability.
    pub start_attempt_id: String,
    pub start_outcome: PlatformStartOutcome,
    /// Set by the VPN Extension process after it has attached the shared-memory
    /// session and bound the matching Want. This distinguishes a dispatched
    /// request from one the extension has actually accepted.
    pub extension_attached: bool,
    pub starting: bool,
    pub running: bool,
    pub lifecycle: ConnectionLifecycle,
    pub last_error: Option<String>,
    pub assigned_ip: String,
    pub gateway: String,
    pub mtu: u32,
    pub network: NetworkSnapshot,
    pub stats: SessionStats,
    /// Process-local diagnostic tail for live cross-process display.
    pub diagnostics: Vec<DiagnosticEntry>,
    /// Strictly monotonic Unix-nanosecond revision within an ashmem session.
    pub updated_at: u128,
}

impl PlatformVpnState {
    pub fn now_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    pub fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default()
    }
}
