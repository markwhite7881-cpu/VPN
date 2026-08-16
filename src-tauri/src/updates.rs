//! Trusted runtime updates for the bundled `sing-box` core.
//!
//! Release metadata, asset URLs, checksums, extraction, and installation all stay
//! in Rust. The WebView may supply only the version it previously displayed.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zip::ZipArchive;

use crate::error::{AppError, AppResult};
use crate::process::ProcessManager;

const RUNTIME_BIN_SUBDIR: &str = "singbox-runtime";
#[cfg(windows)]
const RUNTIME_BIN_NAME: &str = "sing-box.exe";
#[cfg(not(windows))]
const RUNTIME_BIN_NAME: &str = "sing-box";
const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/SagerNet/sing-box/releases/latest";
const USER_AGENT: &str = concat!("cloakwire/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingboxUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub asset_name: Option<String>,
    pub size_bytes: u64,
}

impl SingboxUpdateInfo {
    pub fn not_available(current: String, latest: String) -> Self {
        Self { current_version: current, latest_version: latest, available: false, asset_name: None, size_bytes: 0 }
    }
}

pub fn runtime_bin_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(|e| AppError::BinaryNotFound(format!("app_data_dir: {e}")))?;
    Ok(dir.join(RUNTIME_BIN_SUBDIR).join(RUNTIME_BIN_NAME))
}

pub fn runtime_bin_exists(app: &AppHandle) -> bool {
    runtime_bin_path(app).map(|p| p.exists()).unwrap_or(false)
}

pub async fn check_singbox_update(app: &AppHandle) -> AppResult<SingboxUpdateInfo> {
    let current = crate::commands::get_singbox_version(app.clone()).await.map(|v| v.version).unwrap_or_default();
    let release = fetch_latest_release().await?;
    let latest = normalize_version(&release.tag_name);
    let asset = match current_platform().and_then(|platform| select_archive(&release.assets, &latest, platform)) {
        Ok(asset) => asset,
        Err(_) => return Ok(SingboxUpdateInfo::not_available(current, latest)),
    };
    Ok(SingboxUpdateInfo {
        current_version: current.clone(),
        latest_version: latest.clone(),
        available: version_is_newer(&latest, &current),
        asset_name: Some(asset.name.clone()),
        size_bytes: asset.size,
    })
}

/// Refetches the official release record and installs only the checksum-verified
/// archive selected for this host. No browser-provided download data is accepted.
pub async fn apply_singbox_update(app: AppHandle, expected_version: Option<String>) -> AppResult<String> {
    let release = fetch_latest_release().await?;
    let version = bind_expected_version(&normalize_version(&release.tag_name), expected_version)?;
    let archive = select_archive(&release.assets, &version, current_platform()?)?;
    let checksum_asset = select_checksum_manifest(&release.assets)?;
    let tag = release.tag_name.clone();

    let checksum_bytes = download_release_asset(&checksum_asset, &tag).await?;
    let checksum_text = std::str::from_utf8(&checksum_bytes)
        .map_err(|_| AppError::Network("checksum manifest is not UTF-8".to_string()))?;
    let expected_hash = checksum_for_archive(checksum_text, &archive.name)?;

    let dest = runtime_bin_path(&app)?;
    let runtime_dir = dest.parent().ok_or_else(|| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, "runtime path has no parent")))?;
    fs::create_dir_all(runtime_dir)?;
    let staging = runtime_dir.join(format!(".staging-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let result = install_verified_release(&app, &archive, &tag, &expected_hash, &version, &dest, &staging).await;
    let _ = fs::remove_dir_all(&staging);
    result
}

async fn install_verified_release(
    app: &AppHandle,
    archive: &GithubAsset,
    tag: &str,
    expected_hash: &str,
    expected_version: &str,
    dest: &Path,
    staging: &Path,
) -> AppResult<String> {
    let archive_bytes = download_release_asset(archive, tag).await?;
    let actual_hash = sha256_hex(&archive_bytes);
    hashes_match(expected_hash, &actual_hash)?;

    let archive_path = staging.join("archive.zip");
    write_synced(&archive_path, &archive_bytes)?;
    let candidate = extract_singbox_from_zip(&archive_path, staging)?;
    let candidate_version = probe_binary_version(&candidate).await?;
    if normalize_version(&candidate_version) != expected_version {
        return Err(AppError::Spawn(format!("candidate version {candidate_version:?} does not match release {expected_version}")));
    }

    let manager = app.state::<Arc<ProcessManager>>().inner().clone();
    if manager.is_running().await {
        manager.stop().await.map_err(|e| AppError::Spawn(format!("refusing to replace running sing-box: {e}")))?;
    }
    if manager.is_running().await {
        return Err(AppError::Spawn("refusing to replace sing-box while it is still running".to_string()));
    }

    let parent = dest.parent().ok_or_else(|| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, "runtime path has no parent")))?;
    let staged_dest = parent.join(format!(".sing-box-{}.candidate", Uuid::new_v4()));
    fs::copy(&candidate, &staged_dest)?;
    sync_file(&staged_dest)?;
    let previous = dest.with_extension("previous");
    if previous.exists() {
        fs::remove_file(&previous)?;
    }

    let had_previous = dest.exists();
    if had_previous {
        fs::rename(dest, &previous)?;
    }
    if let Err(error) = fs::rename(&staged_dest, dest) {
        if had_previous { let _ = fs::rename(&previous, dest); }
        return Err(AppError::Io(error));
    }

    match probe_binary_version(dest).await {
        Ok(installed) if normalize_version(&installed) == expected_version => {
            if had_previous { let _ = fs::remove_file(&previous); }
            Ok(installed)
        }
        Ok(installed) => rollback(dest, &previous, had_previous, format!("installed version {installed:?} does not match release {expected_version}")),
        Err(error) => rollback(dest, &previous, had_previous, format!("installed binary validation failed: {error}")),
    }
}

