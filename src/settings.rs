use anyhow::{Context, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    // Navigation
    pub last_view: String,
    pub browse_section: String,

    // Browse filters
    pub browse_only_new: bool,
    pub browse_date_from: String,
    pub browse_date_to: String,

    // Bulk fetch params
    pub bulk_max_articles: String,
    pub bulk_per_section_cap: String,
    pub bulk_stop_after_old: String,

    // Library filters
    pub library_sort: String,
    pub library_only_not_uploaded: bool,
    pub library_min_words: String,
    pub library_max_words: String,
    pub library_duplicate_only: bool,

    // LingQ settings
    pub lingq_language: String,
    pub lingq_api_key: String,
    pub lingq_collection_id: Option<i64>,
    pub lingq_only_not_uploaded: bool,
    pub lingq_min_words: String,
    pub lingq_max_words: String,

    // UI toggles
    pub show_library_filters: bool,
    pub show_upload_tools: bool,
    pub preview_wide: bool,
    pub article_density: String,

    // Auto-fetch
    pub auto_fetch_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_view: "browse".to_owned(),
            browse_section: "politik".to_owned(),
            browse_only_new: true,
            browse_date_from: String::new(),
            browse_date_to: String::new(),
            bulk_max_articles: "60".to_owned(),
            bulk_per_section_cap: "30".to_owned(),
            bulk_stop_after_old: "12".to_owned(),
            library_sort: "newest".to_owned(),
            library_only_not_uploaded: false,
            library_min_words: String::new(),
            library_max_words: String::new(),
            library_duplicate_only: false,
            lingq_language: "de".to_owned(),
            lingq_api_key: String::new(),
            lingq_collection_id: None,
            lingq_only_not_uploaded: true,
            lingq_min_words: String::new(),
            lingq_max_words: String::new(),
            show_library_filters: true,
            show_upload_tools: true,
            preview_wide: false,
            article_density: "compact".to_owned(),
            auto_fetch_on_startup: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    data: AppSettings,
}

impl SettingsStore {
    /// Create a fallback store that writes to a temp path.
    pub fn fallback_default() -> Self {
        Self {
            path: std::env::temp_dir().join("taz-reader-settings.json"),
            data: AppSettings::default(),
        }
    }

    pub fn load_default() -> Result<Self> {
        let path = crate::app_data_dir()?.join("settings.json");
        Self::load(path)
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        let data = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            match serde_json::from_str(&raw) {
                Ok(settings) => settings,
                Err(err) => {
                    eprintln!(
                        "warning: settings file {} is malformed ({}), using defaults",
                        path.display(),
                        err
                    );
                    AppSettings::default()
                }
            }
        } else {
            AppSettings::default()
        };

        Ok(Self { path, data })
    }

    pub fn data(&self) -> &AppSettings {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut AppSettings {
        &mut self.data
    }

    pub fn update<F>(&mut self, updater: F) -> Result<()>
    where
        F: FnOnce(&mut AppSettings),
    {
        updater(&mut self.data);
        self.save()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw =
            serde_json::to_string_pretty(&self.data).context("failed to serialize settings")?;
        std::fs::write(&self.path, raw)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}

// Store the LingQ token outside `settings.json`.

fn api_key_path() -> Result<PathBuf> {
    Ok(api_key_path_in(&crate::app_data_dir()?))
}

fn api_key_path_in(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("lingq_token")
}

/// Load the LingQ API key from its dedicated file, falling back to `settings.json`
/// so older installs can migrate forward.
pub fn load_api_key(settings: &mut AppSettings) -> String {
    if let Ok(path) = api_key_path()
        && path.exists()
        && let Ok(token) = std::fs::read_to_string(&path)
    {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return token;
        }
    }
    let legacy = std::mem::take(&mut settings.lingq_api_key);
    if !legacy.trim().is_empty()
        && let Err(err) = save_api_key(&legacy)
    {
        warn!("Failed to migrate API key to token file: {err:#}");
    }
    legacy
}

/// Save the LingQ API key to its dedicated file with restricted permissions.
pub fn save_api_key(key: &str) -> Result<()> {
    let path = api_key_path()?;
    std::fs::write(&path, key)
        .with_context(|| format!("failed to write token to {}", path.display()))?;
    // Restrict permissions on Unix (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn settings_round_trip() {
        let dir = unique_temp_dir("round-trip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_settings.json");

        let store = SettingsStore::load(path.clone()).unwrap();
        store.save().unwrap();

        let mut store = SettingsStore::load(path.clone()).unwrap();
        store
            .update(|s| {
                s.browse_section = "kultur".to_owned();
                s.browse_only_new = false;
                s.library_sort = "title".to_owned();
                s.library_duplicate_only = true;
                s.article_density = "comfortable".to_owned();
                s.lingq_language = "fr".to_owned();
                s.bulk_max_articles = "100".to_owned();
            })
            .unwrap();

        let loaded = SettingsStore::load(path.clone()).unwrap();
        let d = loaded.data();
        assert_eq!(d.browse_section, "kultur");
        assert!(!d.browse_only_new);
        assert_eq!(d.library_sort, "title");
        assert!(d.library_duplicate_only);
        assert_eq!(d.article_density, "comfortable");
        assert_eq!(d.lingq_language, "fr");
        assert_eq!(d.bulk_max_articles, "100");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_malformed_json_uses_defaults() {
        let dir = unique_temp_dir("malformed");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad_settings.json");
        std::fs::write(&path, "not json at all").unwrap();

        let store = SettingsStore::load(path).unwrap();
        let d = store.data();
        assert_eq!(d.browse_section, "politik");
        assert_eq!(d.lingq_language, "de");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_missing_file_uses_defaults() {
        let dir = unique_temp_dir("missing");
        let path = dir.join("missing.json");
        let store = SettingsStore::load(path).unwrap();
        assert_eq!(store.data().browse_section, "politik");
    }

    #[test]
    fn settings_partial_json_fills_defaults() {
        let dir = unique_temp_dir("partial");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("partial.json");
        std::fs::write(&path, r#"{"browse_section":"sport"}"#).unwrap();

        let store = SettingsStore::load(path).unwrap();
        let d = store.data();
        assert_eq!(d.browse_section, "sport");
        assert_eq!(d.lingq_language, "de");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn api_key_path_uses_app_data_token_file() {
        let dir = unique_temp_dir("token-path");
        let path = api_key_path_in(&dir);

        assert_eq!(path, dir.join("lingq_token"));
        assert_ne!(path, dir.join("settings.json"));
    }

    #[test]
    fn fallback_default_has_correct_defaults() {
        let store = SettingsStore::fallback_default();
        assert_eq!(store.data().browse_section, "politik");
        assert_eq!(store.data().lingq_language, "de");
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let dir = unique_temp_dir("save-parent");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("settings.json");

        let store = SettingsStore::load(path.clone()).unwrap();
        store.save().unwrap();

        assert!(path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("taz-reader-settings-{label}-{nonce}"))
    }
}
