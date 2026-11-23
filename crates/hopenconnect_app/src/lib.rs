//! UI-independent application command and observation boundary for H-OpenConnect.
//!
//! Native hosts own permissions, file pickers, web auth sessions, and HarmonyOS
//! VPN abilities; this crate owns command validation, state projection, display
//! vocabulary sanitization, and auto-reconnect edge detection.

mod connection_qr;
mod observation;
mod reconnect;
mod request;
pub mod sanitize;

pub use observation::{
    AboutDomainData, ConfigDomainData, DomainProjection, LogsDomainData, ProfilesDomainData,
    RuntimeDomain, RuntimeDomainKey, StatisticsDomainData, StatusDomainData,
};
pub use reconnect::TickOutcome;
pub use request::{AppRequest, AppResponse, PlatformEffect};
pub use sanitize::sanitize_display_text;

use hopenconnect_core::{
    shared_engine, AuthChallengeReply, AuthFieldKind, AuthMethod, CoreError, SessionEngine,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("invalid application request: {0}")]
    InvalidRequest(String),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("application state lock poisoned")]
    Poisoned,
    #[error("observation owner was closed")]
    ObservationClosed,
    #[error("observation subscription was superseded or the domain is not subscribed")]
    ObservationSuperseded,
    #[error("a query for this observation domain is already in flight")]
    ObservationQueryBusy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesSnapshot {
    pub language: String,
    pub theme: String,
    pub system_locale: String,
    pub system_color_mode: i32,
}

struct SystemAppearance {
    locale: String,
    color_mode: i32,
}

static SYSTEM_APPEARANCE: LazyLock<Mutex<SystemAppearance>> = LazyLock::new(|| {
    let locale = std::env::var("HOPENCONNECT_UI_LOCALE")
        .or_else(|_| std::env::var("HMETA_UI_LOCALE"))
        .unwrap_or_else(|_| "zh-CN".to_owned());
    let color_mode = std::env::var("HOPENCONNECT_SYSTEM_COLOR_MODE")
        .or_else(|_| std::env::var("HMETA_SYSTEM_COLOR_MODE"))
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| (-1..=1).contains(v))
        .unwrap_or(1);
    Mutex::new(SystemAppearance { locale, color_mode })
});

pub fn set_system_locale(locale: String) -> Result<bool, AppError> {
    let trimmed = locale.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRequest("empty system locale".into()));
    }
    let mut state = SYSTEM_APPEARANCE.lock().map_err(|_| AppError::Poisoned)?;
    let changed = state.locale != trimmed;
    state.locale = trimmed.to_owned();
    Ok(changed)
}

pub fn set_system_color_mode(mode: i32) -> Result<bool, AppError> {
    if !(-1..=1).contains(&mode) {
        return Err(AppError::InvalidRequest("invalid color mode".into()));
    }
    let mut state = SYSTEM_APPEARANCE.lock().map_err(|_| AppError::Poisoned)?;
    let changed = state.color_mode != mode;
    state.color_mode = mode;
    Ok(changed)
}

pub fn preferences_snapshot(engine: &SessionEngine) -> Result<PreferencesSnapshot, AppError> {
    let (language, theme) = engine.appearance_preferences()?;
    let system = SYSTEM_APPEARANCE.lock().map_err(|_| AppError::Poisoned)?;
    Ok(PreferencesSnapshot {
        language,
        theme,
        system_locale: system.locale.clone(),
        system_color_mode: system.color_mode,
    })
}

fn dry_run_from_env(sdk_ready: bool) -> bool {
    match std::env::var("HOPENCONNECT_DRY_RUN") {
        Ok(value) => value == "1" || value.eq_ignore_ascii_case("true"),
        Err(_) => !sdk_ready,
    }
}

/// Pure application service with zero UI toolkit or N-API dependency.
pub struct Application {
    engine: Arc<SessionEngine>,
    observation: observation::StateObservation,
    reconnect: reconnect::AutoReconnectTracker,
}

impl Application {
    pub fn shared() -> Self {
        Self::with_engine(shared_engine())
    }

    pub fn with_engine(engine: Arc<SessionEngine>) -> Self {
        let initial_lifecycle = engine
            .snapshot()
            .map(|snap| snap.lifecycle)
            .unwrap_or_default();
        let observation = observation::StateObservation::new(engine.subscribe_runtime_revisions());
        Self {
            engine,
            observation,
            reconnect: reconnect::AutoReconnectTracker::new(initial_lifecycle),
        }
    }

