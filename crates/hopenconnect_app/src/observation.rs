//! Page-scoped runtime domain observation and lease-fenced domain queries.

use crate::sanitize::{sanitize_auth_challenge, sanitize_diagnostic_entry, sanitize_optional};
use crate::{AppError, PreferencesSnapshot};
use hopenconnect_core::{
    AuthChallenge, ConnectionLifecycle, ConnectionProfile, DiagnosticEntry, LogRecordingStatus,
    NetworkSnapshot, RuntimeRevisions, SessionEngine, SessionStats, TrafficSample,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeDomain {
    Config,
    Status,
    Profiles,
    Statistics,
    Logs,
    About,
}

impl RuntimeDomain {
    pub fn is_critical(self) -> bool {
        matches!(self, Self::Config | Self::Status)
    }

    pub fn changed(self, previous: RuntimeRevisions, current: RuntimeRevisions) -> bool {
        match self {
            Self::Config => previous.config != current.config,
            Self::Status => previous.status != current.status,
            Self::Profiles => previous.profiles != current.profiles,
            Self::Statistics => previous.statistics != current.statistics,
            Self::Logs => previous.logs != current.logs,
            Self::About => previous.about != current.about,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDomainKey {
    pub domain: RuntimeDomain,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Subscription {
    pub generation: u64,
    pub domains: Vec<RuntimeDomainKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Query {
    pub generation: u64,
    pub domain: RuntimeDomain,
}

impl Query {
    pub fn key(&self) -> RuntimeDomainKey {
        RuntimeDomainKey {
            domain: self.domain,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainRevision {
    pub domain: RuntimeDomain,
    pub revision: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Changed {
    pub generation: u64,
    pub revision: u64,
    pub changes: Vec<DomainRevision>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Barrier {
    pub generation: u64,
    pub required: Vec<DomainRevision>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Observation {
    pub change: Changed,
    pub barrier: Barrier,
}

struct State {
    generation: u64,
    closed: bool,
    last: RuntimeRevisions,
    domains: BTreeMap<RuntimeDomainKey, u64>,
    in_flight: BTreeSet<RuntimeDomainKey>,
}

pub(crate) struct StateObservation {
    state: Mutex<State>,
    revisions: tokio::sync::watch::Receiver<RuntimeRevisions>,
}

impl StateObservation {
    pub fn new(revisions: tokio::sync::watch::Receiver<RuntimeRevisions>) -> Self {
        let last = *revisions.borrow();
        Self {
            state: Mutex::new(State {
                generation: 0,
                closed: false,
                last,
                domains: BTreeMap::new(),
                in_flight: BTreeSet::new(),
            }),
            revisions,
        }
    }

    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            state.domains.clear();
            state.in_flight.clear();
        }
    }

    pub fn subscribe(&self, request: Subscription) -> Result<Changed, AppError> {
        if request.generation == 0
            || request.generation > 9_007_199_254_740_991
            || request.domains.len() > 32
        {
            return Err(AppError::InvalidRequest(
                "invalid observation generation or domain count".into(),
            ));
        }
        let mut domains = BTreeMap::new();
        for key in request.domains {
            domains.insert(key, 0);
        }
        for domain in [RuntimeDomain::Config, RuntimeDomain::Status] {
            domains.insert(RuntimeDomainKey { domain }, 0);
        }
        let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        if state.closed {
            return Err(AppError::ObservationClosed);
        }
        if request.generation < state.generation
            || request.generation == state.generation && !state.domains.keys().eq(domains.keys())
        {
            return Err(AppError::ObservationSuperseded);
        }
        let current = *self.revisions.borrow();
        if request.generation > state.generation {
            state.generation = request.generation;
            state.domains = domains;
            state.in_flight.clear();
        }
        state.last = current;
        for version in state.domains.values_mut() {
            *version = current.revision;
        }
        Ok(Changed {
            generation: state.generation,
            revision: current.revision,
            changes: Self::versions(&state, false),
        })
    }

    pub fn validate(&self, generation: u64, key: &RuntimeDomainKey) -> Result<(), AppError> {
        let state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        if state.closed {
            return Err(AppError::ObservationClosed);
        }
        if state.generation != generation || !state.domains.contains_key(key) {
            return Err(AppError::ObservationSuperseded);
        }
        Ok(())
    }

    pub fn begin_query(
        &self,
        generation: u64,
        key: RuntimeDomainKey,
    ) -> Result<QueryLease<'_>, AppError> {
        let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        if state.closed {
            return Err(AppError::ObservationClosed);
        }
        if state.generation != generation || !state.domains.contains_key(&key) {
            return Err(AppError::ObservationSuperseded);
        }
        if !state.in_flight.insert(key.clone()) {
            return Err(AppError::ObservationQueryBusy);
        }
        Ok(QueryLease {
            owner: self,
            generation,
            key,
        })
    }

    pub fn observe(&self, critical_only: bool) -> Result<Observation, AppError> {
        let mut state = self.state.lock().map_err(|_| AppError::Poisoned)?;
        if state.closed {
            return Err(AppError::ObservationClosed);
        }
        let current = *self.revisions.borrow();
        let previous = state.last;
        let mut changes = Vec::new();
        for (key, version) in &mut state.domains {
            if key.domain.changed(previous, current) {
                *version = current.revision;
                changes.push(DomainRevision {
                    domain: key.domain,
                    revision: *version,
                });
            }
        }
        state.last = current;
        Ok(Observation {
            change: Changed {
                generation: state.generation,
                revision: current.revision,
                changes,
            },
            barrier: Barrier {
                generation: state.generation,
                required: Self::versions(&state, critical_only),
            },
        })
    }

    fn versions(state: &State, critical_only: bool) -> Vec<DomainRevision> {
        state
            .domains
            .iter()
            .filter(|(key, _)| !critical_only || key.domain.is_critical())
            .map(|(key, revision)| DomainRevision {
                domain: key.domain,
                revision: *revision,
            })
            .collect()
    }
}

pub(crate) struct QueryLease<'a> {
    owner: &'a StateObservation,
    generation: u64,
    key: RuntimeDomainKey,
}

impl Drop for QueryLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.state.lock() {
            if state.generation == self.generation {
                state.in_flight.remove(&self.key);
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDomainData {
    pub active_connection_id: Option<String>,
    pub language: String,
    pub theme: String,
    pub system_locale: String,
    pub system_color_mode: i32,
    pub sdk_ready: bool,
    pub backend: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDomainData {
    pub lifecycle: ConnectionLifecycle,
    pub vpn_session_id: String,
    pub last_error: Option<String>,
    pub sdk_ready: bool,
    pub backend: String,
    pub pending_auth: Option<AuthChallenge>,
    pub operation_pending: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesDomainData {
    pub active_connection_id: Option<String>,
    pub profiles: Vec<ConnectionProfile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsDomainData {
    pub lifecycle: ConnectionLifecycle,
    pub stats: SessionStats,
    pub network: NetworkSnapshot,
    pub traffic_history: Vec<TrafficSample>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsDomainData {
    pub entries: Vec<DiagnosticEntry>,
    pub recording: LogRecordingStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutDomainData {
    pub app_version: String,
    pub openconnect_version: Option<String>,
    pub backend: String,
    pub sdk_ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainProjection {
    pub generation: u64,
    pub domain: RuntimeDomain,
    pub revision: u64,
    pub data: Value,
}

pub(crate) fn build_domain_projection(
    engine: &SessionEngine,
    preferences: PreferencesSnapshot,
    generation: u64,
    domain: RuntimeDomain,
) -> Result<DomainProjection, AppError> {
    let revisions = engine.runtime_revisions();
    let snapshot = engine.snapshot()?;
    let data = match domain {
        RuntimeDomain::Config => serde_json::to_value(ConfigDomainData {
            active_connection_id: snapshot.active_connection_id,
            language: preferences.language,
            theme: preferences.theme,
            system_locale: preferences.system_locale,
            system_color_mode: preferences.system_color_mode,
            sdk_ready: snapshot.sdk_ready,
            backend: snapshot.backend,
        })?,
        RuntimeDomain::Status => {
            let (vpn_session_id, operation_pending, _) = engine.platform_status_meta()?;
            serde_json::to_value(StatusDomainData {
                lifecycle: snapshot.lifecycle,
                vpn_session_id,
                last_error: sanitize_optional(snapshot.last_error.as_deref()),
                sdk_ready: snapshot.sdk_ready,
                backend: snapshot.backend,
                pending_auth: snapshot.pending_auth.as_ref().map(sanitize_auth_challenge),
                operation_pending: operation_pending || snapshot.lifecycle.is_busy(),
            })?
        }
        RuntimeDomain::Profiles => serde_json::to_value(ProfilesDomainData {
            active_connection_id: snapshot.active_connection_id,
            profiles: snapshot.connections,
        })?,
        RuntimeDomain::Statistics => {
            let traffic_history = engine.traffic_history()?;
            serde_json::to_value(StatisticsDomainData {
                lifecycle: snapshot.lifecycle,
                stats: snapshot.stats,
                network: snapshot.network,
                traffic_history,
            })?
        }
        RuntimeDomain::Logs => {
            let recording = engine.log_recording_status()?;
            let entries = snapshot
                .diagnostics
                .iter()
                .map(sanitize_diagnostic_entry)
                .collect();
            serde_json::to_value(LogsDomainData { entries, recording })?
        }
        RuntimeDomain::About => serde_json::to_value(AboutDomainData {
            app_version: snapshot.app_version,
            openconnect_version: snapshot.anyconnect_version,
            backend: snapshot.backend,
            sdk_ready: snapshot.sdk_ready,
        })?,
    };
    Ok(DomainProjection {
        generation,
        domain,
        revision: revisions.revision,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn observation_tracks_domain_revisions_and_enforces_query_leases() {
        let initial = RuntimeRevisions {
            revision: 1,
            config: 1,
            status: 1,
            profiles: 1,
            statistics: 1,
            logs: 1,
            about: 1,
        };
        let (tx, rx) = tokio::sync::watch::channel(initial);
        let obs = StateObservation::new(rx);
        let sub: Subscription =
            serde_json::from_value(json!({"generation": 1, "domains": [{"domain": "logs"}]}))
                .unwrap();
        let init = obs.subscribe(sub).unwrap();
        assert_eq!(init.changes.len(), 3); // config, status, logs

        // Statistics update does not dirty logs/config/status.
        tx.send_replace(RuntimeRevisions {
            revision: 2,
            statistics: 2,
            ..initial
        });
        let tick = obs.observe(false).unwrap();
        assert!(tick.change.changes.is_empty());

        // Logs update dirties only logs.
        tx.send_replace(RuntimeRevisions {
            revision: 3,
            statistics: 2,
            logs: 2,
            ..initial
        });
        let next = obs.observe(false).unwrap();
        assert_eq!(next.change.changes.len(), 1);
        assert_eq!(next.change.changes[0].domain, RuntimeDomain::Logs);

        // Lease prevents duplicate in-flight queries and generation bump fences stale lease.
        let key = RuntimeDomainKey {
            domain: RuntimeDomain::Logs,
        };
        let lease1 = obs.begin_query(1, key.clone()).unwrap();
        assert!(matches!(
            obs.begin_query(1, key.clone()),
            Err(AppError::ObservationQueryBusy)
        ));
        let sub2: Subscription =
            serde_json::from_value(json!({"generation": 2, "domains": [{"domain": "logs"}]}))
                .unwrap();
        obs.subscribe(sub2).unwrap();
        assert!(obs.validate(1, &key).is_err());
        let lease2 = obs.begin_query(2, key.clone()).unwrap();
        drop(lease1);
        assert!(matches!(
            obs.begin_query(2, key.clone()),
            Err(AppError::ObservationQueryBusy)
        ));
        drop(lease2);
        assert!(obs.begin_query(2, key).is_ok());
    }
}
