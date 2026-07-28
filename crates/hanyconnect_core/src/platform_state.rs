//! Cross-process platform VPN state (paws-aligned).
//!
//! HarmonyOS runs `EntryAbility` and `VpnExtensionAbility` in different
//! processes. In-memory `SessionEngine` is not shared; both sides read/write
//! `platform-vpn-state.json` under the app home directory.

use crate::error::{CoreError, CoreResult};
use crate::model::{ConnectionLifecycle, NetworkSnapshot, SessionStats};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PLATFORM_VPN_STATE_FILE: &str = "platform-vpn-state.json";
/// Session handoff for the VPN-extension process (cookie / credentials).
/// Want parameters can truncate large cookies; the file is the source of truth.
pub const SESSION_HANDOFF_FILE: &str = "session-handoff.json";

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
        fs::create_dir_all(home)?;
        let path = Self::path(home);
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        let content = serde_json::to_vec_pretty(self)
            .map_err(|err| CoreError::msg(format!("serialize session handoff: {err}")))?;
        fs::write(&temp, content)?;
        fs::rename(&temp, &path).map_err(|err| {
            let _ = fs::remove_file(&temp);
            CoreError::from(err)
        })?;
        Ok(())
    }

    pub fn load(home: &Path) -> Option<Self> {
        let content = fs::read(Self::path(home)).ok()?;
        serde_json::from_slice(&content).ok()
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
        fs::create_dir_all(home)?;
        let path = Self::path(home);
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        let content = serde_json::to_vec_pretty(self)
            .map_err(|err| CoreError::msg(format!("serialize platform VPN state: {err}")))?;
        fs::write(&temp, content)?;
        fs::rename(&temp, &path).map_err(|err| {
            let _ = fs::remove_file(&temp);
            CoreError::from(err)
        })?;
        Ok(())
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
        if self.owner_pid == std::process::id() {
            return true;
        }
        #[cfg(unix)]
        {
            if self.owner_pid > i32::MAX as u32 {
                return false;
            }
            // Signal 0 performs permission/existence checking without sending
            // a signal. The UI and its VPN extension share an application UID.
            let result = unsafe { libc::kill(self.owner_pid as i32, 0) };
            return result == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}
