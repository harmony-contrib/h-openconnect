//! Interactive authentication bridge between the OpenConnect form callback
//! (blocking worker thread) and the UI event loop.
//!
//! Flow:
//! 1. Worker builds an [`AuthChallenge`] and calls [`AuthInteraction::wait_for_reply`].
//! 2. UI reads it via [`AuthInteraction::pending`] / session snapshot.
//! 3. UI calls [`AuthInteraction::submit`] or [`AuthInteraction::cancel`].
//! 4. Worker unblocks, applies values, returns `Submit` / `Cancelled` to OpenConnect.

use crate::error::{CoreError, CoreResult};
use crate::model::{AuthChallenge, AuthChallengeReply, AuthField, AuthFieldKind, AuthFieldValue};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared credentials used for auto-fill before prompting the UI.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
    pub group: String,
}

#[derive(Debug)]
struct PendingSlot {
    challenge: AuthChallenge,
    reply_tx: Sender<AuthChallengeReply>,
}

#[derive(Debug, Default)]
struct Inner {
    next_id: u64,
    round: u32,
    pending: Option<PendingSlot>,
    /// Global cancel (disconnect) — any waiter returns cancelled.
    aborted: bool,
}

/// Cross-thread auth interaction handle (one per [`crate::SessionEngine`]).
#[derive(Debug, Default)]
pub struct AuthInteraction {
    inner: Mutex<Inner>,
}

impl AuthInteraction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Snapshot of the form currently waiting on the UI, if any.
    pub fn pending(&self) -> Option<AuthChallenge> {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.pending.as_ref().map(|p| p.challenge.clone()))
    }

    /// Clear pending state and abort any waiter (used on disconnect / failed connect).
    pub fn abort(&self) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.aborted = true;
        if let Some(pending) = guard.pending.take() {
            let _ = pending.reply_tx.send(AuthChallengeReply {
                id: pending.challenge.id,
                values: Vec::new(),
                cancelled: true,
            });
        }
    }

    /// Reset abort flag at the start of a new connect attempt.
    pub fn begin_session(&self) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.aborted = false;
        guard.round = 0;
        if let Some(pending) = guard.pending.take() {
            let _ = pending.reply_tx.send(AuthChallengeReply {
                id: pending.challenge.id,
                values: Vec::new(),
                cancelled: true,
            });
        }
    }

    /// Submit a UI response for the current challenge.
    pub fn submit(&self, reply: AuthChallengeReply) -> CoreResult<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| CoreError::msg("auth interaction lock poisoned"))?;
        let Some(pending) = guard.pending.take() else {
            return Err(CoreError::msg("no pending authentication challenge"));
        };
        if reply.id != 0 && reply.id != pending.challenge.id {
            // Put it back if ids don't match (stale UI).
            guard.pending = Some(pending);
            return Err(CoreError::msg("stale authentication challenge id"));
        }
        let mut reply = reply;
        reply.id = pending.challenge.id;
        pending
            .reply_tx
            .send(reply)
            .map_err(|_| CoreError::msg("authentication worker is no longer waiting"))?;
        Ok(())
    }

    pub fn cancel_pending(&self) -> CoreResult<()> {
        let pending = self.pending();
        let id = pending.map(|c| c.id).unwrap_or(0);
        self.submit(AuthChallengeReply {
            id,
            values: Vec::new(),
            cancelled: true,
        })
    }

    /// Allocate id/round and block until the UI replies (or abort).
    pub fn wait_for_reply(&self, mut challenge: AuthChallenge) -> AuthChallengeReply {
        let (tx, rx): (Sender<AuthChallengeReply>, Receiver<AuthChallengeReply>) = mpsc::channel();
        {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            if guard.aborted {
                return AuthChallengeReply {
                    id: 0,
                    values: Vec::new(),
                    cancelled: true,
                };
            }
            // Replace any prior pending (should not happen for a single worker).
            if let Some(old) = guard.pending.take() {
                let _ = old.reply_tx.send(AuthChallengeReply {
                    id: old.challenge.id,
                    values: Vec::new(),
                    cancelled: true,
                });
            }
            guard.next_id = guard.next_id.saturating_add(1);
            guard.round = guard.round.saturating_add(1);
            challenge.id = guard.next_id;
            challenge.round = guard.round;
            guard.pending = Some(PendingSlot {
                challenge: challenge.clone(),
                reply_tx: tx,
            });
        }

        // Poll so abort from another thread is observed even if UI never replies.
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(reply) => {
                    // Ensure pending slot is cleared if submit raced.
                    if let Ok(mut guard) = self.inner.lock() {
                        if guard
                            .pending
                            .as_ref()
                            .map(|p| p.challenge.id == reply.id)
                            .unwrap_or(false)
                        {
                            guard.pending = None;
                        }
                    }
                    return reply;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let aborted = self.inner.lock().map(|g| g.aborted).unwrap_or(true);
                    if aborted {
                        if let Ok(mut guard) = self.inner.lock() {
                            guard.pending = None;
                        }
                        return AuthChallengeReply {
                            id: challenge.id,
                            values: Vec::new(),
                            cancelled: true,
                        };
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Ok(mut guard) = self.inner.lock() {
                        guard.pending = None;
                    }
                    return AuthChallengeReply {
                        id: challenge.id,
                        values: Vec::new(),
                        cancelled: true,
                    };
                }
            }
        }
    }
}

