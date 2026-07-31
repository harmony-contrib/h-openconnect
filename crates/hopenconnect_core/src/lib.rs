//! Session engine for H-OpenConnect.
//!
//! Architecture (aligned with paws):
//! - UI (`hopenconnect_ui`) owns presentation and dispatches connect/disconnect.
//! - This crate owns profile persistence, lifecycle, diagnostics, and
//!   the AnyConnect protocol path (`anyconnect-rs`) when enabled.
//! - HarmonyOS VpnExtensionAbility owns the TUN fd and notifies native code.

mod auth_bridge;
mod client_identity;
mod engine;
mod error;
mod log_recording;
mod model;
#[cfg(feature = "native-anyconnect")]
mod native_session;
mod platform_browser;
mod platform_ipc;
mod platform_protect;
mod platform_state;
mod private_fs;
mod store;

pub use platform_browser::{
    clear_pending as clear_browser_open_pending, set_handler as set_external_browser_handler,
    take_pending as take_browser_open_pending, BrowserOpenRequest,
};
pub use platform_protect::set_handler as set_socket_protect_handler;

pub use auth_bridge::AuthInteraction;
pub use client_identity::{
    configure_platform_identity, default_client_version, default_user_agent,
};
pub use engine::{shared_engine, SessionEngine};
pub use error::{CoreError, CoreResult};
pub use log_recording::{LogArchiveSummary, LogRecordingStatus};
pub use model::{
    AuthChallenge, AuthChallengeReply, AuthField, AuthFieldChoice, AuthFieldKey, AuthFieldKind,
    AuthFieldValue, AuthGroupDiscovery, AuthMethod, ConnectRequest, ConnectionLifecycle,
    ConnectionProfile, DiagnosticEntry, NetworkSnapshot, ProtocolKind, SessionSnapshot,
    SessionStats, SoftwareToken, SplitTunnelMode, VpnOptions,
};
pub use platform_state::PlatformStartOutcome;
pub use store::{Preferences, ProfileStore};

pub fn secure_private_file(path: impl AsRef<std::path::Path>) -> CoreResult<()> {
    private_fs::secure_existing_file(path.as_ref())
}
