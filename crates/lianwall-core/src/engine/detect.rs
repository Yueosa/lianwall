//! Executable detection only; the daemon owns the single engine lifecycle implementation.

pub async fn is_mpvpaper_available() -> bool {
    executable_in_path("mpvpaper")
}

/// Resolve executables directly; minimal installations need not ship the `which` tool.
pub fn executable_in_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| executable_in(name, &path))
}

fn executable_in(name: &str, path: &std::ffi::OsStr) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path).any(|directory| {
        std::fs::metadata(directory.join(name))
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

/// Prefer awww, requiring both the client and its matching daemon.
pub async fn detect_image_bin() -> Option<String> {
    ["awww", "swww"]
        .into_iter()
        .find(|bin| executable_in_path(bin) && executable_in_path(&format!("{bin}-daemon")))
        .map(str::to_string)
}

pub async fn detect() -> super::DetectOutput {
    super::DetectOutput {
        mpvpaper_available: is_mpvpaper_available().await,
        swww_available: detect_image_bin().await.is_some(),
    }
}

#[cfg(test)]
mod detection_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn path_detection_needs_no_which_and_rejects_nonexecutables_and_directories() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("awww");
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        assert!(!executable_in("awww", directory.path().as_os_str()));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(executable_in("awww", directory.path().as_os_str()));
        std::fs::create_dir(directory.path().join("swww")).unwrap();
        assert!(!executable_in("swww", directory.path().as_os_str()));
    }
}
