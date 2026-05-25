pub mod database;
pub mod gui;
pub mod identity;
pub mod lingq;
pub mod settings;
pub mod taz;

use anyhow::{Context, Result};
use log::warn;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Taz Reader";
const APP_DIR_NAME: &str = "taz-reader";
const LEGACY_APP_DIR_NAME: &str = "taz_lingq_tool";
const DATABASE_FILE_NAME: &str = "library.db";
const LEGACY_DATABASE_FILE_NAME: &str = "taz_lingq_tool.db";

/// Returns the app data directory (`<local_app_data>/taz-reader/`), creating it if needed.
///
/// Existing installs under the legacy `taz_lingq_tool` folder are migrated in place
/// the first time the app runs after the rename.
pub fn app_data_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .context("could not determine local app data directory for this OS/user")?;
    prepare_app_data_dir(&base)
}

/// Returns the SQLite database path, migrating the old filename when possible.
pub fn app_database_path() -> Result<PathBuf> {
    let dir = app_data_dir()?;
    resolve_database_path(&dir)
}

fn prepare_app_data_dir(base: &Path) -> Result<PathBuf> {
    let preferred = base.join(APP_DIR_NAME);
    let legacy = base.join(LEGACY_APP_DIR_NAME);

    if preferred.exists() {
        std::fs::create_dir_all(&preferred)
            .with_context(|| format!("failed to create {}", preferred.display()))?;

        if legacy.exists() {
            let preferred_was_empty = !dir_has_entries(&preferred)?;
            let legacy_has_entries = dir_has_entries(&legacy)?;

            if preferred_was_empty && legacy_has_entries {
                if std::fs::remove_dir(&preferred).is_ok()
                    && try_rename_path(&legacy, &preferred, "app data directory")?
                {
                    return Ok(preferred);
                }

                std::fs::create_dir_all(&preferred)
                    .with_context(|| format!("failed to create {}", preferred.display()))?;
            }

            if legacy_has_entries {
                let moved_any = merge_dir_contents(&legacy, &preferred)?;
                let _ = remove_dir_if_empty(&legacy);

                if preferred_was_empty && !moved_any && dir_has_entries(&legacy).unwrap_or(false) {
                    return Ok(legacy);
                }
            } else {
                let _ = remove_dir_if_empty(&legacy);
            }
        }

        return Ok(preferred);
    }

    if legacy.exists() {
        if try_rename_path(&legacy, &preferred, "app data directory")? {
            return Ok(preferred);
        }

        std::fs::create_dir_all(&legacy)
            .with_context(|| format!("failed to create {}", legacy.display()))?;
        return Ok(legacy);
    }

    std::fs::create_dir_all(&preferred)
        .with_context(|| format!("failed to create {}", preferred.display()))?;
    Ok(preferred)
}

fn resolve_database_path(dir: &Path) -> Result<PathBuf> {
    let preferred = dir.join(DATABASE_FILE_NAME);
    let legacy = dir.join(LEGACY_DATABASE_FILE_NAME);

    if preferred.exists() || !legacy.exists() {
        return Ok(preferred);
    }

    if !try_rename_path(&legacy, &preferred, "database file")? {
        return Ok(legacy);
    }

    for suffix in ["-wal", "-shm"] {
        let legacy_aux = dir.join(format!("{LEGACY_DATABASE_FILE_NAME}{suffix}"));
        let preferred_aux = dir.join(format!("{DATABASE_FILE_NAME}{suffix}"));
        let _ = try_rename_path(&legacy_aux, &preferred_aux, "database sidecar file")?;
    }

    Ok(preferred)
}

fn try_rename_path(from: &Path, to: &Path, kind: &str) -> Result<bool> {
    if !from.exists() || to.exists() {
        return Ok(false);
    }

    match std::fs::rename(from, to) {
        Ok(()) => Ok(true),
        Err(err) => {
            warn!(
                "Could not migrate {kind} from {} to {}: {err}",
                from.display(),
                to.display()
            );
            Ok(false)
        }
    }
}

fn merge_dir_contents(from: &Path, to: &Path) -> Result<bool> {
    let mut moved_any = false;
    let entries =
        std::fs::read_dir(from).with_context(|| format!("failed to read {}", from.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", from.display()))?;
        let source = entry.path();
        let target = to.join(entry.file_name());

        if source.is_dir() {
            if !target.exists() {
                std::fs::create_dir_all(&target)
                    .with_context(|| format!("failed to create {}", target.display()))?;
            }
            moved_any |= merge_dir_contents(&source, &target)?;
            let _ = remove_dir_if_empty(&source);
            continue;
        }

        if target.exists() {
            continue;
        }

        moved_any |= try_rename_path(&source, &target, "app data file")?;
    }

    Ok(moved_any)
}

fn remove_dir_if_empty(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    if dir_has_entries(path)? {
        return Ok(false);
    }

    match std::fs::remove_dir(path) {
        Ok(()) => Ok(true),
        Err(err) => {
            warn!("Could not remove empty directory {}: {err}", path.display());
            Ok(false)
        }
    }
}

fn dir_has_entries(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut entries =
        std::fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?;
    match entries.next() {
        Some(entry) => {
            entry.with_context(|| format!("failed to read {}", path.display()))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn migrates_legacy_app_data_directory() {
        let base = unique_temp_dir("appdir");
        let legacy = base.join(LEGACY_APP_DIR_NAME);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();
        std::fs::write(legacy.join("lingq_token"), "token").unwrap();

        let resolved = prepare_app_data_dir(&base).unwrap();

        assert_eq!(resolved, base.join(APP_DIR_NAME));
        assert!(resolved.join("settings.json").exists());
        assert_eq!(
            std::fs::read_to_string(resolved.join("lingq_token")).unwrap(),
            "token"
        );
        assert!(!legacy.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migrates_legacy_database_filename() {
        let base = unique_temp_dir("dbfile");
        let dir = base.join(APP_DIR_NAME);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LEGACY_DATABASE_FILE_NAME), b"db").unwrap();
        std::fs::write(dir.join(format!("{LEGACY_DATABASE_FILE_NAME}-wal")), b"wal").unwrap();
        std::fs::write(dir.join(format!("{LEGACY_DATABASE_FILE_NAME}-shm")), b"shm").unwrap();

        let resolved = resolve_database_path(&dir).unwrap();

        assert_eq!(resolved, dir.join(DATABASE_FILE_NAME));
        assert!(resolved.exists());
        assert!(dir.join(format!("{DATABASE_FILE_NAME}-wal")).exists());
        assert!(dir.join(format!("{DATABASE_FILE_NAME}-shm")).exists());
        assert!(!dir.join(LEGACY_DATABASE_FILE_NAME).exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn merges_legacy_contents_into_existing_preferred_dir() {
        let base = unique_temp_dir("merge");
        let preferred = base.join(APP_DIR_NAME);
        let legacy = base.join(LEGACY_APP_DIR_NAME);
        std::fs::create_dir_all(&preferred).unwrap();
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(preferred.join("keep.txt"), "new").unwrap();
        std::fs::write(legacy.join("settings.json"), "{}").unwrap();
        std::fs::write(legacy.join(LEGACY_DATABASE_FILE_NAME), b"db").unwrap();

        let resolved = prepare_app_data_dir(&base).unwrap();

        assert_eq!(resolved, preferred);
        assert!(preferred.join("keep.txt").exists());
        assert!(preferred.join("settings.json").exists());
        assert!(preferred.join(LEGACY_DATABASE_FILE_NAME).exists());
        assert!(!legacy.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("taz-reader-{label}-{nonce}"))
    }
}
