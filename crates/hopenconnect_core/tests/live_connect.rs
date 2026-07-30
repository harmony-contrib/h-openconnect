//! Optional live AnyConnect headend test.
//!
//! ```sh
//! HOPEN_E2E_SERVER=https://vpn.example.com \
//! HOPEN_E2E_USER=alice \
//! HOPEN_E2E_PASSWORD=secret \
//! cargo test -p hopenconnect_core --features native-anyconnect --test live_connect -- --ignored --nocapture
//! ```

#![cfg(feature = "native-anyconnect")]

use hopenconnect_core::{
    AuthMethod, ConnectRequest, ConnectionProfile, ProtocolKind, SessionEngine,
};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[tokio::test]
#[ignore = "requires a reachable AnyConnect/OpenConnect headend"]
async fn live_prepare_connect_against_headend() {
    let server = env("HOPEN_E2E_SERVER").expect("HOPEN_E2E_SERVER");
    let username = env("HOPEN_E2E_USER").unwrap_or_default();
    let password = env("HOPEN_E2E_PASSWORD").unwrap_or_default();
    let group = env("HOPEN_E2E_GROUP").unwrap_or_default();

    let home = tempfile::tempdir().expect("create isolated live-test home");
    let engine = SessionEngine::new();
    engine
        .configure_home(home.path())
        .expect("configure isolated live-test home");
    let mut profile = ConnectionProfile::new_draft();
    profile.id = "live".to_owned();
    profile.name = "Live".to_owned();
    profile.server = server;
    profile.username = username;
    profile.password = password;
    profile.group = group;
    profile.protocol = ProtocolKind::Ssl;
    profile.auth_method = AuthMethod::Password;
    // Lab gear often uses private CA — allow for this optional test only.
    profile.strict_certificate_trust = false;
    profile.block_untrusted_servers = false;

    let handoff = engine
        .prepare_connect(ConnectRequest {
            profile,
            dry_run: false,
        })
        .await
        .expect("live prepare_connect");
    assert!(handoff.cookie.is_some());
    let options_json = serde_json::to_string(&handoff).expect("serialize handoff");
    let resumed_json = engine
        .prepare_in_extension(&options_json)
        .await
        .expect("resume cookie and establish CSTP");
    let options: hopenconnect_core::VpnOptions =
        serde_json::from_str(&resumed_json).expect("parse resumed options");
    assert!(!options.addresses.is_empty());
    let snap = engine.snapshot().unwrap();
    assert!(snap.network.address.is_some() || !options.addresses.is_empty());
}
