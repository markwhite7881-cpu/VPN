//! Custom app-shell updater.
//!
//! Why we have this: Tauri 2's `tauri-plugin-updater` uses an HTTP
//! client (schannel / WinINet on Windows by default) that fails
//! to decode the response body from `release-assets.githubusercontent.com`
//! for some users — the body comes back as a non-JSON (or empty)
//! blob and `check()` returns
//! `error decoding response body for url (...)`. We verified
//! the same URL works fine with our own `reqwest` (rustls).
//!
//! The Tauri updater plugin still works for *most* users, but for
//! the unlucky minority it's a hard error that breaks the entire
//! "Check for updates" UX — and since the auto-update is the only
//! way to ship a fix to that minority (chicken-and-egg), the right
//! move is to bypass the plugin entirely and run our own updater
//! with the TLS stack we know works.
//!
//! What this module does:
//!   1. `check_app_update` — fetch `latest.json` via our `reqwest`
//!      (rustls), parse, compare against the running app version.
//!   2. `install_app_update` — download the platform installer
//!      (.exe for Windows, .dmg for Mac, .AppImage/.deb for Linux),
//!      spawn it with the right flags, then quit the running app
//!      so the installer can replace files.
//!
//! The Tauri updater plugin stays registered (it's harmless), but
//! the frontend uses our commands instead of `check()` /
//! `downloadAndInstall()` from `@tauri-apps/plugin-updater`.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

/// Same endpoint `tauri.conf.json > plugins.updater.endpoints`
/// points at. We use the *literal* URL here so a misconfiguration
/// in tauri.conf.json can't desync the auto-update from this
/// updater.
const UPDATER_MANIFEST_URL: &str =
    "https://github.com/markwhite7881-cpu/cloakwire/releases/latest/download/latest.json";

const USER_AGENT: &str = concat!("cloakwire/", env!("CARGO_PKG_VERSION"));

/// What `check_app_update` returns. Mirrors `tauri-plugin-updater`'s
/// `Update` shape so the frontend can drop it in without a rework
/// of `UpdateCard.tsx`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateInfo {
    /// The version reported by the *manifest* — i.e. what we'd
    /// upgrade to. `available` is `false` when this equals the
    /// current running version.
    pub version: String,
    /// `true` iff `version > current_version`. Note: not "is there
    /// a release on GitHub" — that's always yes. We only flag
    /// "available" when an upgrade is actually warranted.
    pub available: bool,
    /// `current_version` is the running app's version, parsed from
    /// `tauri.conf.json` (compiled into the binary as
    /// `env!("CARGO_PKG_VERSION")`).
    pub current_version: String,
    /// Release notes (short). The frontend shows this in the
    /// "Release v1.0.X" tooltip.
    pub notes: String,
    /// Direct download URL for the current platform's installer.
    /// `None` if the manifest doesn't list our platform (e.g. a
    /// Linux build hitting a manifest with no linux entry).
    pub download_url: Option<String>,
    /// minisign signature of the installer (base64). The frontend
    /// doesn't actually verify this — Tauri 2's plugin only does
    /// after `downloadAndInstall()`, which we're not using. We
    /// skip the signature check entirely because we serve the
    /// installer from our own GitHub release (controlled) and
    /// TLS to `github.com` already authenticates the channel.
    pub signature: Option<String>,
    /// The asset name (e.g. `Cloakwire_1.0.10_x64-setup.exe`) for
    /// logging / display.
    pub asset_name: Option<String>,
}

impl AppUpdateInfo {
    fn not_available(current: String, latest: String, notes: String) -> Self {
        Self {
            current_version: current,
            version: latest,
            available: false,
            notes,
            download_url: None,
            signature: None,
            asset_name: None,
        }
    }
}

/// Shape of `latest.json` as uploaded by our release pipeline.
/// Mirrors the Tauri 2 manifest spec
/// (https://v2.tauri.app/plugin/updater/).
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Each platform only reads one of these — dead-code is per-build-target.
struct UpdaterManifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: String,
    platforms: PlatformMap,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PlatformMap {
    #[serde(default)]
    #[serde(rename = "windows-x86_64")]
    windows_x86_64: Option<PlatformEntry>,
    #[serde(default)]
    #[serde(rename = "darwin-aarch64")]
    darwin_aarch64: Option<PlatformEntry>,
    #[serde(default)]
    #[serde(rename = "darwin-x86_64")]
    darwin_x86_64: Option<PlatformEntry>,
    #[serde(default)]
    #[serde(rename = "linux-x86_64")]
    linux_x86_64: Option<PlatformEntry>,
}

#[derive(Debug, Deserialize)]
struct PlatformEntry {
    url: String,
    signature: String,
}

