mod challenge;
mod connections;
mod home;
mod more;
mod statistics;

pub(super) use challenge::auth_challenge_overlay;
pub(super) use connections::{connection_editor_page, connections_page};
pub(super) use home::home_page;
pub(super) use more::{about_page, appearance_page, diagnostics_page, more_page};
pub(super) use statistics::statistics_page;
