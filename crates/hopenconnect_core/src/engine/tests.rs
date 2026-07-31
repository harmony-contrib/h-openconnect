use super::*;
use crate::model::ProtocolKind;
use tempfile::tempdir;

#[tokio::test]
async fn dry_run_prepare_connect_succeeds() {
    let engine = SessionEngine::new();
    let dir = tempdir().unwrap();
    engine.configure_home(dir.path()).unwrap();
    let mut profile = ConnectionProfile::new_draft();
    profile.id = "t1".to_owned();
    profile.name = "Test".to_owned();
    profile.server = "vpn.example.com".to_owned();
    profile.protocol = ProtocolKind::Ssl;
    let options = engine
        .prepare_connect(ConnectRequest {
            profile,
            dry_run: true,
        })
        .await
        .unwrap();
    assert!(!options.addresses.is_empty());
    let snap = engine.snapshot().unwrap();
    assert_eq!(snap.lifecycle, ConnectionLifecycle::Establishing);
}

#[tokio::test]
async fn dry_run_invalid_server_fails() {
    let engine = SessionEngine::new();
    let dir = tempdir().unwrap();
    engine.configure_home(dir.path()).unwrap();
    let mut profile = ConnectionProfile::new_draft();
    profile.name = "Invalid".to_owned();
    profile.server = "invalid.example".to_owned();
    let err = engine
        .prepare_connect(ConnectRequest {
            profile,
            dry_run: true,
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid"));
}

#[tokio::test]
async fn platform_start_completes_only_on_matching_connected_terminal() {
    let engine = SessionEngine::new();
    let attempt_id = engine.begin_platform_vpn_start().unwrap();

    assert!(!engine
        .fail_platform_vpn_start("older-attempt", "late rejection".to_owned())
        .unwrap());
    engine.set_platform_vpn_running(true).unwrap();

    assert_eq!(
        engine.await_platform_vpn_start(&attempt_id).await.unwrap(),
        PlatformStartOutcome::Connected
    );
    assert!(!engine
        .fail_platform_vpn_start(&attempt_id, "late rejection".to_owned())
        .unwrap());
}

#[tokio::test]
async fn platform_start_failure_is_exactly_once() {
    let engine = SessionEngine::new();
    let attempt_id = engine.begin_platform_vpn_start().unwrap();

    assert!(engine
        .fail_platform_vpn_start(&attempt_id, "system rejected".to_owned())
        .unwrap());
    assert!(!engine.cancel_platform_vpn_start(&attempt_id).unwrap());
    let error = engine
        .await_platform_vpn_start(&attempt_id)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("system rejected"));
}

#[tokio::test]
async fn platform_start_attachment_wait_distinguishes_authorization_bootstrap() {
    let engine = SessionEngine::new();
    let attempt_id = engine.begin_platform_vpn_start().unwrap();

    assert!(!engine
        .await_platform_vpn_start_attachment(&attempt_id, Duration::from_millis(10))
        .await
        .unwrap());
    engine.bind_platform_vpn_start(&attempt_id).unwrap();
    assert!(engine
        .await_platform_vpn_start_attachment(&attempt_id, Duration::from_millis(10))
        .await
        .unwrap());
}

#[test]
fn system_rejection_only_fails_before_extension_attachment() {
    let engine = SessionEngine::new();
    let unattached = engine.begin_platform_vpn_start().unwrap();
    assert!(engine
        .fail_unattached_platform_vpn_start(&unattached, "system rejected".to_owned(),)
        .unwrap());

    let attached = engine.begin_platform_vpn_start().unwrap();
    engine.bind_platform_vpn_start(&attached).unwrap();
    assert!(!engine
        .fail_unattached_platform_vpn_start(&attached, "late system rejection".to_owned(),)
        .unwrap());
    engine.set_platform_vpn_running(true).unwrap();
    assert!(!engine
        .fail_unattached_platform_vpn_start(&attached, "late timeout".to_owned())
        .unwrap());
}

#[tokio::test]
async fn platform_start_deadline_produces_one_failed_terminal() {
    let engine = SessionEngine::new();
    let attempt_id = engine.begin_platform_vpn_start().unwrap();

    let error = engine
        .await_platform_vpn_start_with_deadline(&attempt_id, Duration::from_millis(10))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("startup deadline"));
    assert!(!engine
        .fail_platform_vpn_start(&attempt_id, "late failure".to_owned())
        .unwrap());
    assert!(!engine.cancel_platform_vpn_start(&attempt_id).unwrap());
}