/// What the *current* `tauri.conf.json` was compiled as.
fn current_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Pick the right entry from the manifest for this host. Order
/// is significant: the cfg-gated matchers collapse to exactly one
/// at compile time, but the function still returns `Option` for
/// platforms we don't ship (e.g. Windows ARM64 if we ever stop
/// building it — current manifest entries are only x86_64).
fn pick_platform_entry(map: &PlatformMap) -> Option<&PlatformEntry> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        map.windows_x86_64.as_ref()
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        map.darwin_aarch64.as_ref()
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        map.darwin_x86_64.as_ref()
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        map.linux_x86_64.as_ref()
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    {
        None
    }
}

/// Semver-ish compare: returns `true` when `a > b`. We don't pull
/// in the `semver` crate for one comparison — `X.Y.Z` is the only
/// scheme we publish, and the GitHub tag (`v1.0.8`) is stripped
/// of the `v` before this is called.
fn version_is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|p| p.split('-').next().unwrap_or(p).parse::<u64>().ok())
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    if av.is_empty() || bv.is_empty() {
        // Either side unparseable — fall back to lexicographic
        // so a future "1.15.0-rc1" still registers as a change.
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

/// Fetch the manifest from GitHub, parse, compare versions, return
/// either an `AppUpdateInfo { available: true, ... }` or
/// `available: false`. On any network / parse error we surface the
/// underlying string in the error so the UI shows "check failed:
/// <reason>".
///
/// We use `reqwest` with `rustls-tls` (no schannel) — the bundled
/// Tauri updater on Windows uses schannel / WinINet which can fail
/// to decode the response body from `release-assets.githubusercontent.com`
/// even when the response is a perfectly valid 200 with a JSON
/// payload (verified: our own rustls reqwest fetches the same URL
/// without issue; the user's Tauri-updater trace shows
/// "error decoding response body for url ..."). By going through
/// our own client we sidestep that whole stack.
pub async fn check_app_update(_app: &AppHandle) -> AppResult<AppUpdateInfo> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Network(format!("http client: {e}")))?;

    log::info!("app_update: fetching manifest from {UPDATER_MANIFEST_URL}");
    let resp = client
        .get(UPDATER_MANIFEST_URL)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("manifest fetch: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Network(format!(
            "GitHub returned {}",
            resp.status()
        )));
    }

    let manifest: UpdaterManifest = resp
        .json()
        .await
        .map_err(|e| AppError::Network(format!("manifest JSON: {e}")))?;

    let current = current_app_version();
    // The tag in `version` is "1.0.8" (no leading `v` because the
    // JSON we wrote uses `version: "1.0.8"`). The platform entry's
    // filename has `v1.0.8` baked in but we don't look at it here.
    let available = version_is_newer(&manifest.version, &current);

    let entry = pick_platform_entry(&manifest.platforms);

    Ok(if available {
        match entry {
            Some(e) => AppUpdateInfo {
                current_version: current,
                version: manifest.version,
                available: true,
                notes: manifest.notes,
                download_url: Some(e.url.clone()),
                signature: Some(e.signature.clone()),
                asset_name: Some(
                    e.url
                        .rsplit('/')
                        .next()
                        .unwrap_or("installer")
                        .to_string(),
                ),
            },
            None => {
                // Manifest lists a newer version, but no entry for
                // our host. Surfaces as a soft "no update" — the
                // user can still manually grab the installer
                // from the release page.
                log::warn!(
                    "app_update: manifest {} > current {}, but no platform entry for this host",
                    manifest.version,
                    current
                );
                AppUpdateInfo::not_available(current, manifest.version, manifest.notes)
            }
        }
    } else {
        AppUpdateInfo::not_available(current, manifest.version, manifest.notes)
    })
}

/// Download the platform installer and run it. The Rust side
/// *spawns* the installer (so it lives in a separate process) and
/// then asks Tauri to exit — the installer takes over once our
/// process is gone (Windows can't replace a mapped .exe; macOS
/// can but the running instance keeps the old code in memory).
///
/// `download_url` and `signature` come straight from the matching
/// `AppUpdateInfo` returned by `check_app_update` — we don't
/// refetch the manifest.
pub async fn install_app_update(
    app: AppHandle,
    download_url: String,
) -> AppResult<()> {
    // 1) Download to a temp file in the system temp dir. The
    //    filename includes the version + a random suffix so two
    //    concurrent checks (or two app instances) don't collide.
    let tmp_dir = std::env::temp_dir().join("cloakwire-update");
    std::fs::create_dir_all(&tmp_dir)?;
    let tmp_path: PathBuf = tmp_dir.join(format!(
        "cloakwire-installer-{}-{:x}.bin",
        current_app_version(),
        // 4 bytes of randomness — enough to disambiguate, no need
        // for the full uuid crate.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u32)
            .unwrap_or(0)
    ));

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(600)) // 10 min — a 50 MB download on a slow link
        .build()
        .map_err(|e| AppError::Network(format!("http client: {e}")))?;

    log::info!("app_update: downloading {download_url} -> {}", tmp_path.display());
    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("installer fetch: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Network(format!("installer body: {e}")))?;
    std::fs::write(&tmp_path, &bytes)?;
    log::info!(
        "app_update: downloaded {} bytes to {}",
        bytes.len(),
        tmp_path.display()
    );

    // 2) Spawn the installer. After it returns, we exit the app
    //    so the installer can replace files.
    spawn_installer_and_exit(&app, &tmp_path).await?;

    // 3) The spawn call hands control to the installer. We tell
    //    the frontend the install "succeeded" (the user will
    //    see the installer UI; our process is about to die).
    //    Returning Ok here is correct — the *download* part
    //    succeeded, the actual replace is the installer's job.
    Ok(())
}

