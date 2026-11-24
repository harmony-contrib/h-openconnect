//! Display text sanitization ensuring user-visible errors, auth prompts, and
//! logs never leak restricted product vocabulary.

use hopenconnect_core::{
    AuthChallenge, AuthField, AuthFieldChoice, AuthGroupDiscovery, DiagnosticEntry,
};

/// Keep app-generated and remote diagnostic text aligned with the product UI
/// vocabulary before it reaches a visible error, toast, challenge, or log row.
pub fn sanitize_display_text(text: &str) -> String {
    let normalized = text.to_ascii_lowercase();
    if !normalized.contains("vpn") {
        return text.to_owned();
    }

    let mut sanitized = String::with_capacity(text.len() + 16);
    let mut copied_until = 0;
    for (start, _) in normalized.match_indices("vpn") {
        sanitized.push_str(&text[copied_until..start]);
        sanitized.push_str("connection");
        copied_until = start + 3;
    }
    sanitized.push_str(&text[copied_until..]);
    sanitized
}

pub fn sanitize_optional(text: Option<&str>) -> Option<String> {
    text.map(sanitize_display_text)
}

pub fn sanitize_auth_challenge(challenge: &AuthChallenge) -> AuthChallenge {
    AuthChallenge {
        id: challenge.id,
        round: challenge.round,
        banner: challenge.banner.as_deref().map(sanitize_display_text),
        message: challenge.message.as_deref().map(sanitize_display_text),
        error: challenge.error.as_deref().map(sanitize_display_text),
        form_id: challenge.form_id.clone(),
        method: challenge.method.clone(),
        fields: challenge
            .fields
            .iter()
            .map(|field| AuthField {
                key: field.key.clone(),
                name: field.name.clone(),
                label: sanitize_display_text(&field.label),
                kind: field.kind,
                value: field.value.clone(),
                choices: field
                    .choices
                    .iter()
                    .map(|choice| AuthFieldChoice {
                        name: choice.name.clone(),
                        label: sanitize_display_text(&choice.label),
                    })
                    .collect(),
                auth_group: field.auth_group,
                second_auth: field.second_auth,
                required: field.required,
            })
            .collect(),
    }
}

pub fn sanitize_group_discovery(discovery: AuthGroupDiscovery) -> AuthGroupDiscovery {
    AuthGroupDiscovery {
        selected: discovery.selected,
        groups: discovery
            .groups
            .into_iter()
            .map(|choice| AuthFieldChoice {
                name: choice.name,
                label: sanitize_display_text(&choice.label),
            })
            .collect(),
    }
}

pub fn sanitize_diagnostic_entry(entry: &DiagnosticEntry) -> DiagnosticEntry {
    DiagnosticEntry {
        level: entry.level.clone(),
        message: sanitize_display_text(&entry.message),
        timestamp: entry.timestamp.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_vpn_case_insensitively() {
        assert_eq!(
            sanitize_display_text("VPN service failed: vpn extension timeout"),
            "connection service failed: connection extension timeout"
        );
        assert_eq!(
            sanitize_display_text("connected cleanly"),
            "connected cleanly"
        );
    }

    #[test]
    fn sanitizes_auth_challenge_visible_labels_without_touching_protocol_keys() {
        let challenge = AuthChallenge {
            id: 7,
            round: 2,
            banner: Some("Corp VPN Gateway".into()),
            message: Some("Enter your VPN token".into()),
            error: Some("VPN rejected token".into()),
            form_id: Some("vpn_form".into()),
            method: Some("POST".into()),
            fields: vec![AuthField {
                name: "vpn_otp".into(),
                label: "VPN One-Time Code".into(),
                choices: vec![AuthFieldChoice {
                    name: "vpn_group_1".into(),
                    label: "HQ VPN".into(),
                }],
                ..AuthField::default()
            }],
        };
        let sanitized = sanitize_auth_challenge(&challenge);
        assert_eq!(sanitized.banner.as_deref(), Some("Corp connection Gateway"));
        assert_eq!(
            sanitized.message.as_deref(),
            Some("Enter your connection token")
        );
        assert_eq!(
            sanitized.error.as_deref(),
            Some("connection rejected token")
        );
        assert_eq!(sanitized.form_id.as_deref(), Some("vpn_form"));
        assert_eq!(sanitized.fields[0].name, "vpn_otp");
        assert_eq!(sanitized.fields[0].label, "connection One-Time Code");
        assert_eq!(sanitized.fields[0].choices[0].name, "vpn_group_1");
        assert_eq!(sanitized.fields[0].choices[0].label, "HQ connection");
    }
}