#[tokio::test]
#[cfg(feature = "native-anyconnect")]
async fn attach_tun_without_pending_is_rejected() {
    let engine = SessionEngine::new();
    let dir = tempfile::tempdir().unwrap();
    engine.configure_home(dir.path()).unwrap();
    let error = engine.attach_tun(3, "{}").await.unwrap_err();
    assert!(error.to_string().contains("ashmem"));
}

#[test]
fn platform_vpn_state_revision_is_strictly_monotonic() {
    let dir = tempfile::tempdir().unwrap();
    let engine = SessionEngine::new();
    engine.configure_home(dir.path()).unwrap();
    let future_revision = PlatformVpnState::now_nanos().saturating_add(3_600_000_000_000);
    {
        let mut inner = engine.lock().unwrap();
        inner.platform_vpn_state_updated_at = future_revision;
    }

    engine.set_platform_vpn_starting(true).unwrap();

    let revision = engine.lock().unwrap().platform_vpn_state_updated_at;
    assert_eq!(revision, future_revision + 1);
}

#[test]
fn selected_profile_survives_restart() {
    let dir = tempdir().unwrap();
    let engine = SessionEngine::new();
    engine.configure_home(dir.path()).unwrap();
    let mut first = ConnectionProfile::new_draft();
    first.id = "first".to_owned();
    first.name = "First".to_owned();
    first.server = "vpn-a.example.test".to_owned();
    engine.upsert_profile(first).unwrap();
    let mut second_profile = ConnectionProfile::new_draft();
    second_profile.id = "second".to_owned();
    second_profile.name = "Second".to_owned();
    second_profile.server = "vpn-b.example.test".to_owned();
    let second = second_profile.id.clone();
    engine.upsert_profile(second_profile).unwrap();
    engine.select_profile(&second).unwrap();
    assert_eq!(
        engine.snapshot().unwrap().active_connection_id.as_deref(),
        Some(second.as_str())
    );

    let restarted = SessionEngine::new();
    restarted.configure_home(dir.path()).unwrap();
    assert_eq!(
        restarted
            .snapshot()
            .unwrap()
            .active_connection_id
            .as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn production_store_starts_empty_and_stays_empty() {
    let dir = tempdir().unwrap();
    let engine = SessionEngine::new();
    engine.configure_home(dir.path()).unwrap();
    assert!(engine.snapshot().unwrap().connections.is_empty());
    // Idempotent delete must not panic.
    engine.delete_profile("missing-id").unwrap();

    let restarted = SessionEngine::new();
    restarted.configure_home(dir.path()).unwrap();
    assert!(
        restarted.snapshot().unwrap().connections.is_empty(),
        "empty store must not re-inject mock Corporate HQ / Lab Network"
    );
}

#[test]
fn log_recording_is_opt_in_and_archives_the_enabled_session() {
    let dir = tempdir().unwrap();
    let engine = SessionEngine::new();
    engine.configure_home(dir.path()).unwrap();

    assert!(!engine.log_recording_status().unwrap().enabled);
    assert!(engine.snapshot().unwrap().diagnostics.is_empty());

    let status = engine.set_log_recording_enabled(true).unwrap();
    assert!(status.enabled);
    {
        let mut inner = engine.lock().unwrap();
        engine.push_diag_locked(&mut inner, "warning", "recorded session event");
    }
    assert!(engine
        .snapshot()
        .unwrap()
        .diagnostics
        .iter()
        .any(|entry| entry.message == "recorded session event"));

    let stopped = engine.set_log_recording_enabled(false).unwrap();
    assert!(!stopped.enabled);
    assert!(engine.snapshot().unwrap().diagnostics.is_empty());
    assert_eq!(stopped.archives.len(), 1);
    let file_name = stopped.archives[0].file_name.clone();
    let content = engine.read_log_archive(&file_name).unwrap();
    assert!(content.contains("log recording enabled"));
    assert!(content.contains("recorded session event"));
    assert!(content.contains("log recording disabled"));

    let deleted = engine.delete_log_archive(&file_name).unwrap();
    assert!(deleted.archives.is_empty());
}

#[test]
fn authenticated_handoff_is_attempt_scoped_in_ashmem() {
    let ui = SessionEngine::new();
    {
        let mut inner = ui.lock().unwrap();
        inner.platform_session_handoff = Some(SessionHandoff {
            attempt_id: String::new(),
            options: VpnOptions {
                cookie: Some("session-cookie".to_owned()),
                ..VpnOptions::default()
            },
            network: NetworkSnapshot::default(),
            updated_at: PlatformVpnState::now_nanos(),
        });
    }
    let attempt_id = ui.begin_platform_vpn_start().unwrap();

    let handoff = ui.lock().unwrap().platform_session_handoff.clone().unwrap();
    assert!(handoff.is_valid_for(&attempt_id));
    assert_eq!(handoff.options.cookie.as_deref(), Some("session-cookie"));

    ui.clear_platform_session_handoff(&attempt_id).unwrap();
    assert!(ui.lock().unwrap().platform_session_handoff.is_none());
}

#[test]
fn browser_open_request_is_attempt_scoped_and_consumed_once() {
    let ui = SessionEngine::new();
    let attempt_id = ui.begin_platform_vpn_start().unwrap();
    let request = BrowserOpenRequest {
        request_id: "browser-1".to_owned(),
        attempt_id: attempt_id.clone(),
        uri: "https://idp.example/sso".to_owned(),
        requested_at_ms: PlatformVpnState::now_millis(),
    };
    let mut inner = ui.lock().unwrap();
    let request = consume_platform_browser_request_locked(&mut inner, Some(request.clone()))
        .expect("browser request");
    assert_eq!(request.attempt_id, attempt_id);
    assert_eq!(request.uri, "https://idp.example/sso");
    assert!(consume_platform_browser_request_locked(&mut inner, Some(request)).is_none());
}

#[test]
fn browser_open_request_is_cleared_only_by_its_matching_ui_ack() {
    let engine = SessionEngine::new();
    let mut inner = engine.lock().unwrap();
    inner.platform_browser_request = Some(BrowserOpenRequest {
        request_id: "browser-1".to_owned(),
        attempt_id: "attempt-1".to_owned(),
        uri: "https://idp.example/sso".to_owned(),
        requested_at_ms: PlatformVpnState::now_millis(),
    });

    assert!(!acknowledge_platform_browser_request_locked(
        &mut inner,
        Some("browser-stale")
    ));
    assert!(inner.platform_browser_request.is_some());
    assert!(acknowledge_platform_browser_request_locked(
        &mut inner,
        Some("browser-1")
    ));
    assert!(inner.platform_browser_request.is_none());
}

#[test]
fn configure_home_removes_obsolete_cross_process_files() {
    let dir = tempdir().unwrap();
    for file_name in OBSOLETE_CROSS_PROCESS_FILES {
        std::fs::write(dir.path().join(file_name), b"obsolete").unwrap();
    }

    let engine = SessionEngine::new();
    engine.configure_home(dir.path()).unwrap();

    for file_name in OBSOLETE_CROSS_PROCESS_FILES {
        assert!(!dir.path().join(file_name).exists());
    }
}

#[test]
fn platform_want_options_exclude_authenticated_secrets() {
    let options = VpnOptions {
        addresses: vec!["10.0.0.2/24".to_owned()],
        username: Some("alice".to_owned()),
        password: Some("password".to_owned()),
        cookie: Some("cookie".to_owned()),
        key_password: "key-password".to_owned(),
        token_string: "token".to_owned(),
        ..VpnOptions::default()
    };

    let sanitized = sanitized_want_options(&options);
    assert_eq!(sanitized.addresses, options.addresses);
    assert!(sanitized.username.is_none());
    assert!(sanitized.password.is_none());
    assert!(sanitized.cookie.is_none());
    assert!(sanitized.key_password.is_empty());
    assert!(sanitized.token_string.is_empty());
}
