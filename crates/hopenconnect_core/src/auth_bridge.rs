//! Interactive authentication bridge between the OpenConnect form callback
//! (blocking worker thread) and the UI event loop.
//!
//! Flow:
//! 1. Worker builds an [`AuthChallenge`] and calls [`AuthInteraction::wait_for_reply`].
//! 2. UI reads it via [`AuthInteraction::pending`] / session snapshot.
//! 3. UI calls [`AuthInteraction::submit`] or [`AuthInteraction::cancel`].
//! 4. Worker unblocks, applies values, returns `Submit` / `Cancelled` to OpenConnect.

use crate::error::{CoreError, CoreResult};
use crate::model::{AuthChallenge, AuthChallengeReply, AuthField, AuthFieldKind};
use std::collections::HashSet;
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

/// Match OpenConnect CLI's configured-username rule.
///
/// `process_auth_form_cb()` accepts text option names beginning with `user` or
/// `uname`. Labels are never used to infer identity semantics.
fn accepts_configured_username(field: &AuthField) -> bool {
    let name = field.name.to_ascii_lowercase();
    matches!(field.kind, AuthFieldKind::Text)
        && !field.second_auth
        && (name.starts_with("user") || name.starts_with("uname"))
}

/// Result of applying immutable profile credentials to one server form.
///
/// Indices are used instead of field names because a broken headend may reuse
/// the same name for primary and secondary fields in a single form.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CredentialApplication {
    /// Values supplied from the profile and therefore hidden from the prompt.
    profile_owned: HashSet<usize>,
    /// Visible options explicitly owned by a server challenge.
    user_owned: HashSet<usize>,
}

/// AnyConnect protocol role of one server authentication form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // constructed from native_session when `native-anyconnect` is on
pub(crate) enum AuthFormRole {
    /// Primary login form (`main` for AnyConnect).
    Primary,
    /// Any non-`main` follow-up form reported by OpenConnect.
    Challenge,
}

impl AuthFormRole {
    /// Classify by the AnyConnect form identity, not by option names or labels.
    ///
    /// `main` is the protocol's primary-login form. Other named forms include
    /// `challenge`, `next_tokencode`, and vendor-defined follow-up forms. They
    /// all have the same UI rule: collect only the visible options actually
    /// supplied by the server. A missing id falls back to primary so malformed
    /// forms can never consume the configured password as an inferred OTP.
    #[allow(dead_code)] // used from native_session when `native-anyconnect` is on
    pub(crate) fn for_anyconnect(form_id: Option<&str>) -> Self {
        match form_id {
            Some("main") | None => Self::Primary,
            Some(_) => Self::Challenge,
        }
    }
}

/// Apply configured primary credentials using OpenConnect's form semantics.
///
/// Primary credentials are immutable session inputs and are applied on every
/// primary form. A follow-up form never consumes or inherits them: every
/// visible option is returned to the UI exactly as declared by the server.
/// A primary form may also contain options carrying OpenConnect's explicit
/// `OC_FORM_OPT_SECOND_AUTH` flag; those exact visible options are returned to
/// the UI without interpreting their names or labels as OTP/SMS metadata.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn apply_credentials_to_fields(
    fields: &mut [AuthField],
    creds: &AuthCredentials,
    role: AuthFormRole,
) -> CredentialApplication {
    let mut applied = CredentialApplication::default();
    let mut password_available =
        matches!(role, AuthFormRole::Primary) && !creds.password.is_empty();

    for (index, field) in fields.iter_mut().enumerate() {
        if matches!(field.kind, AuthFieldKind::Hidden) {
            field.required = false;
        } else if matches!(role, AuthFormRole::Challenge) || field.second_auth {
            field.required = field.value.trim().is_empty();
            applied.user_owned.insert(index);
        } else if accepts_configured_username(field) && !creds.username.is_empty() {
            field.value = creds.username.clone();
            field.required = false;
            applied.profile_owned.insert(index);
        } else if password_available
            && !field.second_auth
            && matches!(field.kind, AuthFieldKind::Password)
        {
            // Consume the configured password at the first protocol-designated
            // primary PASSWORD option. Do not bind it to a guessed name.
            field.value = creds.password.clone();
            field.required = false;
            applied.profile_owned.insert(index);
            password_available = false;
        } else if field.value.trim().is_empty() && !matches!(field.kind, AuthFieldKind::Hidden) {
            // Only a follow-up form or an explicit SECOND_AUTH protocol flag
            // can transfer option ownership to the UI. Other unresolved
            // primary options are never guessed to be an OTP.
            field.required = true;
        }
    }
    applied
}

