//! Cross-process platform VPN state (paws-aligned).
//!
//! HarmonyOS runs `EntryAbility` and `VpnExtensionAbility` in different
//! processes. In-memory `SessionEngine` is not shared; both sides read/write
//! `platform-vpn-state.json` under the app home directory.

use crate::error::{CoreError, CoreResult};
use crate::model::{ConnectionLifecycle, NetworkSnapshot, SessionStats};
use crate::private_fs::{ensure_private_dir, write_atomic_private};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PLATFORM_VPN_STATE_FILE: &str = "platform-vpn-state.json";
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
#[serde(rename_all = "camelCase")]
pub struct PlatformVpnState {
    pub starting: bool,
    pub running: bool,
    /// Process which last asserted `starting` or `running`.
    ///
    /// The VPN extension is a sibling process and may terminate without
    /// receiving `onDestroy` (for example after a native crash). Persisted
    /// active flags are valid only while their owning process is alive.
    #[serde(default)]
    pub owner_pid: u32,
    /// Linux process start ticks from `/proc/<pid>/stat`; prevents PID reuse
    /// from reviving state written by a previous VPN extension process.
    #[serde(default)]
    pub owner_start_ticks: u64,
    #[serde(default)]
    pub lifecycle: ConnectionLifecycle,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub assigned_ip: String,
    #[serde(default)]
    pub gateway: String,
    #[serde(default)]
    pub mtu: u32,
    #[serde(default)]
    pub network: NetworkSnapshot,
    #[serde(default)]
    pub stats: SessionStats,
    /// Unix nanos; used to ignore stale writes from a previous session.
    #[serde(default)]
    pub updated_at: u128,
}

impl PlatformVpnState {
    pub fn path(home: &Path) -> PathBuf {
        home.join(PLATFORM_VPN_STATE_FILE)
    }

    pub fn load(home: &Path) -> Option<Self> {
        let path = Self::path(home);
        let content = fs::read(&path).ok()?;
        serde_json::from_slice(&content).ok()
    }

    pub fn save(&self, home: &Path) -> CoreResult<()> {
        ensure_private_dir(home)?;
        let path = Self::path(home);
        let content = serde_json::to_vec_pretty(self)
            .map_err(|err| CoreError::msg(format!("serialize platform VPN state: {err}")))?;
        write_atomic_private(&path, &content)
    }

    pub fn now_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    pub fn owner_is_alive(&self) -> bool {
        if self.owner_pid == 0 {
            return false;
        }
        #[cfg(unix)]
        {
            if self.owner_pid > i32::MAX as u32 {
                return false;
            }
            // Signal 0 performs permission/existence checking without sending
            // a signal. The UI and its VPN extension share an application UID.
            let result = unsafe { libc::kill(self.owner_pid as i32, 0) };
            let alive =
                result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
            if !alive {
                return false;
            }
            return self.owner_start_ticks == 0
                || process_start_ticks(self.owner_pid) == Some(self.owner_start_ticks);
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

pub fn current_process_start_ticks() -> u64 {
    process_start_ticks(std::process::id()).unwrap_or(0)
}

fn process_start_ticks(pid: u32) -> Option<u64> {
    let raw = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = raw.rfind(')')?;
    raw.get(end + 1..)?.split_whitespace().nth(19)?.parse().ok()
}
