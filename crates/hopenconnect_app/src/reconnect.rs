//! Tracks lifecycle edges so unexpected drops on `connect_on_demand` profiles
//! trigger automatic reconnection while explicit user disconnects do not.

use crate::AppError;
use hopenconnect_core::{ConnectionLifecycle, SessionEngine};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickOutcome {
    pub lifecycle: ConnectionLifecycle,
    pub should_reconnect: bool,
    pub active_connection_id: Option<String>,
}

struct ReconnectState {
    last_lifecycle: ConnectionLifecycle,
    user_disconnect: bool,
}

pub(crate) struct AutoReconnectTracker {
    state: Mutex<ReconnectState>,
}

impl AutoReconnectTracker {
    pub fn new(initial_lifecycle: ConnectionLifecycle) -> Self {
        Self {
            state: Mutex::new(ReconnectState {
                last_lifecycle: initial_lifecycle,
                user_disconnect: false,
            }),
        }
    }

    pub fn mark_connecting(&self) -> Result<(), AppError> {
        let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        state.user_disconnect = false;
        state.last_lifecycle = ConnectionLifecycle::Connecting;
        Ok(())
    }

    pub fn mark_user_disconnect(&self) -> Result<(), AppError> {
        let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        state.user_disconnect = true;
        Ok(())
    }

    /// Observe native engine tick and detect unexpected terminal transitions.
    ///
    /// Preserves the strict ordering invariant: capture `prev` BEFORE calling
    /// `engine.tick()`, then evaluate `prev.should_auto_reconnect_to(now, ...)`,
    /// then persist `last_lifecycle = now`.
    pub fn tick_engine(&self, engine: &SessionEngine) -> Result<TickOutcome, AppError> {
        let (prev, user_disconnect) = {
            let state = self.state.lock().map_err(|_| AppError::Poisoned)?;
            (state.last_lifecycle, state.user_disconnect)
        };

        let _ = engine.tick()?;
        let _ = engine.record_traffic_sample()?;
        let snapshot = engine.snapshot()?;
        let (_, _, dry_run) = engine.platform_status_meta()?;
        let now = snapshot.lifecycle;
        let connect_on_demand = snapshot
            .active_connection_id
            .as_deref()
            .and_then(|id| snapshot.connections.iter().find(|p| p.id == id))
            .is_some_and(|profile| profile.connect_on_demand);

        let should_reconnect =
            prev.should_auto_reconnect_to(now, user_disconnect, dry_run, connect_on_demand);

        {
            let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
            state.last_lifecycle = now;
        }

        Ok(TickOutcome {
            lifecycle: now,
            should_reconnect,
            active_connection_id: snapshot.active_connection_id,
        })
    }

    #[cfg(test)]
    pub(crate) fn set_last_lifecycle_for_test(&self, lifecycle: ConnectionLifecycle) {
        if let Ok(mut state) = self.state.lock() {
            state.last_lifecycle = lifecycle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hopenconnect_core::ConnectionProfile;

    #[test]
    fn detects_unexpected_drop_only_when_connect_on_demand_and_not_user_disconnect() {
        let engine = SessionEngine::new();
        let mut profile = ConnectionProfile::new_draft();
        profile.id = "hq".into();
        profile.name = "HQ".into();
        profile.server = "gw.example.com".into();
        profile.connect_on_demand = true;
        engine.upsert_profile(profile).unwrap();

        let tracker = AutoReconnectTracker::new(ConnectionLifecycle::Connected);
        let outcome = tracker.tick_engine(&engine).unwrap();
        assert_eq!(outcome.lifecycle, ConnectionLifecycle::Disconnected);
        assert!(outcome.should_reconnect);

        // Second tick after recording Disconnected does not re-trigger.
        let second = tracker.tick_engine(&engine).unwrap();
        assert!(!second.should_reconnect);

        // Explicit user disconnect suppresses reconnect.
        tracker.set_last_lifecycle_for_test(ConnectionLifecycle::Connected);
        tracker.mark_user_disconnect().unwrap();
        let after_user_stop = tracker.tick_engine(&engine).unwrap();
        assert!(!after_user_stop.should_reconnect);
    }
}
