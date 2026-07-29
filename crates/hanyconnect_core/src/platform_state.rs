//! Cross-process VPN payloads used by the paws-aligned platform bridge.
//!
//! HarmonyOS runs `EntryAbility` and `VpnExtensionAbility` in different
//! processes. Live lifecycle/network/traffic state is exchanged through
//! ashmem. `SessionHandoff` remains a private, short-lived file because the
//! authenticated cookie can exceed HarmonyOS Want parameter limits.

use crate::error::{CoreError, CoreResult};
use crate::model::{ConnectionLifecycle, DiagnosticEntry, NetworkSnapshot, SessionStats};
use crate::private_fs::{ensure_private_dir, write_atomic_private};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Session handoff for the VPN-extension process (cookie / credentials).
/// Want parameters can truncate large cookies; the file is the source of truth.
pub const SESSION_HANDOFF_FILE: &str = "session-handoff.json";
const SESSION_HANDOFF_MAX_AGE_NANOS: u128 = 10 * 60 * 1_000_000_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandoff {
    pub options: crate::model::VpnOptions,
    pub network: crate::model::NetworkSnapshot,
    #[serde(default)]
    pub updated_at: u128,
}

impl SessionHandoff {
    pub fn path(home: &Path) -> PathBuf {
        home.join(SESSION_HANDOFF_FILE)
    }

    pub fn save(&self, home: &Path) -> CoreResult<()> {
        ensure_private_dir(home)?;
        let path = Self::path(home);
        let content = serde_json::to_vec_pretty(self)
            .map_err(|err| CoreError::msg(format!("serialize session handoff: {err}")))?;
        write_atomic_private(&path, &content)
    }

    pub fn load(home: &Path) -> Option<Self> {
        let path = Self::path(home);
        let content = fs::read(&path).ok()?;
        let handoff: Self = match serde_json::from_slice(&content) {
            Ok(handoff) => handoff,
            Err(_) => {
                let _ = fs::remove_file(path);
                return None;
            }
        };
        let age = PlatformVpnState::now_nanos().saturating_sub(handoff.updated_at);
        if handoff.updated_at == 0 || age > SESSION_HANDOFF_MAX_AGE_NANOS {
            let _ = fs::remove_file(path);
            return None;
        }
        Some(handoff)
    }

    pub fn clear(home: &Path) {
        let _ = fs::remove_file(Self::path(home));
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlatformVpnState {
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
}