fn rollback<T>(dest: &Path, previous: &Path, had_previous: bool, message: String) -> AppResult<T> {
    let _ = fs::remove_file(dest);
    if had_previous {
        fs::rename(previous, dest).map_err(|restore| AppError::Spawn(format!("{message}; rollback also failed: {restore}")))?;
    }
    Err(AppError::Spawn(message))
}

async fn fetch_latest_release() -> AppResult<GithubRelease> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).timeout(std::time::Duration::from_secs(15)).build()
        .map_err(|e| AppError::Network(format!("http client: {e}")))?;
    let response = client.get(RELEASES_LATEST_URL).send().await.map_err(|e| AppError::Network(format!("releases/latest: {e}")))?;
    if !response.status().is_success() { return Err(AppError::Network(format!("GitHub API returned {}", response.status()))); }
    response.json().await.map_err(|e| AppError::Network(format!("release JSON: {e}")))
}

async fn download_release_asset(asset: &GithubAsset, tag: &str) -> AppResult<Vec<u8>> {
    if !is_trusted_download_url(&asset.browser_download_url, tag, &asset.name) {
        return Err(AppError::Network("release asset URL is outside the trusted GitHub download route".to_string()));
    }
    let client = reqwest::Client::builder().redirect(Policy::none()).user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120)).build().map_err(|e| AppError::Network(format!("http client: {e}")))?;
    let mut url = asset.browser_download_url.clone();
    for _ in 0..=MAX_REDIRECTS {
        let response = client.get(&url).send().await.map_err(|e| AppError::Network(format!("asset download: {e}")))?;
        if response.status().is_success() { return response.bytes().await.map(|b| b.to_vec()).map_err(|e| AppError::Network(format!("asset body: {e}"))); }
        if !response.status().is_redirection() { return Err(AppError::Network(format!("asset download returned {}", response.status()))); }
        let next = response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Network("asset redirect has no valid Location".to_string()))?;
        let resolved = url::Url::parse(&url).and_then(|base| base.join(next))
            .map_err(|e| AppError::Network(format!("invalid asset redirect: {e}")))?;
        if !is_trusted_redirect_url(resolved.as_str(), tag, &asset.name) { return Err(AppError::Network("asset redirect escaped trusted GitHub release hosts".to_string())); }
        url = resolved.to_string();
    }
    Err(AppError::Network("too many asset redirects".to_string()))
}

#[derive(Debug, Deserialize)]
struct GithubRelease { tag_name: String, assets: Vec<GithubAsset> }
#[derive(Debug, Clone, Deserialize)]
struct GithubAsset { name: String, browser_download_url: String, size: u64 }

#[derive(Debug, Clone, Copy)]
enum Platform { WindowsX86_64 }
fn current_platform() -> AppResult<Platform> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { Ok(Platform::WindowsX86_64) }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    { Err(AppError::Network("sing-box runtime updates are unsupported on this platform".to_string())) }
}

