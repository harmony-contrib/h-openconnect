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
mod tests;
