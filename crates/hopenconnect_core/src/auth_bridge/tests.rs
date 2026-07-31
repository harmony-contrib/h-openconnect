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