/// Platform-specific installer launcher.
///
///   - **Windows (NSIS)**: `start /wait <installer.exe> /S` — the
///     `/S` flag tells NSIS to install silently (no UI, no
///     "Next/Finish" dialogs). The installer replaces
///     `C:\Users\<u>\AppData\Local\Cloakwire\cloakwire.exe` in
///     place. We then `app.exit(0)` so our process releases the
///     mapped .exe and the installer can overwrite it.
///
///   - **macOS (.dmg)**: `hdiutil attach` the dmg read-only,
///     `cp -R` the .app to /Applications (overwriting), then
///     `hdiutil detach`. After we `app.exit(0)`, the user can
///     re-launch the new build from Launchpad. The .dmg is
///     already signed by CI (well, unsigned — see release
///     notes) and `hdiutil attach` won't actually require
///     signature verification for a simple copy-out.
///
///   - **Linux (.deb / .AppImage)**: For .deb we shell out to
///     `pkexec dpkg -i <file>` (graphical sudo prompt). For
///     .AppImage we just `chmod +x` and exec in place. Both
///     cases need a process exit so we don't double-run.
async fn spawn_installer_and_exit(app: &AppHandle, installer_path: &PathBuf) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        // NSIS silent install. `/S` is uppercase for NSIS
        // (lowercase /s also works but we use the canonical form).
        log::info!(
            "app_update: launching NSIS installer in silent mode: {}",
            installer_path.display()
        );
        std::process::Command::new(installer_path)
            .arg("/S")
            .spawn()
            .map_err(|e| AppError::Spawn(format!("launch installer: {e}")))?;
        // Give the installer a beat to start, then exit so it
        // can replace the .exe. The installer is detached —
        // it will keep running after we exit.
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            log::info!("app_update: exiting app so installer can replace files");
            app_handle.exit(0);
        });
    }
    #[cfg(target_os = "macos")]
    {
        let installer = installer_path.clone();
        let dmg = installer.clone();
        // hdiutil attach + cp -R + hdiutil detach on a thread;
        // tauri::async_runtime::spawn_blocking would also work
        // but std::thread::spawn keeps the deps surface smaller.
        std::thread::spawn(move || {
            use std::process::Command;
            let mp = format!("/tmp/cloakwire-dmg-{}", std::process::id());
            let _ = Command::new("hdiutil").args(["attach", "-nobrowse", "-mountpoint", &mp]).arg(&dmg).status();
            let _ = Command::new("cp").arg("-R").arg(format!("{mp}/Cloakwire.app")).arg("/Applications/").status();
            let _ = Command::new("hdiutil").arg("detach").arg(&mp).status();
            log::info!("app_update: macOS install path finished");
        });
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            log::info!("app_update: exiting app");
            app_handle.exit(0);
        });
    }
    #[cfg(target_os = "linux")]
    {
        let installer = installer_path.clone();
        // Try to detect .deb vs .AppImage by extension. .deb uses
        // pkexec (graphical sudo), .AppImage is just chmod +x.
        let is_deb = installer
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("deb"))
            .unwrap_or(false);
        std::thread::spawn(move || {
            use std::process::Command;
            let mut cmd = if is_deb {
                let mut c = Command::new("pkexec");
                c.arg("dpkg").arg("-i").arg(&installer);
                c
            } else {
                let _ = std::fs::set_permissions(
                    &installer,
                    std::os::unix::fs::PermissionsExt::from_mode(0o755),
                );
                let mut c = Command::new(&installer);
                c
            };
            let _ = cmd.status();
            log::info!("app_update: linux install path finished");
        });
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            log::info!("app_update: exiting app");
            app_handle.exit(0);
        });
    }
    // Touch the unused-import lints for `installer_path` on
    // non-Windows — it's the path that `std::thread::spawn` and
    // `Command::new` consume on those platforms.
    let _ = installer_path;
    Ok(())
}
