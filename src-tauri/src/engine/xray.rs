use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

const EXPECTED_EXECUTABLE_SHA256: &str =
    "15c2d007954ac53ba69b80ec91242786b3c0b71d52649165b4ca1d5cc96ef8f1";
const EXPECTED_EXECUTABLE_SIZE: u64 = 35_613_696;

/// Locate and verify the packaged, repository-controlled Xray sidecar. Unlike sing-box,
/// Xray is never downloaded or selected through a lossy config conversion.
pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
    let names = candidate_names();
    if let Ok(path) = std::env::var("XRAY_BIN") {
        let path = PathBuf::from(path);
        if path.exists() {
            return verify_binary(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(path) = first_existing(dir, &names) {
                return verify_binary(path);
            }
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        if let Some(path) = first_existing(&resource_dir.join("binaries"), &names) {
            return verify_binary(path);
        }
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let mut cursor = Some(PathBuf::from(manifest));
        for _ in 0..4 {
            let Some(dir) = cursor.take() else { break };
            if let Some(path) = first_existing(&dir.join("binaries"), &names) {
                return verify_binary(path);
            }
            cursor = dir.parent().map(Path::to_path_buf);
        }
    }
    Err(AppError::BinaryNotFound(
        "Xray-core sidecar is unavailable".to_string(),
    ))
}

fn verify_binary(path: PathBuf) -> AppResult<PathBuf> {
    let bytes = fs::read(&path)?;
    if bytes.len() as u64 != EXPECTED_EXECUTABLE_SIZE {
        return Err(AppError::BinaryNotFound(
            "Xray-core sidecar integrity check failed".into(),
        ));
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != EXPECTED_EXECUTABLE_SHA256 {
        return Err(AppError::BinaryNotFound(
            "Xray-core sidecar integrity check failed".into(),
        ));
    }
    Ok(path)
}

pub async fn validate_config(binary: &Path, config_path: &Path) -> AppResult<()> {
    let mut command = Command::new(binary);
    command.args(validation_args(config_path));
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .await
        .map_err(|_| AppError::Spawn("Xray config validation could not start".into()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Validation("Xray config validation failed".into()))
    }
}

pub fn validation_args(config_path: &Path) -> [std::ffi::OsString; 4] {
    [
        "run".into(),
        "-test".into(),
        "-config".into(),
        config_path.as_os_str().to_os_string(),
    ]
}

pub fn run_args(config_path: &Path) -> [std::ffi::OsString; 3] {
    [
        "run".into(),
        "-config".into(),
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

    fn argument_strings(args: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
        args.into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn xray_validation_uses_test_config_command() {
        let args = argument_strings(validation_args(Path::new("runtime.json")));
        assert_eq!(args, vec!["run", "-test", "-config", "runtime.json"]);
    }

    #[test]
    fn xray_launch_uses_original_runtime_config_path() {
        let args = argument_strings(run_args(Path::new("runtime.json")));
        assert_eq!(args, vec!["run", "-config", "runtime.json"]);
    }

    #[test]
    fn resolver_has_targeted_sidecar_names() {
        let names = candidate_names();
        assert!(names.iter().any(|name| name.starts_with("xray-")));
        assert!(names.iter().any(|name| name.starts_with("xray")));
    }
}
