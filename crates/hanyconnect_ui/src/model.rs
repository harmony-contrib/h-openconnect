//! UI-facing re-exports of the session model.
//!
//! Keep presentation code free of core crate paths while remaining SDK-ready.

pub use hanyconnect_core::{
    AuthChallenge, AuthChallengeReply, AuthFieldChoice, AuthFieldKey, AuthFieldKind,
    AuthFieldValue, AuthMethod, ConnectionLifecycle, ConnectionProfile as VpnConnection,
    NetworkSnapshot, ProtocolKind, SessionSnapshot, SessionStats, SoftwareToken, SplitTunnelMode,
};

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}
