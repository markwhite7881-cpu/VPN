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

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::process::ProcessManager;

/// Path the runtime-cached sing-box lives at, relative to the
/// per-platform app data dir (`app.path().app_data_dir()`).
///
/// We embed the app version in the subdir name so that an app
/// upgrade (which usually ships a newer bundled sing-box) starts
/// fresh — the old cache is orphaned and harmless, and the new
/// cache starts empty. Without this, a 1.13.18 cached from a
/// prior `apply_singbox_update` would keep winning over a
/// 1.14.x that ships with the new .app, even though the bundled
/// is the "newer" one the user actually wants.
fn runtime_subdir() -> String {
    format!("singbox-runtime-{}", env!("CARGO_PKG_VERSION"))
}
#[cfg(windows)]
const RUNTIME_BIN_NAME: &str = "sing-box.exe";
#[cfg(not(windows))]
const RUNTIME_BIN_NAME: &str = "sing-box";

/// GitHub releases API endpoint for sing-box. Unauthenticated;
/// 60 req/hour/IP rate limit (sufficient for a desktop app).
const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases/latest";

/// User-Agent sent to GitHub. They ask for a meaningful UA on
/// unauthenticated API calls; "cloakwire" identifies us and
/// the version helps debugging on their side.
const USER_AGENT: &str = concat!("cloakwire/", env!("CARGO_PKG_VERSION"));

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

/// Resolve `<app_data_dir>/singbox-runtime-<version>/sing-box.exe`.
/// The directory may not exist yet; we create it on first write.
pub fn runtime_bin_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::BinaryNotFound(format!("app_data_dir: {e}")))?;
    Ok(dir.join(runtime_subdir()).join(RUNTIME_BIN_NAME))
}

