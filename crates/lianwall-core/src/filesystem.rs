//! Atomic replacement for small configuration/state files, following existing dotfile links.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Stage and sync in the destination directory, then rename in one filesystem operation.
/// A write/rename error leaves the old file intact. A dangling final symlink is an error.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_with(path, |file| file.write_all(contents.as_ref()))
}

fn write_with(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let previous = match fs::metadata(&target) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Destination is not a regular file",
                ));
            }
            if metadata.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Destination is read-only",
                ));
            }
            // Atomic rename must not bypass the existing file's write permissions/ACL.
            drop(OpenOptions::new().write(true).open(&target)?);
            Some(metadata.permissions())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".lianwall-")
        .tempfile_in(parent)?;
    if let Some(permissions) = previous {
        staged.as_file().set_permissions(permissions)?;
    }
    write(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged.persist(&target).map_err(|error| error.error)?;
    // The rename has committed. A directory-sync error must not be reported as an
    // uncommitted edit, otherwise callers would keep old memory after changing disk.
    if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
        tracing::warn!(path = %parent.display(), %error, "File committed but directory sync failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn failed_staged_write_preserves_old_file_and_cleans_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        atomic_write(&path, "old complete contents").unwrap();
        let result = write_with(&path, |file| {
            file.write_all(b"partial replacement")?;
            Err(io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old complete contents");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn follows_dotfile_symlink_and_preserves_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("dotfile");
        let link = directory.path().join("config.toml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        symlink("dotfile", &link).unwrap();
        atomic_write(&link, "new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn refuses_readonly_targets_and_preserves_dangling_links() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("readonly");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(atomic_write(&target, "new").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        let link = directory.path().join("dangling");
        symlink("missing", &link).unwrap();
        assert!(atomic_write(&link, "new").is_err());
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
