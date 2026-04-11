use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const KEY_FILE_EXTENSION: &str = "key";
const ENV_MAPPINGS: [(&str, &str); 4] = [
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OPENAI_API_KEY"),
    ("gemini", "GEMINI_API_KEY"),
    ("groq", "GROQ_API_KEY"),
];

pub struct KeyStore {
    keys_dir: PathBuf,
}

impl KeyStore {
    pub fn new(keys_dir: &Path) -> Self {
        Self {
            keys_dir: keys_dir.to_path_buf(),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if !Self::is_valid_key_name(key) {
            return None;
        }

        if let Some(env_name) = env_var_name(key) {
            if let Ok(value) = std::env::var(env_name) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        let content = fs::read_to_string(self.key_path(key)).ok()?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        if !Self::is_valid_key_name(key) {
            return Err(format!("Invalid key name '{}'", key));
        }

        self.ensure_keys_dir()?;
        let key_path = self.key_path(key);
        fs::write(&key_path, value)
            .map_err(|err| format!("Failed to write key '{}': {}", key, err))?;
        Self::set_mode(&key_path, 0o600)?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        if !Self::is_valid_key_name(key) {
            return Err(format!("Invalid key name '{}'", key));
        }

        let key_path = self.key_path(key);
        match fs::remove_file(&key_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("Failed to delete key '{}': {}", key, err)),
        }
    }

    pub fn list_stored(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let entries = match fs::read_dir(&self.keys_dir) {
            Ok(entries) => entries,
            Err(_) => return keys,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if extension != KEY_FILE_EXTENSION {
                continue;
            }

            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if Self::is_valid_key_name(stem) {
                keys.push(stem.to_string());
            }
        }

        keys.sort();
        keys.dedup();
        keys
    }

    pub fn list_from_env(&self) -> Vec<String> {
        let mut keys = ENV_MAPPINGS
            .iter()
            .filter_map(|(key, env_name)| match std::env::var(env_name) {
                Ok(value) if !value.trim().is_empty() => Some((*key).to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    fn ensure_keys_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.keys_dir).map_err(|err| {
            format!(
                "Failed to create keys dir '{}': {}",
                self.keys_dir.display(),
                err
            )
        })?;
        Self::set_mode(&self.keys_dir, 0o700)
    }

    fn key_path(&self, key: &str) -> PathBuf {
        self.keys_dir.join(format!("{}.{}", key, KEY_FILE_EXTENSION))
    }

    fn is_valid_key_name(key: &str) -> bool {
        !key.trim().is_empty()
            && !key.contains('/')
            && !key.contains('\\')
            && !key.contains("..")
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
        let permissions = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, permissions)
            .map_err(|err| format!("Failed to set permissions on '{}': {}", path.display(), err))
    }

    #[cfg(not(unix))]
    fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
        Ok(())
    }
}

pub fn env_var_name(key: &str) -> Option<&'static str> {
    match key {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "gemini" => Some("GEMINI_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{env_var_name, KeyStore};
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn get_prefers_environment_over_disk() {
        let dir = tempdir().unwrap();
        let store = KeyStore::new(dir.path());
        store.set("anthropic", "disk-secret").unwrap();

        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "env-secret");
        }

        assert_eq!(store.get("anthropic").as_deref(), Some("env-secret"));

        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }

    #[test]
    fn set_creates_restricted_key_file() {
        let dir = tempdir().unwrap();
        let keys_dir = dir.path().join("keys");
        let store = KeyStore::new(&keys_dir);
        store.set("gemini", "AIza-test").unwrap();

        let dir_mode = std::fs::metadata(&keys_dir).unwrap().permissions().mode() & 0o777;
        let file_mode = std::fs::metadata(keys_dir.join("gemini.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
        assert_eq!(store.get("gemini").as_deref(), Some("AIza-test"));
    }

    #[test]
    fn delete_removes_key_file_and_list_tracks_stored_keys() {
        let dir = tempdir().unwrap();
        let store = KeyStore::new(dir.path());
        store.set("runtime-contract", "secret").unwrap();
        store.set("openai-codex.access_token", "token").unwrap();

        assert_eq!(
            store.list_stored(),
            vec![
                "openai-codex.access_token".to_string(),
                "runtime-contract".to_string()
            ]
        );

        store.delete("runtime-contract").unwrap();
        assert_eq!(
            store.list_stored(),
            vec!["openai-codex.access_token".to_string()]
        );
    }

    #[test]
    fn list_from_env_reports_only_known_populated_mappings() {
        let dir = tempdir().unwrap();
        let store = KeyStore::new(dir.path());

        unsafe {
            std::env::set_var("OPENAI_API_KEY", "sk-test");
            std::env::set_var("GEMINI_API_KEY", "");
        }

        assert_eq!(store.list_from_env(), vec!["openai".to_string()]);
        assert_eq!(env_var_name("groq"), Some("GROQ_API_KEY"));
        assert_eq!(env_var_name("unknown"), None);

        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("GEMINI_API_KEY");
        }
    }
}
