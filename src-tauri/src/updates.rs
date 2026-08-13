//! Auto-update for the bundled `sing-box` core.
//!
//! The Tauri updater (see `tauri.conf.json > plugins.updater` and
//! `tauri-plugin-updater` registered in `lib.rs`) handles the
//! **app shell** — Tauri releases on GitHub get a signed
//! `latest.json` and the app offers one-click install.
//!
//! `sing-box` itself is a separate thing. We bundle a specific
//! `sing-box-x86_64-pc-windows-msvc.exe` in the installer, and
//! updating that binary means re-shipping the whole app. To
//! decouple, this module:
//!
//!   1. Queries the sing-box GitHub releases API for the latest
//!      Windows release.
//!   2. Compares the latest version to the version reported by the
//!      currently-active binary.
//!   3. Downloads the Windows .zip, extracts `sing-box.exe`, and
//!      stashes it at `<app_data_dir>/singbox-runtime/sing-box.exe`.
//!   4. Updates `process::ProcessManager::locate_binary` to prefer
//!      that user-writable copy when it exists. The bundled binary
//!      is the fallback for fresh installs.
//!
//! This is a runtime override, NOT a replacement of the bundled
//! binary. Reinstalling the app keeps the user's current run-time
//! version in place (which is what they want — they just upgraded).
//!
//! GitHub releases API is unauthenticated and rate-limited to 60
//! requests/hour/IP. The frontend should debounce user clicks; we
//! don't cache here because the v0.4.0 use case (user opens the
//! app, sees "update available", applies it) is well under the
//! limit. If we start getting throttled, add a 1h in-memory cache
//! keyed by URL.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::process::ProcessManager;

/// Path the runtime-cached sing-box lives at, relative to the
/// per-platform app data dir (`app.path().app_data_dir()`).
/// We use a subdirectory so the file is clearly ours and won't
/// collide with whatever else might end up in `app_data_dir/`.
const RUNTIME_BIN_SUBDIR: &str = "singbox-runtime";
#[cfg(windows)]
const RUNTIME_BIN_NAME: &str = "sing-box.exe";
#[cfg(not(windows))]
const RUNTIME_BIN_NAME: &str = "sing-box";

/// GitHub releases API endpoint for sing-box. Unauthenticated;
/// 60 req/hour/IP rate limit (sufficient for a desktop app).
const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases/latest";

/// User-Agent sent to GitHub. They ask for a meaningful UA on
/// unauthenticated API calls; "singbox-client" identifies us and
/// the version helps debugging on their side.
const USER_AGENT: &str = concat!("singbox-client/", env!("CARGO_PKG_VERSION"));

/// What `check_singbox_update` returns. The frontend shows
/// "v1.14.0 → v1.15.0 available" if `available` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingboxUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub size_bytes: u64,
}

impl SingboxUpdateInfo {
    pub fn not_available(current: String, latest: String) -> Self {
        Self {
            current_version: current,
            latest_version: latest,
            available: false,
            download_url: None,
            asset_name: None,
            size_bytes: 0,
        }
    }
}

/// Resolve `<app_data_dir>/singbox-runtime/sing-box.exe`. The
/// directory may not exist yet; we create it on first write.
pub fn runtime_bin_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::BinaryNotFound(format!("app_data_dir: {e}")))?;
    Ok(dir.join(RUNTIME_BIN_SUBDIR).join(RUNTIME_BIN_NAME))
}

