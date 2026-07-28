//! Optional live AnyConnect headend test.
//!
//! ```sh
//! HANY_E2E_SERVER=https://vpn.example.com \
//! HANY_E2E_USER=alice \
//! HANY_E2E_PASSWORD=secret \
//! cargo test -p hanyconnect_core --features native-anyconnect --test live_connect -- --ignored --nocapture
//! ```

#![cfg(feature = "native-anyconnect")]

use hanyconnect_core::{
    shared_engine, AuthMethod, ConnectRequest, ConnectionProfile, ProtocolKind, SessionEngine,
};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[tokio::test]
#[ignore = "requires a reachable AnyConnect/OpenConnect headend"]
async fn live_prepare_connect_against_headend() {
    let server = env("HANY_E2E_SERVER").expect("HANY_E2E_SERVER");
    let username = env("HANY_E2E_USER").unwrap_or_default();
    let password = env("HANY_E2E_PASSWORD").unwrap_or_default();
    let group = env("HANY_E2E_GROUP").unwrap_or_default();

    let engine = SessionEngine::new();
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

    let options = engine
        .prepare_connect(ConnectRequest {
            profile,
            dry_run: false,
        })
        .await
        .expect("live prepare_connect");
    assert!(!options.addresses.is_empty());
    let snap = engine.snapshot().unwrap();
    assert!(snap.network.address.is_some() || !options.addresses.is_empty());
    let _ = shared_engine();
}