    pub fn close(&self) {
        self.observation.close();
    }

    pub fn preferences(&self) -> Result<PreferencesSnapshot, AppError> {
        preferences_snapshot(&self.engine)
    }

    pub fn subscribe_state_json(&self, request: &str) -> Result<String, AppError> {
        let parsed: observation::Subscription = serde_json::from_str(request)?;
        Ok(serde_json::to_string(&self.observation.subscribe(parsed)?)?)
    }

    pub fn observe_state_json(&self, critical_only: bool) -> Result<String, AppError> {
        Ok(serde_json::to_string(
            &self.observation.observe(critical_only)?,
        )?)
    }

    pub async fn query_state_json(&self, request: &str) -> Result<String, AppError> {
        let query: observation::Query = serde_json::from_str(request)?;
        let key = query.key();
        let _lease = self
            .observation
            .begin_query(query.generation, key.clone())?;
        let prefs = self.preferences()?;
        let projection = observation::build_domain_projection(
            &self.engine,
            prefs,
            query.generation,
            query.domain,
        )?;
        let json = serde_json::to_string(&projection)?;
        self.observation.validate(query.generation, &key)?;
        Ok(json)
    }

    pub async fn wait_for_revision(&self, after: u64) -> Result<u64, AppError> {
        match tokio::time::timeout(
            Duration::from_secs(20),
            self.engine.wait_for_runtime_revision(after),
        )
        .await
        {
            Ok(revisions) => Ok(revisions.revision),
            Err(_) => Ok(self.engine.runtime_revisions().revision),
        }
    }

    pub fn tick_session(&self) -> Result<TickOutcome, AppError> {
        self.reconnect.tick_engine(&self.engine)
    }

    pub async fn dispatch_json(&self, text: &str) -> Result<String, AppError> {
        let request: AppRequest = serde_json::from_str(text)?;
        Ok(serde_json::to_string(&self.dispatch(request).await)?)
    }

    pub async fn dispatch(&self, request: AppRequest) -> AppResponse {
        let id = request.id;
        match self.execute(request).await {
            Ok((data, effect)) => AppResponse {
                id,
                ok: true,
                data: Some(data),
                effect,
                error: None,
            },
            Err(error) => AppResponse {
                id,
                ok: false,
                data: None,
                effect: None,
                error: Some(sanitize_display_text(&error.to_string())),
            },
        }
    }

