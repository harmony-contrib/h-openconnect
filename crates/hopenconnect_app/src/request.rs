//! Strongly typed command requests, responses, and host platform effects.

use crate::AppError;
use hopenconnect_core::{AuthFieldValue, ConnectionProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRequest {
    pub id: u64,
    pub command: String,
    pub payload: Value,
}

impl AppRequest {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.id > 9_007_199_254_740_991 || !self.payload.is_object() {
            return Err(AppError::InvalidRequest(
                "request requires a safe integer id and an object payload".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<PlatformEffect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformEffect {
    pub kind: String,
    pub options_json: String,
}

impl PlatformEffect {
    pub(crate) fn connect() -> Self {
        Self {
            kind: "connect".into(),
            options_json: String::new(),
        }
    }

    pub(crate) fn stop() -> Self {
        Self {
            kind: "stop".into(),
            options_json: String::new(),
        }
    }
}

macro_rules! fields {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub(crate) struct $name { $(pub $field: $ty),* }
    };
}

fields!(IdPayload { id: String });
fields!(ProfilePayload {
    profile: ConnectionProfile
});
fields!(ForceGlobalPayload { force_global: bool });
fields!(EnabledPayload { enabled: bool });
fields!(ArchivePayload { file_name: String });
fields!(QrImportPayload { content: String });
fields!(AuthSubmitPayload {
    id: u64,
    values: Vec<AuthFieldValue>
});

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreferencesPatch {
    pub language: Option<String>,
    pub theme: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_envelope() {
        let bad = AppRequest {
            id: 1,
            command: "profiles.select".into(),
            payload: Value::Null,
        };
        assert!(bad.validate().is_err());

        let good = AppRequest {
            id: 1,
            command: "profiles.select".into(),
            payload: serde_json::json!({ "id": "p1" }),
        };
        assert!(good.validate().is_ok());
    }
}
