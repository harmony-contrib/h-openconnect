use super::*;
use tempfile::tempdir;

#[test]
fn credentials_persist_with_the_local_profile() {
    let dir = tempdir().unwrap();
    let store = ProfileStore::open(dir.path()).unwrap();
    let mut profile = ConnectionProfile::new_draft();
    profile.id = "saved".to_owned();
    profile.name = "Saved".to_owned();
    profile.server = "vpn.example.test".to_owned();
    profile.password = "primary-secret".to_owned();
    profile.key_password = "key-secret".to_owned();
    profile.secondary_key_password = "secondary-key-secret".to_owned();

    store.save(&[profile.clone()]).unwrap();

    assert_eq!(store.load().unwrap(), vec![profile]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.path().join(PROFILES_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn appearance_preferences_round_trip() {
    let dir = tempdir().unwrap();
    let store = ProfileStore::open(dir.path()).unwrap();
    let preferences = Preferences {
        active_connection_id: Some("vpn".to_owned()),
        language: "zh-CN".to_owned(),
        theme: "dark".to_owned(),
    };
    store.save_preferences(&preferences).unwrap();
    let loaded = store.load_preferences().unwrap();
    assert_eq!(loaded.active_connection_id.as_deref(), Some("vpn"));
    assert_eq!(loaded.language, "zh-CN");
    assert_eq!(loaded.theme, "dark");
}
