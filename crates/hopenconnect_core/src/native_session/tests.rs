use super::*;

#[test]
fn browser_and_generated_token_fields_stay_protocol_owned() {
    assert!(protocol_owns_auth_value(anyconnect::FormOptionKind::Token));
    assert!(protocol_owns_auth_value(
        anyconnect::FormOptionKind::SsoToken
    ));
    assert!(protocol_owns_auth_value(
        anyconnect::FormOptionKind::SsoUser
    ));
    assert!(!protocol_owns_auth_value(
        anyconnect::FormOptionKind::Password
    ));
}

#[test]
fn failover_does_not_retry_user_authentication_failures() {
    assert!(is_failover_eligible(
        "failed to connect: network unreachable"
    ));
    assert!(is_failover_eligible("TLS handshake timed out"));
    assert!(!is_failover_eligible("authentication cancelled"));
    assert!(!is_failover_eligible("invalid password"));
}

#[test]
fn certificate_pin_list_preserves_base64_case() {
    assert_eq!(
        server_certificate_hashes("pin-sha256:AbCdEf+/=, sha256:0011\nsha1:AABB"),
        vec![
            "pin-sha256:AbCdEf+/=".to_owned(),
            "sha256:0011".to_owned(),
            "sha1:AABB".to_owned()
        ]
    );
}

#[test]
fn certificate_policy_uses_exactly_one_intended_trust_source() {
    assert!(should_use_system_trust("", false));
    assert!(!should_use_system_trust("", true));
    assert!(!should_use_system_trust("sha256:0011", false));
    assert!(!should_use_system_trust("pin-sha256:AbCdEf+/=", true));
}

#[test]
fn default_protocol_identity_matches_openharmony() {
    let profile = ConnectionProfile::new_draft();

    assert_eq!(
        effective_user_agent(&profile),
        format!("AnyConnect OpenHarmony {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        effective_client_version(&profile),
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        openconnect_reported_os(effective_reported_os(&profile)),
        "OpenHarmony"
    );
}

#[test]
fn configured_openharmony_identity_passes_through_to_openconnect() {
    let mut profile = ConnectionProfile::new_draft();
    profile.reported_os = "OpenHarmony".to_owned();

    assert_eq!(effective_reported_os(&profile), "OpenHarmony");
    assert_eq!(
        openconnect_reported_os(effective_reported_os(&profile)),
        "OpenHarmony"
    );
}

#[test]
fn configured_auth_group_prefers_protocol_value_and_migrates_saved_label() {
    let choices = vec![
        AuthFieldChoice {
            name: "password".to_owned(),
            label: "密码认证".to_owned(),
        },
        AuthFieldChoice {
            name: "sms".to_owned(),
            label: "短信认证".to_owned(),
        },
    ];

    assert_eq!(configured_auth_group_index("sms", &choices), Some(1));
    assert_eq!(configured_auth_group_index("短信认证", &choices), Some(1));
    assert_eq!(configured_auth_group_index("", &choices), None);
}

#[test]
fn configured_auth_group_never_falls_back_to_another_choice() {
    let choices = vec![AuthFieldChoice {
        name: "sms".to_owned(),
        label: "短信认证".to_owned(),
    }];
    assert_eq!(configured_auth_group_index("不存在", &choices), None);
}

#[test]
fn unchanged_authentication_group_does_not_refresh_the_form() {
    let mut unchanged = AuthFormState::default();
    assert!(!unchanged.take_auth_group_refresh(false));
    assert!(!unchanged.take_auth_group_refresh(true));
}

#[test]
fn changed_authentication_group_refreshes_exactly_once() {
    let mut changed = AuthFormState::default();
    assert!(changed.take_auth_group_refresh(true));
    assert!(!changed.take_auth_group_refresh(true));
    assert!(!changed.take_auth_group_refresh(false));
}

#[test]
fn automatic_main_form_is_guarded_across_an_intervening_challenge() {
    let mut state = AuthFormState::default();
    let main = AuthFormFingerprint::from_fields(
        Some("main".to_owned()),
        &[AuthField {
            key: crate::model::AuthFieldKey {
                form_id: Some("main".to_owned()),
                option_index: 1,
                option_digest: "password".to_owned(),
            },
            name: "password".to_owned(),
            label: "Password".to_owned(),
            kind: AuthFieldKind::Password,
            ..AuthField::default()
        }],
    );
    let challenge = AuthFormFingerprint::from_fields(
        Some("challenge".to_owned()),
        &[AuthField {
            key: crate::model::AuthFieldKey {
                form_id: Some("challenge".to_owned()),
                option_index: 0,
                option_digest: "hidden-trigger".to_owned(),
            },
            kind: AuthFieldKind::Hidden,
            ..AuthField::default()
        }],
    );

    assert!(!state.was_automatically_submitted(&main));
    state.record_submission(main.clone(), true);
    state.record_submission(challenge, true);
    assert!(state.was_automatically_submitted(&main));
}

#[test]
fn hidden_follow_up_is_remembered_until_a_visible_follow_up_arrives() {
    let mut state = AuthFormState::default();

    state.record_follow_up_submission(AuthFormRole::Challenge, false, Some("next_tokencode"));
    assert_eq!(
        state.hidden_follow_up_without_input(),
        Some("next_tokencode")
    );

    state.record_follow_up_submission(AuthFormRole::Challenge, true, Some("challenge"));
    assert_eq!(state.hidden_follow_up_without_input(), None);
}

#[test]
fn repeated_form_fingerprint_ignores_echoed_credential_values() {
    let mut first = AuthField {
        name: "password".to_owned(),
        label: "Password".to_owned(),
        kind: AuthFieldKind::Password,
        value: "first-secret".to_owned(),
        ..AuthField::default()
    };
    let first_fingerprint =
        AuthFormFingerprint::from_fields(Some("main".to_owned()), std::slice::from_ref(&first));
    first.value = "echoed-or-different-secret".to_owned();
    let repeated_fingerprint = AuthFormFingerprint::from_fields(Some("main".to_owned()), &[first]);

    assert_eq!(first_fingerprint, repeated_fingerprint);
}

#[test]
fn form_fingerprint_includes_server_form_id() {
    let field = AuthField {
        key: crate::model::AuthFieldKey {
            option_index: 0,
            option_digest: "same-option".to_owned(),
            ..crate::model::AuthFieldKey::default()
        },
        ..AuthField::default()
    };

    assert_ne!(
        AuthFormFingerprint::from_fields(Some("main".to_owned()), std::slice::from_ref(&field)),
        AuthFormFingerprint::from_fields(Some("challenge".to_owned()), &[field])
    );
}

#[test]
fn option_digest_is_value_free_but_sensitive_to_structure() {
    let first = auth_option_digest("password", "Password", AuthFieldKind::Password, &[], false);
    let same = auth_option_digest("password", "Password", AuthFieldKind::Password, &[], false);
    let challenge = auth_option_digest(
        "password",
        "Verification code",
        AuthFieldKind::Password,
        &[],
        false,
    );

    assert_eq!(first, same);
    assert_ne!(first, challenge);
}

#[test]
fn third_consecutive_empty_authentication_form_is_rejected() {
    let mut state = AuthFormState::default();

    assert!(state.accept_form(false));
    assert!(state.accept_form(false));
    assert!(!state.accept_form(false));
    assert!(state.accept_form(true));
    assert!(state.accept_form(false));
}
