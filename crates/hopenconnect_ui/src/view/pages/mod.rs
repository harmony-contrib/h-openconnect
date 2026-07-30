mod about;
mod challenge;
mod connections;
mod home;
mod logs;
mod settings;
mod statistics;

pub(super) use about::about_page;
pub(super) use challenge::auth_challenge_overlay;
pub(super) use connections::{connection_editor_page, connections_page};
pub(super) use home::home_page;
pub(super) use logs::diagnostics_page;
pub(super) use settings::{appearance_page, more_page};
pub(super) use statistics::statistics_page;
