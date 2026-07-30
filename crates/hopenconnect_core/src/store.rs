use crate::error::CoreResult;
use crate::model::ConnectionProfile;
use crate::private_fs::{ensure_private_dir, write_atomic_private};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILES_FILE: &str = "connections.json";
const PREFERENCES_FILE: &str = "preferences.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub active_connection_id: Option<String>,
    #[serde(default = "default_system_preference")]
    pub language: String,
    #[serde(default = "default_system_preference")]
    pub theme: String,
}

fn default_system_preference() -> String {
    "system".to_owned()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            active_connection_id: None,
            language: default_system_preference(),
            theme: default_system_preference(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    pub fn open(root: impl Into<PathBuf>) -> CoreResult<Self> {
        let root = root.into();
        ensure_private_dir(&root)?;
        Ok(Self { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// True when the on-disk profile file has never been created.
    /// An empty list is a valid persisted state (user deleted everything).
    pub fn profiles_file_exists(&self) -> bool {
        self.root.join(PROFILES_FILE).exists()
    }

    pub fn load(&self) -> CoreResult<Vec<ConnectionProfile>> {
        let file = self.root.join(PROFILES_FILE);
        if !file.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(file)?;
        let profiles: Vec<ConnectionProfile> = serde_json::from_str(&raw)?;
        Ok(profiles)
    }

    pub fn save(&self, profiles: &[ConnectionProfile]) -> CoreResult<()> {
        let file = self.root.join(PROFILES_FILE);
        // Credentials are profile fields and persist in the app-private sandbox.
        let raw = serde_json::to_string_pretty(profiles)?;
        write_atomic_private(&file, raw.as_bytes())
    }

    pub fn load_preferences(&self) -> CoreResult<Preferences> {
        let file = self.root.join(PREFERENCES_FILE);
        if !file.exists() {
            return Ok(Preferences::default());
        }
        let raw = fs::read_to_string(file)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save_preferences(&self, preferences: &Preferences) -> CoreResult<()> {
        let file = self.root.join(PREFERENCES_FILE);
        let raw = serde_json::to_string_pretty(preferences)?;
        write_atomic_private(&file, raw.as_bytes())
    }
}

#[cfg(test)]
mod tests {
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
}