/// Prefill known fields from the connection profile.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn apply_credentials_to_fields(fields: &mut [AuthField], creds: &AuthCredentials) {
    for field in fields.iter_mut() {
        let name = field.name.to_ascii_lowercase();
        let label = field.label.to_ascii_lowercase();
        // Primary password only — never treat secondary_password / OTP as the
        // profile password (would silently submit wrong value for MFA).
        let is_primary_password = matches!(field.kind, AuthFieldKind::Password)
            && matches!(
                name.as_str(),
                "password" | "passwd" | "pass" | "pwd" | "user_password"
            )
            && !name.contains("secondary")
            && !name.contains("otp")
            && !name.contains("token")
            && !name.contains("sms")
            && !label.contains("otp")
            && !label.contains("短信")
            && !label.contains("验证码");
        let is_username = matches!(field.kind, AuthFieldKind::Text)
            && (matches!(
                name.as_str(),
                "username" | "user" | "uname" | "login" | "userid" | "user_name"
            ) || label.contains("用户")
                || label.contains("账号"));
        let is_group = field.auth_group
            || (matches!(field.kind, AuthFieldKind::Text | AuthFieldKind::Select)
                && matches!(
                    name.as_str(),
                    "group_list" | "group" | "auth_group" | "group_list[]" | "grouplist"
                ));

        if is_username && field.value.is_empty() && !creds.username.is_empty() {
            field.value = creds.username.clone();
            field.required = false;
        } else if is_primary_password && field.value.is_empty() && !creds.password.is_empty() {
            field.value = creds.password.clone();
            field.required = false;
        } else if is_group && field.value.is_empty() && !creds.group.is_empty() {
            // Prefer exact choice name when select options are present.
            if !field.choices.is_empty() {
                let want = creds.group.as_str();
                if let Some(choice) = field.choices.iter().find(|c| {
                    c.name.eq_ignore_ascii_case(want) || c.label.eq_ignore_ascii_case(want)
                }) {
                    field.value = choice.name.clone();
                } else {
                    field.value = creds.group.clone();
                }
            } else {
                field.value = creds.group.clone();
            }
            field.required = false;
        } else if matches!(field.kind, AuthFieldKind::Hidden) {
            field.required = false;
        } else if field.value.trim().is_empty()
            && !matches!(field.kind, AuthFieldKind::Hidden | AuthFieldKind::Unknown)
        {
            // Token / secondary password / SMS / OTP must go to the UI.
            field.required = true;
        }
    }
}