fn select_archive<'a>(assets: &'a [GithubAsset], version: &str, platform: Platform) -> AppResult<&'a GithubAsset> {
    let suffix = match platform {
        Platform::WindowsX86_64 => "windows-amd64.zip",
    };
    let exact = format!("sing-box-{version}-{suffix}");
    let mut matches = assets.iter().filter(|asset| asset.name == exact);
    match (matches.next(), matches.next()) {
        (Some(asset), None) => Ok(asset),
        _ => Err(AppError::Network(format!("release does not contain exactly one {exact}"))),
    }
}

fn select_checksum_manifest(assets: &[GithubAsset]) -> AppResult<&GithubAsset> {
    let mut manifests = assets.iter().filter(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name == "checksums.txt" || name == "sha256sums.txt" || name.ends_with(".sha256") || name.ends_with(".sha256sum")
    });
    match (manifests.next(), manifests.next()) {
        (Some(asset), None) => Ok(asset),
        _ => Err(AppError::Network("release must contain exactly one SHA-256 checksum manifest".to_string())),
    }
}

fn checksum_for_archive(manifest: &str, archive_name: &str) -> AppResult<String> {
    let mut matching = Vec::new();
    for line in manifest.lines().filter(|line| !line.trim().is_empty()) {
        let (hash, filename) = line.split_once("  ").ok_or_else(|| AppError::Network("malformed SHA-256 manifest entry".to_string()))?;
        if !is_sha256(hash) || filename.is_empty() || filename.starts_with('*') || filename.contains('/') || filename.contains('\\') {
            return Err(AppError::Network("malformed SHA-256 manifest entry".to_string()));
        }
        if filename == archive_name { matching.push(hash.to_ascii_lowercase()); }
    }
    match matching.as_slice() {
        [hash] => Ok(hash.clone()),
        [] => Err(AppError::Network(format!("checksum manifest has no entry for {archive_name}"))),
        _ => Err(AppError::Network(format!("checksum manifest has duplicate entries for {archive_name}"))),
    }
}

fn is_sha256(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) }
fn hashes_match(expected: &str, actual: &str) -> AppResult<()> {
    if is_sha256(expected) && expected.eq_ignore_ascii_case(actual) { Ok(()) } else { Err(AppError::Network("downloaded archive SHA-256 does not match upstream manifest".to_string())) }
}
fn sha256_hex(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn normalize_version(version: &str) -> String { version.trim().trim_start_matches('v').to_string() }
fn bind_expected_version(latest: &str, expected: Option<String>) -> AppResult<String> {
    if let Some(expected) = expected.filter(|v| !v.trim().is_empty()) {
        if normalize_version(&expected) != latest { return Err(AppError::Network(format!("release changed from {} to {latest}; check again", normalize_version(&expected)))); }
    }
    Ok(latest.to_string())
}
fn version_is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| s.split('.').filter_map(|p| p.split('-').next().unwrap_or(p).parse::<u64>().ok()).collect::<Vec<_>>();
    let (av, bv) = (parse(a), parse(b));
    if av.is_empty() || bv.is_empty() { return a != b && a > b; }
    (0..av.len().max(bv.len())).find_map(|i| match av.get(i).copied().unwrap_or(0).cmp(&bv.get(i).copied().unwrap_or(0)) { std::cmp::Ordering::Equal => None, other => Some(other) }).is_some_and(|o| o.is_gt())
}

