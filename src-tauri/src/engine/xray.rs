use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

/// Locate the packaged, repository-controlled Xray sidecar. Unlike sing-box,
/// Xray is never downloaded or selected through a lossy config conversion.
pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
    let names = candidate_names();
    if let Ok(path) = std::env::var("XRAY_BIN") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(path) = first_existing(dir, &names) {
                return Ok(path);
            }
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        if let Some(path) = first_existing(&resource_dir.join("binaries"), &names) {
            return Ok(path);
        }
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let mut cursor = Some(PathBuf::from(manifest));
        for _ in 0..4 {
            let Some(dir) = cursor.take() else { break };
            if let Some(path) = first_existing(&dir.join("binaries"), &names) {
                return Ok(path);
            }
            cursor = dir.parent().map(Path::to_path_buf);
        }
    }
    Err(AppError::BinaryNotFound(
        "Xray-core sidecar is unavailable".to_string(),
    ))
}

pub fn run_args(config_path: &Path) -> [std::ffi::OsString; 3] {
    [
        "run".into(),
        "-c".into(),
        config_path.as_os_str().to_os_string(),
    ]
}

fn candidate_names() -> [String; 2] {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    [
        format!("xray-x86_64-pc-windows-msvc{suffix}"),
        format!("xray{suffix}"),
    ]
}

fn first_existing(dir: &Path, names: &[String; 2]) -> Option<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_raw_xray_config_arguments() {
        let args = run_args(Path::new("profile.json"));
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "-c");
        assert_eq!(args[2], Path::new("profile.json").as_os_str());
    }

    #[test]
    fn resolver_has_targeted_sidecar_names() {
        let names = candidate_names();
        assert!(names.iter().any(|name| name.starts_with("xray-")));
        assert!(names.iter().any(|name| name.starts_with("xray")));
    }
}