    async fn execute(
        &self,
        request: AppRequest,
    ) -> Result<(Value, Option<PlatformEffect>), AppError> {
        use request::*;
        request.validate()?;
        let p = request.payload;
        let mut effect = None;

        let data = match request.command.as_str() {
            "vpn.connect" => {
                let snapshot = self.engine.snapshot()?;
                let profile = self.engine.active_profile()?.ok_or_else(|| {
                    AppError::InvalidRequest("select a connection profile first".into())
                })?;
                profile.validate().map_err(AppError::InvalidRequest)?;
                let dry_run = dry_run_from_env(snapshot.sdk_ready);
                if !dry_run
                    && matches!(
                        profile.auth_method,
                        AuthMethod::Password | AuthMethod::PasswordAndCertificate
                    )
                    && profile.password.trim().is_empty()
                {
                    return Err(AppError::InvalidRequest(
                        "password is required for this connection".into(),
                    ));
                }
                if !dry_run
                    && matches!(
                        profile.auth_method,
                        AuthMethod::Certificate | AuthMethod::PasswordAndCertificate
                    )
                    && profile.certificate.trim().is_empty()
                {
                    return Err(AppError::InvalidRequest(
                        "client certificate is required for this connection".into(),
                    ));
                }
                hopenconnect_core::clear_browser_open_pending();
                self.reconnect.mark_connecting()?;
                effect = Some(PlatformEffect::connect());
                json!({ "started": true })
            }
            "vpn.disconnect" => {
                self.reconnect.mark_user_disconnect()?;
                let _ = self.engine.cancel_auth_challenge();
                effect = Some(PlatformEffect::stop());
                Value::Null
            }
            "vpn.cancelConnect" => {
                self.reconnect.mark_user_disconnect()?;
                let _ = self.engine.cancel_auth_challenge();
                let _ = self.engine.disconnect().await;
                effect = Some(PlatformEffect::stop());
                Value::Null
            }
            "vpn.confirmSync" => {
                let _ = self.engine.sync_platform_changes();
                let outcome = self.tick_session()?;
                to_value(outcome)?
            }
            "vpn.resetState" => {
                self.reconnect.mark_user_disconnect()?;
                let _ = self.engine.cancel_auth_challenge();
                self.engine.disconnect().await?;
                effect = Some(PlatformEffect::stop());
                Value::Null
            }
            "auth.submit" => {
                let p: AuthSubmitPayload = payload(p)?;
                let challenge = self.engine.pending_auth().ok_or_else(|| {
                    AppError::InvalidRequest("no pending authentication form".into())
                })?;
                if challenge.id != p.id {
                    return Err(AppError::InvalidRequest(
                        "authentication form has changed".into(),
                    ));
                }
                for field in &challenge.fields {
                    if matches!(field.kind, AuthFieldKind::Hidden) || !field.required {
                        continue;
                    }
                    let supplied = p
                        .values
                        .iter()
                        .find(|entry| entry.key == field.key)
                        .map(|entry| entry.value.trim())
                        .unwrap_or("");
                    if supplied.is_empty() {
                        return Err(AppError::InvalidRequest(
                            "please complete all required verification fields".into(),
                        ));
                    }
                }
                let values: Vec<_> = p
                    .values
                    .into_iter()
                    .filter(|entry| !entry.value.trim().is_empty())
                    .collect();
                if values.is_empty() {
                    return Err(AppError::InvalidRequest(
                        "please enter a verification response".into(),
                    ));
                }
                self.engine.submit_auth_challenge(AuthChallengeReply {
                    id: challenge.id,
                    values,
                    cancelled: false,
                })?;
                Value::Null
            }
            "auth.cancel" => {
                self.reconnect.mark_user_disconnect()?;
                self.engine.cancel_auth_challenge()?;
                Value::Null
            }
            "profiles.select" => {
                let p: IdPayload = payload(p)?;
                self.engine.select_profile(&p.id)?;
                Value::Null
            }
            "profiles.save" => {
                let p: ProfilePayload = payload(p)?;
                let mut profile = p.profile;
                profile.name = profile.name.trim().to_owned();
                profile.server = profile.server.trim().to_owned();
                if profile.id.trim().is_empty() {
                    let millis = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    profile.id = format!("conn-{millis}");
                }
                let id = profile.id.clone();
                self.engine.upsert_profile(profile)?;
                json!({ "id": id })
            }
            "profiles.delete" => {
                let p: IdPayload = payload(p)?;
                self.engine.delete_profile(&p.id)?;
                Value::Null
            }
            "profiles.toggleFavorite" => {
                let p: IdPayload = payload(p)?;
                let favorite = self.engine.toggle_favorite(&p.id)?;
                json!({ "favorite": favorite })
            }
            "profiles.discoverGroups" => {
                let p: ProfilePayload = payload(p)?;
                let discovery = self.engine.discover_auth_groups(p.profile).await?;
                to_value(sanitize::sanitize_group_discovery(discovery))?
            }
            "profiles.setForceGlobal" => {
                let p: ForceGlobalPayload = payload(p)?;
                let session_live = self.engine.set_active_force_global(p.force_global)?;
                json!({ "sessionLive": session_live })
            }
            "profiles.encodeQr" => {
                let p: IdPayload = payload(p)?;
                let snapshot = self.engine.snapshot()?;
                let profile = snapshot
                    .connections
                    .iter()
                    .find(|item| item.id == p.id)
                    .ok_or_else(|| {
                        AppError::InvalidRequest("connection profile not found".into())
                    })?;
                let encoded = connection_qr::encode_connection_qr(profile)
                    .map_err(AppError::InvalidRequest)?;
                json!({ "name": profile.name, "payload": encoded })
            }
            "profiles.importQr" => {
                let p: QrImportPayload = payload(p)?;
                let mut profile = connection_qr::decode_connection_qr(&p.content)
                    .map_err(AppError::InvalidRequest)?;
                let snapshot = self.engine.snapshot()?;
                profile.id = connection_qr::new_imported_connection_id(&snapshot.connections);
                let id = profile.id.clone();
                let name = profile.name.clone();
                self.engine.upsert_profile(profile)?;
                json!({ "id": id, "name": name })
            }
            "logs.recordingStatus" => to_value(self.engine.log_recording_status()?)?,
            "logs.setRecording" => {
                let p: EnabledPayload = payload(p)?;
                to_value(self.engine.set_log_recording_enabled(p.enabled)?)?
            }
            "logs.readArchive" => {
                let p: ArchivePayload = payload(p)?;
                to_value(self.engine.read_log_archive(&p.file_name)?)?
            }
            "logs.deleteArchive" => {
                let p: ArchivePayload = payload(p)?;
                to_value(self.engine.delete_log_archive(&p.file_name)?)?
            }
            "logs.clear" => {
                self.engine.clear_diagnostics()?;
                Value::Null
            }
            "preferences.get" => to_value(self.preferences()?)?,
            "preferences.set" => {
                let patch: PreferencesPatch = payload(p)?;
                let (current_language, current_theme) = self.engine.appearance_preferences()?;
                let language = patch.language.unwrap_or(current_language);
                let theme = patch.theme.unwrap_or(current_theme);
                self.engine.set_appearance_preferences(&language, &theme)?;
                to_value(self.preferences()?)?
            }
            _ => {
                return Err(AppError::InvalidRequest(format!(
                    "unknown command: {}",
                    request.command
                )));
            }
        };

        Ok((data, effect))
    }
}