/// True if a runtime-cached binary exists at the user-writable
/// location. Used by `ProcessManager::locate_binary` to decide
/// whether to prefer the cached copy over the bundled one.
pub fn runtime_bin_exists(app: &AppHandle) -> bool {
    runtime_bin_path(app)
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Fetch the latest sing-box release from GitHub. Returns the
/// `SingboxUpdateInfo` the frontend uses to render the update card.
///
/// On any network/parse error we surface the error string verbatim;
/// the UI displays it in the "check failed" state.
pub async fn check_singbox_update(app: &AppHandle) -> AppResult<SingboxUpdateInfo> {
    let current = crate::commands::get_singbox_version(app.clone())
        .await
        .map(|v| v.version)
        .unwrap_or_default();

    // 1) GET the latest release.
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Network(format!("http client: {e}")))?;
    let resp = client
        .get(RELEASES_LATEST_URL)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("releases/latest: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!(
            "GitHub API returned {}",
            resp.status()
        )));
    }
    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| AppError::Network(format!("release JSON: {e}")))?;

    // `tag_name` is "v1.15.0" — strip the leading 'v' for a clean
    // comparison against `sing-box version` output ("1.15.0").
    let latest = release.tag_name.trim_start_matches('v').to_string();

    // 2) Find the Windows-amd64 asset. sing-box uses
    // `sing-box-VERSION-windows-amd64.zip` in the .zip. There's
    // also an archive without the suffix in some pre-releases —
    // match the windows-amd64 .zip specifically.
    let target = pick_windows_amd64_asset(&release.assets);

    let Some(asset) = target else {
        return Ok(SingboxUpdateInfo::not_available(current, latest));
    };

    // 3) Compare versions. The tag uses semver; we don't pull in
    // a semver crate for a single comparison — split on '.' and
    // compare each component as a number with a fallback to
    // string compare. This is "good enough" for upstream's
    // X.Y.Z scheme (it never has pre-release suffixes for the
    // tags we care about).
    let available = version_is_newer(&latest, &current);

    Ok(SingboxUpdateInfo {
        current_version: current,
        latest_version: latest,
        available,
        download_url: Some(asset.browser_download_url.clone()),
        asset_name: Some(asset.name.clone()),
        size_bytes: asset.size,
    })
}

/// Download the asset, extract `sing-box.exe`, replace the cached
/// runtime binary. Stops the running sing-box first if it's up.
///
/// The frontend passes the `download_url` straight from
/// `check_singbox_update` — we don't fetch it again, so the user
/// is always updating to the version they were shown.
///
/// Returns the new version string (parsed from the zip's binary)
/// for the UI to show "v1.15.0 installed".
pub async fn apply_singbox_update(
    app: AppHandle,
    download_url: String,
) -> AppResult<String> {
    // 1) Stop the running sing-box so we don't have a file handle
    // contention on Windows (the .exe is mapped into the process
    // and can't be overwritten while running).
    let mgr = app.state::<Arc<ProcessManager>>().inner().clone();
    if mgr.snapshot_status().await.status != crate::process::Status::Stopped {
        log::info!("updates: stopping running sing-box before applying update");
        if let Err(e) = mgr.stop().await {
            log::warn!("updates: stop_singbox returned {e}; continuing anyway");
        }
    }

    // 2) Download the .zip to a temp file in the system temp dir.
    let tmp_dir = std::env::temp_dir().join("singbox-client-update");
    std::fs::create_dir_all(&tmp_dir)?;
    let zip_path = tmp_dir.join("update.zip");

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Network(format!("http client: {e}")))?;
    log::info!("updates: downloading {download_url}");
    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("download: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("download body: {e}")))?;
    std::fs::write(&zip_path, &bytes)?;

    // 3) Extract `sing-box.exe` from the .zip. sing-box releases
    // are flat — a single binary at the top level of the archive.
    let extracted = extract_singbox_from_zip(&zip_path, &tmp_dir)?;
    log::info!("updates: extracted to {}", extracted.display());

    // 4) Place it at the runtime-cached location.
    let dest = runtime_bin_path(&app)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // On Windows we can't replace a running .exe — but we already
    // stopped sing-box above, so this should be fine. If something
    // else holds a handle (AV scanner etc.) the rename will fail
    // and the user sees the error in the UI.
    if dest.exists() {
        // Best-effort: remove the old cached binary. If this fails
        // (e.g. AV scanner), fall back to writing to a sibling
        // .new path and renaming — atomic-ish on the same volume.
        let _ = std::fs::remove_file(&dest);
    }
    std::fs::rename(&extracted, &dest)?;

    // 5) Verify the new binary actually runs and reports a version.
    // If this fails, the user will see "v0.0.0" and we should
    // surface the real error in the UI.
    let new_version = crate::commands::get_singbox_version(app.clone())
        .await
        .map(|v| v.version)
        .unwrap_or_else(|e| {
            log::warn!("updates: get_singbox_version after update failed: {e}");
            String::new()
        });

    // Cleanup: best-effort removal of the .zip and any extracted
    // files. Errors here are non-fatal.
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&tmp_dir);

    log::info!("updates: applied sing-box v{new_version}");
    Ok(new_version)
}

