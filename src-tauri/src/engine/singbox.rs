use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

pub fn locate_binary(app: &AppHandle) -> AppResult<PathBuf> {
    if let Ok(p) = std::env::var("SINGBOX_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(p) = crate::updates::runtime_bin_path(app) {
        if p.exists() {
            return Ok(p);
        }
    }

    let triple = current_target_triple();
    let exe_name = if cfg!(windows) {
        format!("sing-box-{triple}.exe")
    } else {
        format!("sing-box-{triple}")
    };
    let plain_name = if cfg!(windows) {
        "sing-box.exe".to_string()
    } else {
        "sing-box".to_string()
    };

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [&exe_name, &plain_name] {
                let p = dir.join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        for name in [&exe_name, &plain_name] {
            let p = resource_dir.join("binaries").join(name);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let manifest = PathBuf::from(manifest);
        let mut cursor: Option<&Path> = Some(manifest.as_path());
        for _ in 0..4 {
            let Some(dir) = cursor else { break };
            for name in [&exe_name, &plain_name] {
                let p = dir.join("binaries").join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
            cursor = dir.parent();
        }
    }

    Err(AppError::BinaryNotFound(format!(
        "expected one of: {exe_name}, {plain_name}"
    )))
}

pub fn check_args(config_path: &Path) -> [&std::ffi::OsStr; 3] {
    ["check".as_ref(), "-c".as_ref(), config_path.as_os_str()]
}

pub fn parse_version(stdout: &str) -> (String, String, String) {
    let mut version = String::new();
    let mut environment = String::new();
    let mut revision = String::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("sing-box version ") {
            version = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Environment: ") {
            environment = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Revision: ") {
            revision = rest.trim().to_string();
        }
    }
    (version, environment, revision)
}

fn current_target_triple() -> &'static str {
    option_env!("CARGO_CFG_TARGET_TRIPLE").unwrap_or("x86_64-pc-windows-msvc")
}