fn is_trusted_download_url(value: &str, tag: &str, filename: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else { return false; };
    url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && url.host_str() == Some("github.com") && url.path() == format!("/SagerNet/sing-box/releases/download/{tag}/{filename}") && url.query().is_none() && url.fragment().is_none()
}
fn is_trusted_redirect_url(value: &str, tag: &str, filename: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else { return false; };
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() || url.fragment().is_some() { return false; }
    match url.host_str() {
        Some("github.com") => is_trusted_download_url(value, tag, filename),
        Some("release-assets.githubusercontent.com") | Some("objects.githubusercontent.com") => !url.path().is_empty(),
        _ => false,
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> AppResult<()> { let mut file = File::create(path)?; file.write_all(bytes)?; file.sync_all()?; Ok(()) }
fn sync_file(path: &Path) -> AppResult<()> { OpenOptions::new().read(true).open(path)?.sync_all()?; Ok(()) }

fn extract_singbox_from_zip(archive_path: &Path, staging: &Path) -> AppResult<PathBuf> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| AppError::Spawn(format!("invalid zip archive: {e}")))?;
    let mut index = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| AppError::Spawn(format!("zip entry: {e}")))?;
        let path = Path::new(entry.name());
        if path.components().any(|part| !matches!(part, Component::Normal(_))) { return Err(AppError::Spawn("zip archive contains unsafe path".to_string())); }
        if path.file_name().is_some_and(|name| name == RUNTIME_BIN_NAME) {
            if index.replace(i).is_some() { return Err(AppError::Spawn("zip archive contains multiple sing-box binaries".to_string())); }
        }
    }
    let index = index.ok_or_else(|| AppError::Spawn(format!("zip archive lacks {RUNTIME_BIN_NAME}")))?;
    let mut entry = archive.by_index(index).map_err(|e| AppError::Spawn(format!("zip entry: {e}")))?;
    let candidate = staging.join("candidate").join(RUNTIME_BIN_NAME);
    fs::create_dir_all(candidate.parent().expect("candidate has parent"))?;
    let mut output = File::create(&candidate)?;
    std::io::copy(&mut entry, &mut output)?;
    output.sync_all()?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755))?; }
    Ok(candidate)
}

async fn probe_binary_version(binary: &Path) -> AppResult<String> {
    let mut command = tokio::process::Command::new(binary);
    command.arg("version");
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    let output = command.output().await.map_err(|e| AppError::Spawn(format!("version probe failed: {e}")))?;
    if !output.status.success() { return Err(AppError::Spawn(format!("version probe exited {:?}", output.status.code()))); }
    String::from_utf8_lossy(&output.stdout).lines().find_map(|line| line.strip_prefix("sing-box version ").map(|v| v.trim().to_string())).filter(|v| !v.is_empty()).ok_or_else(|| AppError::Spawn("version probe returned no sing-box version".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn asset(name: &str) -> GithubAsset { GithubAsset { name: name.to_string(), browser_download_url: format!("https://github.com/SagerNet/sing-box/releases/download/v1.2.3/{name}"), size: 1 } }
    #[test] fn selects_only_exact_platform_archive() { let assets = vec![asset("sing-box-1.2.3-windows-amd64-cgo.zip"), asset("sing-box-1.2.3-windows-amd64.zip"), asset("sing-box-1.2.3-linux-amd64.tar.gz")]; assert_eq!(select_archive(&assets, "1.2.3", Platform::WindowsX86_64).unwrap().name, "sing-box-1.2.3-windows-amd64.zip"); }
    #[test] fn rejects_release_without_checksum_for_selected_archive() { assert!(checksum_for_archive(&("a".repeat(64) + "  other.zip\n"), "target.zip").is_err()); }
    #[test] fn rejects_checksum_mismatch_malformed_and_duplicate_entries() { assert!(checksum_for_archive("not-a-hash  target.zip\n", "target.zip").is_err()); let duplicate = format!("{}  target.zip\n{}  target.zip\n", "a".repeat(64), "b".repeat(64)); assert!(checksum_for_archive(&duplicate, "target.zip").is_err()); assert!(hashes_match(&"a".repeat(64), &"b".repeat(64)).is_err()); }
    #[test] fn rejects_expected_version_mismatch() { assert!(bind_expected_version("1.2.3", Some("1.2.4".to_string())).is_err()); }
    #[test] fn accepts_only_github_release_download_routes() { assert!(is_trusted_download_url("https://github.com/SagerNet/sing-box/releases/download/v1.2.3/sing-box-1.2.3-windows-amd64.zip", "v1.2.3", "sing-box-1.2.3-windows-amd64.zip")); assert!(!is_trusted_download_url("https://example.com/SagerNet/sing-box/releases/download/v1.2.3/sing-box-1.2.3-windows-amd64.zip", "v1.2.3", "sing-box-1.2.3-windows-amd64.zip")); assert!(!is_trusted_download_url("https://github.com/SagerNet/sing-box/releases/download/v1.2.3/other.zip", "v1.2.3", "sing-box-1.2.3-windows-amd64.zip")); assert!(!is_trusted_redirect_url("https://example.com/evil", "v1.2.3", "sing-box-1.2.3-windows-amd64.zip")); }
}
