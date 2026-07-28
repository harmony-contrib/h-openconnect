use crate::error::{CoreError, CoreResult};
use crate::model::ConnectionProfile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILES_FILE: &str = "connections.json";
const PREFERENCES_FILE: &str = "preferences.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub active_connection_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    pub fn open(root: impl Into<PathBuf>) -> CoreResult<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
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
        let tmp = self.root.join(format!("{PROFILES_FILE}.tmp"));
        // Credentials are profile fields and persist in the app-private sandbox.
        let raw = serde_json::to_string_pretty(profiles)?;
        fs::write(&tmp, raw)?;
        fs::rename(&tmp, file).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            CoreError::from(err)
        })
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
        let tmp = self.root.join(format!("{PREFERENCES_FILE}.tmp"));
        let raw = serde_json::to_string_pretty(preferences)?;
        fs::write(&tmp, raw)?;
        fs::rename(&tmp, file).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            CoreError::from(err)
        })
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
    }
}
