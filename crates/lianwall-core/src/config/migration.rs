//! One-time on-disk migration; the legacy key is never part of the public schema.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::ConfigError;

fn failure(path: &Path, message: impl ToString) -> ConfigError {
    ConfigError::Migration {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

/// Preserve comments and all unrelated TOML. Dual keys are ambiguous even if equal.
pub(super) fn prepare(path: &Path, content: &str) -> Result<Option<String>, ConfigError> {
    let mut document: toml_edit::DocumentMut = content.parse().map_err(|e| failure(path, e))?;
    let Some(image) = document
        .get_mut("image_engine")
        .and_then(|v| v.as_table_like_mut())
    else {
        return Ok(None);
    };
    if !image.contains_key("swww_args") {
        return Ok(None);
    }
    if image.contains_key("awww_args") {
        return Err(failure(
            path,
            "Both image_engine.swww_args and awww_args exist; keep the intended value under awww_args and remove swww_args",
        ));
    }
    // TableLike has no rename API. Preserve the old key's decor on the new key.
    let decor = image
        .key_mut("swww_args")
        .expect("key exists")
        .leaf_decor()
        .clone();
    let value = image.remove("swww_args").expect("key exists");
    image.insert("awww_args", value);
    *image
        .key_mut("awww_args")
        .expect("inserted")
        .leaf_decor_mut() = decor;
    Ok(Some(document.to_string()))
}

pub(super) fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".pre-awww.bak");
    PathBuf::from(name)
}

/// Never replace an existing backup. A conflicting backup prevents disk migration.
pub(super) fn persist(path: &Path, original: &str, migrated: &str) -> Result<(), ConfigError> {
    let backup = backup_path(path);
    let verify_backup = || -> Result<(), ConfigError> {
        let metadata = fs::symlink_metadata(&backup).map_err(|e| failure(&backup, e))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(failure(&backup, "Backup must be a regular file"));
        }
        if fs::read(&backup).map_err(|e| failure(&backup, e))? != original.as_bytes() {
            return Err(failure(
                &backup,
                "Existing backup differs; preserve or move it before retrying migration",
            ));
        }
        Ok(())
    };
    match fs::symlink_metadata(&backup) {
        Ok(_) => verify_backup()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = backup
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let mut staged =
                tempfile::NamedTempFile::new_in(parent).map_err(|e| failure(&backup, e))?;
            staged
                .write_all(original.as_bytes())
                .map_err(|e| failure(&backup, e))?;
            staged
                .as_file()
                .sync_all()
                .map_err(|e| failure(&backup, e))?;
            match staged.persist_noclobber(&backup) {
                Ok(_) => (),
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => verify_backup()?,
                Err(e) => return Err(failure(&backup, e.error)),
            }
        }
        Err(e) => return Err(failure(&backup, e)),
    }
    crate::filesystem::atomic_write(path, migrated).map_err(|e| failure(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, ConfigReadInput, ConfigUpdateInput, DEFAULT_CONFIG_TOML};
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn legacy() -> String {
        DEFAULT_CONFIG_TOML.replace("awww_args", "swww_args")
    }
    fn read(path: &Path) -> Result<config::ConfigReadOutput, ConfigError> {
        config::read(ConfigReadInput {
            path: Some(path.to_path_buf()),
        })
    }

    #[test]
    fn migrates_with_exact_backup_comments_values_and_idempotence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = legacy().replace("swww_args =", "# user comment\nswww_args =");
        fs::write(&path, &original).unwrap();
        let loaded = read(&path).unwrap();
        assert!(!loaded.config.image_engine.awww_args.is_empty());
        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("# user comment"));
        assert!(migrated.contains("awww_args ="));
        assert!(!migrated.contains("swww_args ="));
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), original);
        assert_eq!(
            loaded.config.image_engine.awww_args,
            read(&path).unwrap().config.image_engine.awww_args
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), migrated);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        // Restoring the backup triggers the same deterministic migration and reuses it.
        fs::copy(backup_path(&path), &path).unwrap();
        read(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), migrated);
    }

    #[test]
    fn new_config_is_unchanged_and_old_api_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, DEFAULT_CONFIG_TOML).unwrap();
        let mut loaded = read(&path).unwrap().config;
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG_TOML);
        assert!(!backup_path(&path).exists());
        assert!(config::get_value(&loaded, "image_engine.swww_args").is_none());
        assert!(
            config::set_value(&mut loaded, "image_engine.swww_args", serde_json::json!([]))
                .is_err()
        );
        config::set_value(
            &mut loaded,
            "image_engine.awww_args",
            serde_json::json!(["--transition-type", "fade"]),
        )
        .unwrap();
        assert_eq!(loaded.image_engine.awww_args, ["--transition-type", "fade"]);
    }

    #[test]
    fn dual_keys_fail_without_any_disk_write_even_when_equal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = legacy().replace("swww_args =", "awww_args = []\nswww_args =");
        fs::write(&path, &original).unwrap();
        assert!(read(&path).unwrap_err().to_string().contains("Both"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn malformed_legacy_is_not_migrated_or_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        for original in [
            "[image_engine]\nswww_args = 5",
            "[image_engine]\nswww_args = [",
        ] {
            fs::write(&path, original).unwrap();
            assert!(read(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            assert!(!backup_path(&path).exists());
        }
    }

    #[test]
    fn readonly_config_loads_in_memory_but_save_fails_without_data_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, legacy()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let loaded = read(&path).unwrap().config;
        assert!(!loaded.image_engine.awww_args.is_empty());
        assert!(
            config::update(ConfigUpdateInput {
                path: Some(path.clone()),
                config: loaded
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), legacy());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn conflicting_or_symlink_backup_is_never_overwritten_and_blocks_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, legacy()).unwrap();
        let backup = backup_path(&path);
        fs::write(&backup, "older backup").unwrap();
        let loaded = read(&path).unwrap().config;
        assert!(
            config::update(ConfigUpdateInput {
                path: Some(path.clone()),
                config: loaded
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "older backup");
        fs::remove_file(&backup).unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, legacy()).unwrap();
        symlink(&victim, &backup).unwrap();
        read(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), legacy());
        assert!(
            fs::symlink_metadata(&backup)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn config_symlink_is_preserved_during_migration() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfile");
        let path = dir.path().join("config.toml");
        fs::write(&target, legacy()).unwrap();
        symlink(&target, &path).unwrap();
        read(&path).unwrap();
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!fs::read_to_string(&target).unwrap().contains("swww_args ="));
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), legacy());
    }
}