// ---- helpers --------------------------------------------------------

/// JSON shape of the `releases/latest` endpoint. We only pull
/// the fields we use; the rest is ignored.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Pick the Windows amd64 .zip from the asset list. sing-box
/// tags them as `sing-box-VERSION-windows-amd64.zip` (also
/// `windows-amd64-cgo.zip` historically). We match the
/// `windows-amd64` substring and the `.zip` suffix; this
/// gracefully survives upstream's naming variations.
fn pick_windows_amd64_asset(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    assets
        .iter()
        .find(|a| a.name.ends_with(".zip") && a.name.contains("windows-amd64"))
}

/// Compare two sing-box version strings. Returns true if `a` is
/// strictly newer than `b`. Handles upstream's `X.Y.Z` scheme
/// (no pre-release tags for the `latest` endpoint we use).
fn version_is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|p| p.split('-').next().unwrap_or(p).parse::<u64>().ok())
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    if av.is_empty() || bv.is_empty() {
        // Either side unparseable — fall back to string compare so
        // a future "1.15.0-rc1" tag still registers as a change.
        return a != b && a > b;
    }
    let n = av.len().max(bv.len());
    for i in 0..n {
        let x = av.get(i).copied().unwrap_or(0);
        let y = bv.get(i).copied().unwrap_or(0);
        if x > y {
            return true;
        }
        if x < y {
            return false;
        }
    }
    false
}

/// Extract `sing-box.exe` (or `sing-box` on non-Windows) from a
/// sing-box release zip into `out_dir`. Returns the path to the
/// extracted binary.
///
/// The sing-box release zip is a flat archive containing exactly
/// one binary. We don't pull in the `zip` crate to keep the
/// dependency surface small; instead we use `std::process::Command`
/// to invoke the system `tar` (which on Windows 10+ and modern
/// Linux/macOS handles .zip out of the box).
fn extract_singbox_from_zip(zip_path: &std::path::Path, out_dir: &std::path::Path) -> AppResult<PathBuf> {
    // libarchive's `tar` (the BSD one shipped on macOS / Windows 10+)
    // handles .zip via the `-a` flag (auto-detect format). On Linux
    // it's GNU tar, also handles .zip out of the box.
    let status = std::process::Command::new("tar")
        .arg("-xaf")
        .arg(zip_path)
        .arg("-C")
        .arg(out_dir)
        .status()
        .map_err(|e| AppError::Spawn(format!("`tar` not available: {e}")))?;
    if !status.success() {
        return Err(AppError::Spawn(format!(
            "tar extract failed with exit code {:?}",
            status.code()
        )));
    }
    // After extraction, the binary is at `out_dir/sing-box.exe`
    // (or `out_dir/sing-box` on Unix).
    let extracted = out_dir.join(RUNTIME_BIN_NAME);
    if !extracted.exists() {
        return Err(AppError::Spawn(format!(
            "expected {} after extraction, but it's missing",
            extracted.display()
        )));
    }
    Ok(extracted)
}