/// Clone only fields whose values must be collected from the current user.
///
/// Only visible options from a server follow-up form or options carrying the
/// explicit SECOND_AUTH protocol flag are eligible. Ordinary primary-login
/// options are never converted into an OTP prompt.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn fields_for_user_input(
    fields: &[AuthField],
    applied: &CredentialApplication,
) -> Vec<AuthField> {
    fields
        .iter()
        .enumerate()
        .filter(|(index, _)| applied.user_owned.contains(index))
        .map(|(_, field)| field.clone())
        .collect()
}

/// True when every interactive field already has a non-empty value.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn can_autofill_without_ui(fields: &[AuthField]) -> bool {
    let interactive: Vec<&AuthField> = fields
        .iter()
        .filter(|field| !matches!(field.kind, AuthFieldKind::Hidden))
        .collect();
    // Banner-only / empty forms can be submitted without UI.
    if interactive.is_empty() {
        return true;
    }
    interactive
        .iter()
        .all(|field| !field.value.trim().is_empty())
}

/// Bind an ordered reply to the exact options that produced it.
///
/// The result stays aligned with `fields`; no field-name map is built. Reply
/// values must be an in-order subset of the original option sequence, and
/// every key must match form id, raw option ordinal, and structural digest.
#[allow(dead_code)] // used from native_session when `native-anyconnect` is on
pub fn bind_reply_values_by_option(
    fields: &[AuthField],
    reply: &AuthChallengeReply,
) -> CoreResult<Vec<Option<String>>> {
    let mut values = fields
        .iter()
        .map(|field| (!field.value.is_empty()).then(|| field.value.clone()))
        .collect::<Vec<_>>();
    let mut reply_values = reply.values.iter().peekable();

    for (index, field) in fields.iter().enumerate() {
        let Some(answer) = reply_values.peek() else {
            break;
        };
        if answer.key == field.key {
            values[index] = Some(answer.value.clone());
            reply_values.next();
        }
    }

    if reply_values.next().is_some() {
        return Err(CoreError::msg(
            "authentication reply contains a stale or out-of-order option",
        ));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthFieldKey, AuthFieldValue};
    use std::thread;

    fn field_key(form_id: &str, option_index: u32, option_digest: &str) -> AuthFieldKey {
        AuthFieldKey {
            form_id: Some(form_id.to_owned()),
            option_index,
            option_digest: option_digest.to_owned(),
        }
    }

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
                    key: field_key("challenge", 0, "otp"),
                    name: "secondary_password".to_owned(),
                    label: "OTP".to_owned(),
                    kind: AuthFieldKind::Password,
                    value: String::new(),
                    choices: Vec::new(),
                    auth_group: false,
                    second_auth: true,
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
                    key: challenge.fields[0].key.clone(),
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
        let creds = AuthCredentials {
            username: "demo".to_owned(),
            password: "demo".to_owned(),
            group: String::new(),
        };
        let applied = apply_credentials_to_fields(&mut fields, &creds, AuthFormRole::Primary);
        assert!(can_autofill_without_ui(&fields));
        assert_eq!(fields[0].value, "demo");
        assert_eq!(fields[1].value, "demo");
        assert_eq!(applied.profile_owned, HashSet::from([0, 1]));
        assert!(fields_for_user_input(&fields, &applied).is_empty());
        assert_eq!(creds.password, "demo");
    }

    #[test]
    fn anyconnect_form_role_only_reserves_main_for_primary_login() {
        assert_eq!(
            AuthFormRole::for_anyconnect(Some("main")),
            AuthFormRole::Primary
        );
        assert_eq!(
            AuthFormRole::for_anyconnect(Some("challenge")),
            AuthFormRole::Challenge
        );
        assert_eq!(
            AuthFormRole::for_anyconnect(Some("next_tokencode")),
            AuthFormRole::Challenge
        );
        assert_eq!(
            AuthFormRole::for_anyconnect(Some("vendor-follow-up")),
            AuthFormRole::Challenge
        );
        assert_eq!(AuthFormRole::for_anyconnect(None), AuthFormRole::Primary);
    }

    #[test]
    fn primary_form_exposes_only_protocol_marked_second_auth_options() {
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
                second_auth: true,
                required: true,
                ..AuthField::default()
            },
        ];
        let applied = apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials {
                username: "demo".to_owned(),
                password: "demo".to_owned(),
                group: String::new(),
            },
            AuthFormRole::Primary,
        );
        assert!(!can_autofill_without_ui(&fields));
        let user_fields = fields_for_user_input(&fields, &applied);
        assert_eq!(user_fields, vec![fields[2].clone()]);
        assert!(fields[2].required);
        assert_eq!(fields[0].value, "demo");
        assert_eq!(fields[1].value, "demo");
    }

    #[test]
    fn second_auth_password_never_consumes_primary_profile_password() {
        let mut fields = vec![AuthField {
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            second_auth: true,
            required: true,
            ..AuthField::default()
        }];

        let applied = apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials {
                username: "primary-user".to_owned(),
                password: "primary-secret".to_owned(),
                group: String::new(),
            },
            AuthFormRole::Primary,
        );

        assert!(fields[0].value.is_empty());
        assert!(fields[0].required);
        assert!(applied.profile_owned.is_empty());
        assert_eq!(fields_for_user_input(&fields, &applied), fields);
    }

    #[test]
    fn second_auth_username_is_server_owned_not_profile_owned() {
        let mut fields = vec![AuthField {
            name: "username".to_owned(),
            label: "Secondary username".to_owned(),
            kind: AuthFieldKind::Text,
            second_auth: true,
            required: true,
            ..AuthField::default()
        }];

        let applied = apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials {
                username: "primary-user".to_owned(),
                password: "primary-secret".to_owned(),
                group: String::new(),
            },
            AuthFormRole::Primary,
        );

        assert!(fields[0].value.is_empty());
        assert!(fields[0].required);
        assert_eq!(fields_for_user_input(&fields, &applied), fields);
        assert!(!can_autofill_without_ui(&fields));
    }

    #[test]
    fn primary_form_reapplies_profile_password_without_otp_inference() {
        let creds = AuthCredentials {
            username: "primary-user".to_owned(),
            password: "primary-secret".to_owned(),
            group: String::new(),
        };
        let mut fields = vec![
            AuthField {
                name: "username".to_owned(),
                label: "Username".to_owned(),
                kind: AuthFieldKind::Text,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "password".to_owned(),
                label: "Password".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "answer".to_owned(),
                label: "短信验证码".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
        ];

        let applied = apply_credentials_to_fields(&mut fields, &creds, AuthFormRole::Primary);
        let user_fields = fields_for_user_input(&fields, &applied);

        assert_eq!(fields[0].value, "primary-user");
        assert_eq!(fields[1].value, "primary-secret");
        assert!(fields[2].value.is_empty());
        assert_eq!(applied.profile_owned, HashSet::from([0, 1]));
        assert!(user_fields.is_empty());
        assert_eq!(creds.password, "primary-secret");
        assert!(!can_autofill_without_ui(&fields));
    }

    #[test]
    fn ambiguous_password_only_challenge_is_never_autofilled() {
        let creds = AuthCredentials {
            password: "primary-secret".to_owned(),
            ..AuthCredentials::default()
        };
        let mut fields = vec![AuthField {
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            required: true,
            ..AuthField::default()
        }];

        let applied = apply_credentials_to_fields(&mut fields, &creds, AuthFormRole::Challenge);
        let user_fields = fields_for_user_input(&fields, &applied);

        assert!(fields[0].value.is_empty());
        assert!(applied.profile_owned.is_empty());
        assert_eq!(user_fields.len(), 1);
        assert_eq!(user_fields[0].name, "password");
        assert_eq!(user_fields[0].kind, AuthFieldKind::Password);
        assert_eq!(creds.password, "primary-secret");
        assert!(!can_autofill_without_ui(&fields));
    }

    #[test]
    fn main_and_next_tokencode_password_options_never_share_values() {
        let creds = AuthCredentials {
            username: "primary-user".to_owned(),
            password: "primary-secret".to_owned(),
            group: String::new(),
        };
        let mut main = vec![AuthField {
            key: field_key("main", 1, "password"),
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            required: true,
            ..AuthField::default()
        }];
        let main_applied = apply_credentials_to_fields(
            &mut main,
            &creds,
            AuthFormRole::for_anyconnect(Some("main")),
        );
        assert_eq!(main[0].value, "primary-secret");
        assert!(fields_for_user_input(&main, &main_applied).is_empty());

        let mut next_token = vec![AuthField {
            key: field_key("next_tokencode", 1, "password"),
            // The same option name must not route the configured password.
            name: "password".to_owned(),
            label: "Next PASSCODE".to_owned(),
            kind: AuthFieldKind::Password,
            required: true,
            ..AuthField::default()
        }];
        let token_applied = apply_credentials_to_fields(
            &mut next_token,
            &creds,
            AuthFormRole::for_anyconnect(Some("next_tokencode")),
        );
        assert!(next_token[0].value.is_empty());
        assert_eq!(
            fields_for_user_input(&next_token, &token_applied),
            next_token
        );

        let reply = AuthChallengeReply {
            values: vec![AuthFieldValue {
                key: next_token[0].key.clone(),
                value: "654321".to_owned(),
            }],
            ..AuthChallengeReply::default()
        };
        assert_eq!(
            bind_reply_values_by_option(&next_token, &reply).unwrap(),
            vec![Some("654321".to_owned())]
        );
        assert_eq!(creds.password, "primary-secret");
    }

    #[test]
    fn primary_password_is_reused_on_each_primary_form() {
        let creds = AuthCredentials {
            username: "primary-user".to_owned(),
            password: "primary-secret".to_owned(),
            group: String::new(),
        };
        let mut login = vec![AuthField {
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            required: true,
            ..AuthField::default()
        }];
        let first = apply_credentials_to_fields(&mut login, &creds, AuthFormRole::Primary);
        assert_eq!(first.profile_owned, HashSet::from([0]));
        assert_eq!(login[0].value, "primary-secret");

        // A later primary form receives the same immutable login password. The
        // worker's form-fingerprint guard, not password consumption, prevents
        // automatic main/challenge/main loops.
        let mut challenge = vec![AuthField {
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            required: true,
            ..AuthField::default()
        }];
        let second = apply_credentials_to_fields(&mut challenge, &creds, AuthFormRole::Primary);
        let user_fields = fields_for_user_input(&challenge, &second);

        assert_eq!(second.profile_owned, HashSet::from([0]));
        assert_eq!(challenge[0].value, "primary-secret");
        assert!(user_fields.is_empty());
        assert!(can_autofill_without_ui(&challenge));
    }

    #[test]
    fn configured_password_uses_first_primary_password_option_not_its_name() {
        let creds = AuthCredentials {
            password: "primary-secret".to_owned(),
            ..AuthCredentials::default()
        };
        let mut fields = vec![
            AuthField {
                name: "credential".to_owned(),
                label: "Primary secret".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "answer".to_owned(),
                label: "Server response".to_owned(),
                kind: AuthFieldKind::Password,
                required: true,
                ..AuthField::default()
            },
        ];

        let applied = apply_credentials_to_fields(&mut fields, &creds, AuthFormRole::Primary);

        assert_eq!(fields[0].value, "primary-secret");
        assert!(fields[1].value.is_empty());
        assert_eq!(applied.profile_owned, HashSet::from([0]));
        assert!(fields_for_user_input(&fields, &applied).is_empty());
    }

    #[test]
    fn labels_do_not_change_username_or_password_classification() {
        let creds = AuthCredentials {
            username: "primary-user".to_owned(),
            password: "primary-secret".to_owned(),
            ..AuthCredentials::default()
        };
        let mut fields = vec![
            AuthField {
                name: "account".to_owned(),
                label: "用户名".to_owned(),
                kind: AuthFieldKind::Text,
                required: true,
                ..AuthField::default()
            },
            AuthField {
                name: "otp".to_owned(),
                label: "Password".to_owned(),
                kind: AuthFieldKind::Text,
                required: true,
                ..AuthField::default()
            },
        ];

        let applied = apply_credentials_to_fields(&mut fields, &creds, AuthFormRole::Primary);

        assert!(fields.iter().all(|field| field.value.is_empty()));
        assert!(applied.profile_owned.is_empty());
        assert!(applied.profile_owned.is_empty());
    }

    #[test]
    fn unknown_one_time_code_field_still_requires_the_ui() {
        let mut fields = vec![AuthField {
            name: "answer".to_owned(),
            label: "Verification code".to_owned(),
            kind: AuthFieldKind::Unknown,
            ..AuthField::default()
        }];

        let applied = apply_credentials_to_fields(
            &mut fields,
            &AuthCredentials::default(),
            AuthFormRole::Challenge,
        );

        assert!(fields[0].required);
        assert_eq!(fields_for_user_input(&fields, &applied), fields);
        assert!(!can_autofill_without_ui(&fields));
    }

    #[test]
    fn duplicate_option_names_bind_by_key_and_original_order() {
        let fields = vec![
            AuthField {
                key: field_key("challenge", 2, "first"),
                name: "password".to_owned(),
                kind: AuthFieldKind::Password,
                ..AuthField::default()
            },
            AuthField {
                key: field_key("challenge", 4, "second"),
                name: "password".to_owned(),
                kind: AuthFieldKind::Password,
                ..AuthField::default()
            },
        ];
        let reply = AuthChallengeReply {
            values: vec![
                AuthFieldValue {
                    key: fields[0].key.clone(),
                    value: "login-secret".to_owned(),
                },
                AuthFieldValue {
                    key: fields[1].key.clone(),
                    value: "123456".to_owned(),
                },
            ],
            ..AuthChallengeReply::default()
        };

        assert_eq!(
            bind_reply_values_by_option(&fields, &reply).unwrap(),
            vec![Some("login-secret".to_owned()), Some("123456".to_owned())]
        );
    }

    #[test]
    fn stale_form_key_is_rejected_instead_of_falling_back_to_name() {
        let field = AuthField {
            key: field_key("challenge", 1, "answer"),
            name: "password".to_owned(),
            kind: AuthFieldKind::Password,
            ..AuthField::default()
        };
        let reply = AuthChallengeReply {
            values: vec![AuthFieldValue {
                key: field_key("main", 1, "answer"),
                value: "123456".to_owned(),
            }],
            ..AuthChallengeReply::default()
        };

        let error = bind_reply_values_by_option(&[field], &reply).unwrap_err();
        assert!(error.to_string().contains("stale or out-of-order"));
    }

    #[test]
    fn out_of_order_reply_is_rejected() {
        let fields = vec![
            AuthField {
                key: field_key("challenge", 0, "first"),
                ..AuthField::default()
            },
            AuthField {
                key: field_key("challenge", 1, "second"),
                ..AuthField::default()
            },
        ];
        let reply = AuthChallengeReply {
            values: vec![
                AuthFieldValue {
                    key: fields[1].key.clone(),
                    value: "second".to_owned(),
                },
                AuthFieldValue {
                    key: fields[0].key.clone(),
                    value: "first".to_owned(),
                },
            ],
            ..AuthChallengeReply::default()
        };

        assert!(bind_reply_values_by_option(&fields, &reply).is_err());
    }
}
