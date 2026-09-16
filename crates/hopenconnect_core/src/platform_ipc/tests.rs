use super::*;

#[test]
fn frame_header_rejects_corruption() {
    let mut header = [0_u8; FRAME_HEADER_SIZE];
    header[0..4].copy_from_slice(&FRAME_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    header[8..16].copy_from_slice(&7_u64.to_le_bytes());
    header[16..20].copy_from_slice(&12_u32.to_le_bytes());
    header[20..24].copy_from_slice(&34_u32.to_le_bytes());
    let header_checksum = checksum(&header[..24]);
    header[24..28].copy_from_slice(&header_checksum.to_le_bytes());
    assert_eq!(
        FrameHeader::parse(&header, 1024).map(|frame| frame.generation),
        Some(7)
    );
    header[8] ^= 1;
    assert!(FrameHeader::parse(&header, 1024).is_none());
}

#[test]
fn clearing_sensitive_payload_requires_both_local_slots_to_be_overwritten() {
    let mut published = PlatformEnvelope::default();
    let attempt_id = "attempt-sensitive".to_owned();
    let cookie = "cookie-must-not-remain-in-ashmem-slots";
    let uri = "https://idp.example/private-sso-request";
    assert!(!replace_snapshot(
        &mut published,
        PlatformVpnState::default(),
        Some(SessionHandoff {
            attempt_id: attempt_id.clone(),
            options: crate::model::VpnOptions {
                cookie: Some(cookie.to_owned()),
                ..crate::model::VpnOptions::default()
            },
            network: crate::model::NetworkSnapshot::default(),
            updated_at: PlatformVpnState::now_nanos(),
        }),
        Some(BrowserOpenRequest {
            request_id: "browser-1".to_owned(),
            attempt_id,
            uri: uri.to_owned(),
            requested_at_ms: PlatformVpnState::now_millis(),
        }),
        None,
    ));
    assert_eq!(
        published
            .session_handoff
            .clone()
            .and_then(|handoff| handoff.options.cookie),
        Some(cookie.to_owned())
    );

    assert!(replace_snapshot(
        &mut published,
        PlatformVpnState::default(),
        None,
        None,
        None,
    ));
    let cleared = serde_json::to_vec(&published).unwrap();
    assert!(!cleared
        .windows(cookie.len())
        .any(|window| window == cookie.as_bytes()));
    assert!(!cleared
        .windows(uri.len())
        .any(|window| window == uri.as_bytes()));
}

#[test]
fn blocking_event_subscription_wakes_for_peer_publication() {
    let (local, peer) = create_notification_pair().unwrap();
    let subscription =
        Arc::new(SocketNotification::new(None, NotificationAddress::new(0), Some(local)).unwrap());
    let notifier = SocketNotification::new(None, NotificationAddress::new(0), Some(peer)).unwrap();
    let waiter = {
        let subscription = Arc::clone(&subscription);
        std::thread::spawn(move || subscription.wait(None))
    };

    notifier.notify().unwrap();

    assert!(waiter.join().unwrap().unwrap());
}

#[test]
fn stream_notifications_coalesce_and_remain_bidirectional() {
    let (local, peer) = create_notification_pair().unwrap();
    let first = SocketNotification::new(None, NotificationAddress::new(0), Some(local)).unwrap();
    let second = SocketNotification::new(None, NotificationAddress::new(0), Some(peer)).unwrap();

    for _ in 0..100 {
        first.notify().unwrap();
    }
    assert!(second.wait(Some(Duration::ZERO)).unwrap());
    assert!(!second.wait(Some(Duration::ZERO)).unwrap());
    second.notify().unwrap();
    assert!(first.wait(Some(Duration::ZERO)).unwrap());
}

#[test]
fn stream_peer_close_terminates_the_wait() {
    let (local, peer) = create_notification_pair().unwrap();
    let subscription =
        SocketNotification::new(None, NotificationAddress::new(0), Some(local)).unwrap();
    drop(peer);

    assert!(subscription.wait_event_cancellable().is_err());
    assert!(subscription.notify().is_err());
}

#[test]
fn cancellable_wait_keeps_a_pre_registration_wakeup() {
    let (local, _peer) = create_notification_pair().unwrap();
    let subscription =
        SocketNotification::new(None, NotificationAddress::new(0), Some(local)).unwrap();

    subscription.cancel_waits();

    assert!(!subscription.wait_event_cancellable().unwrap());
}

#[test]
fn cancellation_is_scoped_to_one_ipc_binding() {
    let (first_local, _first_peer) = create_notification_pair().unwrap();
    let first =
        SocketNotification::new(None, NotificationAddress::new(0), Some(first_local)).unwrap();
    let (second_local, second_peer) = create_notification_pair().unwrap();
    let second =
        SocketNotification::new(None, NotificationAddress::new(0), Some(second_local)).unwrap();
    let notifier =
        SocketNotification::new(None, NotificationAddress::new(0), Some(second_peer)).unwrap();

    first.cancel_waits();
    notifier.notify().unwrap();

    assert!(!first.wait_event_cancellable().unwrap());
    assert!(second.wait_event_cancellable().unwrap());
}