fn payload<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, AppError> {
    Ok(serde_json::from_value(value)?)
}

fn to_value<T: Serialize>(value: T) -> Result<Value, AppError> {
    Ok(serde_json::to_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hopenconnect_core::ConnectionProfile;
    use tempfile::tempdir;

    #[tokio::test]
    async fn profiles_preferences_and_domain_queries_round_trip() {
        let dir = tempdir().unwrap();
        let engine = Arc::new(SessionEngine::new());
        engine.configure_home(dir.path()).unwrap();
        let app = Application::with_engine(engine.clone());

        // Save a profile.
        let mut draft = ConnectionProfile::new_draft();
        draft.name = "Office HQ".into();
        draft.server = "gw.example.com".into();
        let save_resp = app
            .dispatch(AppRequest {
                id: 1,
                command: "profiles.save".into(),
                payload: json!({ "profile": draft }),
            })
            .await;
        assert!(save_resp.ok, "{:?}", save_resp.error);
        let saved_id = save_resp.data.unwrap()["id"].as_str().unwrap().to_owned();

        // Toggle favorite.
        let fav_resp = app
            .dispatch(AppRequest {
                id: 2,
                command: "profiles.toggleFavorite".into(),
                payload: json!({ "id": saved_id }),
            })
            .await;
        assert!(fav_resp.ok);
        assert_eq!(fav_resp.data.unwrap()["favorite"], true);

        // Update preferences.
        let pref_resp = app
            .dispatch(AppRequest {
                id: 3,
                command: "preferences.set".into(),
                payload: json!({ "language": "en", "theme": "dark" }),
            })
            .await;
        assert!(pref_resp.ok);
        assert_eq!(pref_resp.data.unwrap()["theme"], "dark");

        // Subscribe and query profiles domain.
        let sub_json = app
            .subscribe_state_json(r#"{"generation":1,"domains":[{"domain":"profiles"}]}"#)
            .unwrap();
        assert!(sub_json.contains("\"profiles\""));
        let query_json = app
            .query_state_json(r#"{"generation":1,"domain":"profiles"}"#)
            .await
            .unwrap();
        let query_val: Value = serde_json::from_str(&query_json).unwrap();
        assert_eq!(query_val["data"]["profiles"].as_array().unwrap().len(), 1);
        assert_eq!(query_val["data"]["profiles"][0]["favorite"], true);

        // Connect validates synchronously and delegates async auth/startup via PlatformEffect::connect.
        let connect_resp = app
            .dispatch(AppRequest {
                id: 4,
                command: "vpn.connect".into(),
                payload: json!({}),
            })
            .await;
        assert!(connect_resp.ok, "{:?}", connect_resp.error);
        assert_eq!(connect_resp.effect.unwrap().kind, "connect");

        // Disconnect returns stop effect.
        let stop_resp = app
            .dispatch(AppRequest {
                id: 5,
                command: "vpn.disconnect".into(),
                payload: json!({}),
            })
            .await;
        assert!(stop_resp.ok);
        assert_eq!(stop_resp.effect.unwrap().kind, "stop");
    }

    #[tokio::test]
    async fn errors_are_sanitized_before_leaving_application_layer() {
        let engine = Arc::new(SessionEngine::new());
        let app = Application::with_engine(engine);
        let resp = app
            .dispatch(AppRequest {
                id: 9,
                command: "vpn.unknownVpnCommand".into(),
                payload: json!({}),
            })
            .await;
        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(
            !err.to_ascii_lowercase().contains("vpn"),
            "leaked vpn in: {err}"
        );
    }
}