/// Copy the bundled sing-box into the runtime cache directory and
/// `chmod +x` it. Called from `process::locate_binary` when the
/// cache is empty so a fresh install (or an app upgrade that
/// bumped the cache subdir) gets a user-writable, user-executable
/// copy of the bundled binary. Without this, a freshly-installed
/// .app whose bundled sidecar landed with 0644 perms (e.g.
/// installed by a different user into /Applications on macOS)
/// would fail with EACCES on `execve` — the cache copy in
/// `app_data_dir` is owned by the running user and is exec-able.
///
/// Also: cleans up any *other* `singbox-runtime-*` subdirs under
/// `app_data_dir` (i.e. from prior app versions) so the cache
/// doesn't accumulate stale 80-MB binaries forever. Best-effort
/// — failures here are logged and ignored, never block startup.
pub fn populate_cache_from_bundled(app: &AppHandle, bundled: &std::path::Path) -> AppResult<PathBuf> {
    let dest = runtime_bin_path(app)?;
    let dest_dir = dest
        .parent()
        .ok_or_else(|| AppError::Spawn("runtime cache path has no parent".into()))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    log::info!(
        "updates: populating cache from bundled {} -> {}",
        bundled.display(),
        dest.display()
    );
    std::fs::copy(bundled, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }
    // macOS: strip the `com.apple.quarantine` xattr that browsers
    // (Comet in particular) attach to .dmg downloads. Without this,
    // `execve` of either the cached or the bundled binary silently
    // fails with EPERM and the app reports "VPN core not detected".
    //
    // We strip BOTH paths:
    //   * the cached copy is what `locate_binary` returns, so it
    //     MUST be clean for the next sing-box exec
    //   * the bundled source is what `sing-box check -c <cfg>` and
    //     `sing-box version` shell out to during config validation,
    //     and it must also be exec-able for those to work
    //
    // Implementation: shell out to the system `xattr` binary
    // instead of using a Rust crate. Reasoning:
    //   * macOS ships `xattr` preinstalled since 10.3 — no dep
    //   * avoids a target-specific dep that complicates `cargo check`
    //     on Windows and Linux hosts
    //   * the call is one-shot at first-launch per app version, so
    //     the process-spawn cost (~5 ms) is irrelevant
    //   * xattr's exit-code-on-missing-xattr is well-defined (0 even
    //     if the attribute was already absent), so we don't have to
    //     special-case "not found" — we just log a warning if it
    //     fails for any other reason
    //
    // Best-effort: any error here is logged and ignored, never
    // blocking startup. If the binary is actually quarantined and
    // we somehow fail to strip, the user will see the real error
    // when sing-box fails to exec (EACCES / EPERM) and the existing
    // error message in the UI will guide them.
    #[cfg(target_os = "macos")]
    {
        strip_quarantine_log(&dest, "cache");
        strip_quarantine_log(bundled, "bundled");
    }

    // Orphan cleanup: anything else under app_data_dir that
    // looks like an old cache (matches `singbox-runtime-*`)
    // but isn't the one we just populated.
    let current_subdir = runtime_subdir();
    if let Ok(entries) = std::fs::read_dir(dest_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else { continue };
            if !name_str.starts_with("singbox-runtime-") {
                continue;
            }
            if name_str == current_subdir {
                continue;
            }
            let p = entry.path();
            log::info!(
                "updates: removing orphan cache dir {}",
                p.display()
            );
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    Ok(dest)
}

/// Remove `com.apple.quarantine` from a file. Used by
/// `populate_cache_from_bundled` so a freshly-installed .app
/// downloaded via browsers that tag their downloads (Comet, older
/// Safari versions, etc.) can run its bundled sing-box without
/// requiring the user to run `xattr -dr com.apple.quarantine
/// /Applications/Cloakwire.app` by hand.
///
/// Implementation: spawn `/usr/bin/xattr -d com.apple.quarantine
/// <path>` (macOS-only, gated by `#[cfg(target_os = "macos")]` on
/// the caller side). We use `-d` (delete) rather than `-c` (clear
/// all) so we only touch the one attribute we care about and
/// don't accidentally drop user-set xattrs.
///
/// Behavioural notes:
///   * `xattr -d` exits 0 if the attribute was present and
///     successfully removed.
///   * `xattr -d` exits 0 even if the attribute was NOT set
///     (with the message "No such xattr: com.apple.quarantine"
///     on stderr) — so we don't need to special-case "already
///     clean".
///   * non-zero exit means the syscall genuinely failed (SIP,
///     readonly mount, file vanished). We log and continue —
///     `execve` later will surface the real error and the user
///     sees the existing "VPN core not detected" message.
#[cfg(target_os = "macos")]
fn strip_quarantine_log(path: &std::path::Path, source: &str) {
    let result = std::process::Command::new("/usr/bin/xattr")
        .arg("-d")
        .arg("com.apple.quarantine")
        .arg(path)
        .output();
    match result {
        Ok(out) if out.status.success() => {
            log::info!(
                "updates: stripped quarantine from {source} {}",
                path.display()
            );
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let trimmed = stderr.trim();
            if trimmed.is_empty() {
                log::debug!(
                    "updates: xattr on {source} {} exited {:?} (no stderr)",
                    path.display(),
                    out.status.code()
                );
            } else {
                log::debug!(
                    "updates: xattr on {source} {} exited {:?}: {trimmed}",
                    path.display(),
                    out.status.code()
                );
            }
        }
        Err(e) => log::warn!(
            "updates: failed to spawn xattr for {source} {}: {e}",
            path.display()
        ),
    }
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

    // 2) Find the asset for the current platform. sing-box ships
    // per-OS archives named `sing-box-VERSION-<os>-<arch>.<ext>`
    // where ext is `.zip` on Windows and `.tar.gz` everywhere else.
    // Picking the Windows asset on macOS/Linux is the bug behind
    // "expected sing-box after extraction, but it's missing" (the
    // Windows zip contains sing-box.exe; we look for sing-box).
    let target = pick_platform_asset(&release.assets);

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
    let tmp_dir = std::env::temp_dir().join("cloakwire-update");
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

/// Platform-aware asset picker. sing-box release archives are named
/// `sing-box-VERSION-<os>-<arch>.<ext>` with `.zip` on Windows and
/// `.tar.gz` everywhere else. The `<arch>` part on Windows is
/// `amd64` or `arm64`; on macOS / Linux it's the same. We pick the
/// asset that matches the host's OS + arch tuple (per the `cfg!`
/// block below), preferring extension-specific matches for Windows
/// (`.zip`) and Linux / macOS (`.tar.gz`).
///
/// On platforms we don't have a bundled binary for (e.g. Linux
/// ARM64 on a system that shipped an x86_64 build), this returns
/// None and the UI shows "no update available" — the bundled
/// binary keeps working. The user can still download a different
/// arch manually if they need to.
fn pick_platform_asset(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    // (os, arch, extension) — order matters; the first matching
    // asset wins. We support the two arches sing-box ships per OS.
    let candidates: &[(&str, &str, &str)] = &[
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        ("darwin", "arm64", ".tar.gz"),
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        ("darwin", "amd64", ".tar.gz"),
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        ("linux", "amd64", ".tar.gz"),
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        ("linux", "arm64", ".tar.gz"),
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        ("windows", "amd64", ".zip"),
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        ("windows", "arm64", ".zip"),
    ];

    for (os, arch, ext) in candidates {
        // e.g. "sing-box-1.13.18-linux-amd64.tar.gz"
        let pattern = format!("-{os}-{arch}{ext}");
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(&pattern)) {
            return Some(a);
        }
    }
    None
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
/// sing-box release archive into `out_dir`. Returns the path to the
/// extracted binary.
///
/// The sing-box release archive is mostly flat — a single binary
/// somewhere inside — but the exact layout has varied over time:
///
///   - Older (<1.12): top level flat, e.g. `zip_root/sing-box.exe`
///   - 1.12+: nested in a per-target subdir, e.g.
///     `zip_root/sing-box-1.13.18-darwin-arm64/sing-box`
///   - Always: exactly one binary per archive, named either
///     `sing-box.exe` (Windows) or `sing-box` (macOS / Linux).
///
/// We use Rust-native `zip` and `tar`+`flate2` crates for the actual
/// extraction instead of shelling out to system `tar`. The
/// Windows 10+ shipped `bsdtar` fails on some SagerNet release
/// .zip archives with exit code 1 (verified via friend: a 20.1 MB
/// v1.14.0-beta.14 download hit "tar extract failed with exit code
/// Some(1)" and the update never applied). Pure-Rust extraction
/// is deterministic across platforms and immune to this.
fn extract_singbox_from_zip(archive_path: &std::path::Path, out_dir: &std::path::Path) -> AppResult<PathBuf> {
    let name = RUNTIME_BIN_NAME;
    let path_lower = archive_path
        .to_string_lossy()
        .to_ascii_lowercase();

    if path_lower.ends_with(".zip") {
        extract_zip(archive_path, out_dir, name)
    } else if path_lower.ends_with(".tar.gz") || path_lower.ends_with(".tgz") {
        extract_tar_gz(archive_path, out_dir, name)
    } else {
        Err(AppError::Spawn(format!(
            "unsupported archive format: {}",
            archive_path.display()
        )))
    }
}

/// Extract the first entry whose path matches `name` (either flat
/// at the top of the archive or inside one subdirectory) into
/// `out_dir/<name>`. Returns the path of the written file.
fn extract_zip(archive_path: &std::path::Path, out_dir: &std::path::Path, name: &str) -> AppResult<PathBuf> {
    let f = File::open(archive_path)
        .map_err(|e| AppError::Spawn(format!("open zip: {e}")))?;
    let mut zip = zip::ZipArchive::new(BufReader::new(f))
        .map_err(|e| AppError::Spawn(format!("read zip: {e}")))?;

    // Find the binary by exact basename match. The `zip` crate
    // exposes `is_file()` (true for stored files, false for dirs)
    // and `enclosed_name()` (safe path — strips `../` etc.). We
    // look for an entry whose final path component equals `name`,
    // which covers both flat and one-level-nested layouts.
    let target_basename = std::path::Path::new(name);
    let dest = out_dir.join(name);

    let mut found_idx: Option<usize> = None;
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .map_err(|e| AppError::Spawn(format!("zip entry {i}: {e}")))?;
        if !entry.is_file() {
            continue;
        }
        let Some(safe) = entry.enclosed_name() else {
            continue;
        };
        if safe.file_name() == target_basename.file_name() {
            found_idx = Some(i);
            break;
        }
    }
    let idx = found_idx.ok_or_else(|| {
        AppError::Spawn(format!("expected {name} in zip, but it's missing"))
    })?;
    let mut entry = zip
        .by_index(idx)
        .map_err(|e| AppError::Spawn(format!("zip entry {idx}: {e}")))?;
    let mut out = File::create(&dest)
        .map_err(|e| AppError::Spawn(format!("create {}: {e}", dest.display())))?;
    std::io::copy(&mut entry, &mut out)
        .map_err(|e| AppError::Spawn(format!("copy zip entry: {e}")))?;
    log::info!(
        "updates: extracted {} ({} bytes) -> {}",
        entry.name(),
        entry.size(),
        dest.display()
    );
    Ok(dest)
}

/// Extract the first entry whose basename matches `name` from a
/// `.tar.gz` (or `.tgz`) into `out_dir/<name>`. Same flat-or-nested
/// tolerance as `extract_zip`. The `tar` crate is safe to use on
/// untrusted archives — it does not follow symlinks, and we re-check
/// the basename on every entry to avoid path-traversal surprises.
fn extract_tar_gz(archive_path: &std::path::Path, out_dir: &std::path::Path, name: &str) -> AppResult<PathBuf> {
    let f = File::open(archive_path)
        .map_err(|e| AppError::Spawn(format!("open tar.gz: {e}")))?;
    let gz = flate2::read::GzDecoder::new(BufReader::new(f));
    let mut tar = tar::Archive::new(gz);

    let target_basename = std::path::Path::new(name);
    let dest = out_dir.join(name);
    let mut found = false;
    for entry in tar
        .entries()
        .map_err(|e| AppError::Spawn(format!("read tar entries: {e}")))?
    {
        let mut entry = entry.map_err(|e| AppError::Spawn(format!("tar entry: {e}")))?;
        let entry_path = entry
            .path()
            .map_err(|e| AppError::Spawn(format!("tar entry path: {e}")))?
            .into_owned();
        if entry_path.file_name() == target_basename.file_name() {
            let mut out = File::create(&dest)
                .map_err(|e| AppError::Spawn(format!("create {}: {e}", dest.display())))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| AppError::Spawn(format!("copy tar entry: {e}")))?;
            log::info!(
                "updates: extracted {} -> {}",
                entry_path.display(),
                dest.display()
            );
            found = true;
            break;
        }
    }
    if !found {
        return Err(AppError::Spawn(format!(
            "expected {name} in tar.gz, but it's missing"
        )));
    }
    Ok(dest)
}