/// True when every interactive field already has a non-empty value.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn can_autofill_without_ui(fields: &[AuthField]) -> bool {
    let interactive: Vec<&AuthField> = fields
        .iter()
        .filter(|field| !matches!(field.kind, AuthFieldKind::Hidden | AuthFieldKind::Unknown))
        .collect();
    // Banner-only / empty forms can be submitted without UI.
    if interactive.is_empty() {
        return true;
    }
    interactive
        .iter()
        .all(|field| !field.value.trim().is_empty())
}

/// Merge reply values over the challenge field list (for applying to OpenConnect).
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn merge_reply_values(
    fields: &[AuthField],
    reply: &AuthChallengeReply,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = fields
        .iter()
        .filter(|f| !f.value.is_empty())
        .map(|f| (f.name.clone(), f.value.clone()))
        .collect();
    for AuthFieldValue { name, value } in &reply.values {
        map.insert(name.clone(), value.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn wait_and_submit_roundtrip() {
        let bridge = AuthInteraction::shared();
        bridge.begin_session();
        let bridge_worker = Arc::clone(&bridge);
        let worker = thread::spawn(move || {
            bridge_worker.wait_for_reply(AuthChallenge {
                id: 0,
                round: 0,
                banner: None,
                message: Some("Enter OTP".to_owned()),
                error: None,
                form_id: None,
                method: None,
                fields: vec![AuthField {
                    name: "secondary_password".to_owned(),
                    label: "OTP".to_owned(),
                    kind: AuthFieldKind::Password,
                    value: String::new(),
                    choices: Vec::new(),
                    auth_group: false,
                    required: true,
                }],
            })
        });

        // Spin until pending is visible.
        let challenge = (0..50)
            .find_map(|_| {
                thread::sleep(Duration::from_millis(10));
                bridge.pending()
            })
            .expect("pending challenge");
        assert_eq!(challenge.message.as_deref(), Some("Enter OTP"));
        bridge
            .submit(AuthChallengeReply {
                id: challenge.id,
                values: vec![AuthFieldValue {
                    name: "secondary_password".to_owned(),
                    value: "123456".to_owned(),
                }],
                cancelled: false,
            })
            .unwrap();
        let reply = worker.join().unwrap();
        assert!(!reply.cancelled);
        assert_eq!(reply.values[0].value, "123456");
    }

    #[test]
    fn abort_cancels_waiter() {
        let bridge = AuthInteraction::shared();
        bridge.begin_session();
        let bridge_worker = Arc::clone(&bridge);
        let worker = thread::spawn(move || {
            bridge_worker.wait_for_reply(AuthChallenge {
                fields: vec![AuthField {
                    name: "password".to_owned(),
                    label: "Password".to_owned(),
                    kind: AuthFieldKind::Password,
                    required: true,
                    ..AuthField::default()
                }],
                ..AuthChallenge::default()
            })
        });
        thread::sleep(Duration::from_millis(30));
        bridge.abort();
        let reply = worker.join().unwrap();
        assert!(reply.cancelled);
    }

    #[test]
    fn credentials_autofill_username_password() {
        let mut fields = vec![
            AuthField {
                name: "username".to_owned(),
                label: "User".to_owned(),
                kind: AuthFieldKind::Text,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "password".to_owned(),
                label: "Pass".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
        ];
        apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials {
                username: "demo".to_owned(),
                password: "demo".to_owned(),
                group: String::new(),
            },
        );
        assert!(can_autofill_without_ui(&fields));
    }

    #[test]
    fn otp_requires_ui() {
        let mut fields = vec![
            AuthField {
                name: "username".to_owned(),
                kind: AuthFieldKind::Text,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "password".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "secondary_password".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
        ];
        apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials {
                username: "demo".to_owned(),
                password: "demo".to_owned(),
                group: String::new(),
            },
        );
        assert!(!can_autofill_without_ui(&fields));
    }
}
