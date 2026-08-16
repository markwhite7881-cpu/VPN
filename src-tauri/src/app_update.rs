//! Backend-authoritative, signed app-shell updater.
//!
//! The WebView sees only update metadata. Rust refetches the configured manifest,
//! validates its GitHub release origin and every redirect, verifies the full
//! minisign signature over downloaded bytes, and only then persists an installer.
//! Installer execution is deliberately deferred to the platform-dispatch task.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use minisign::{verify, PublicKeyBox, SignatureBox};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use url::Url;

use crate::error::{AppError, AppResult};

const UPDATER_MANIFEST_URL: &str =
    "https://github.com/markwhite7881-cpu/cloakwire/releases/latest/download/latest.json";
const UPDATER_PUBLIC_KEY: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI3NEUyMThENkM5QzUwMTkKUldRWlVKeHNqU0ZPSjhURUJRN1JOTzkzNnNPR01JeTdOS2VDTjN6aGxOUzBZd3MxbzRlcldaRGYK";
const USER_AGENT: &str = concat!("cloakwire/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 5;
const GITHUB_HOST: &str = "github.com";
const RELEASE_ASSETS_HOST: &str = "release-assets.githubusercontent.com";
const RELEASE_PATH_PREFIX: &str = "/markwhite7881-cpu/cloakwire/releases/download/";
const MANIFEST_PATH: &str = "/markwhite7881-cpu/cloakwire/releases/latest/download/latest.json";

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub available: bool,
    pub notes: String,
}

impl AppUpdateInfo {
    fn unavailable(current_version: String, version: String, notes: String) -> Self {
        Self { version, current_version, available: false, notes }
    }
}

#[derive(Debug, Deserialize)]
struct UpdaterManifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: std::collections::BTreeMap<String, PlatformEntry>,
}

#[derive(Debug, Deserialize)]
struct PlatformEntry {
    url: String,
    signature: String,
}

fn app_error(message: impl Into<String>) -> AppError {
    AppError::Network(format!("update verification failed: {}", message.into()))
}

fn current_platform() -> Option<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { Some("windows-x86_64") }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { Some("darwin-aarch64") }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { Some("darwin-x86_64") }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { Some("linux-x86_64") }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    { None }
}

fn platform_entry_for<'a>(manifest: &'a UpdaterManifest, platform: &str) -> Option<&'a PlatformEntry> {
    manifest.platforms.get(platform)
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    let parse = |value: &str| value.split('.').map(|part| part.parse::<u64>()).collect::<Result<Vec<_>, _>>();
    let (Ok(candidate), Ok(current)) = (parse(candidate), parse(current)) else { return false; };
    let length = candidate.len().max(current.len());
    (0..length).find_map(|index| {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        (left != right).then_some(left > right)
    }).unwrap_or(false)
}

fn ensure_expected_version(expected: Option<&str>, actual: &str) -> AppResult<()> {
    if let Some(expected) = expected.filter(|version| !version.is_empty()) {
        if expected != actual {
            return Err(app_error(format!("expected manifest version {expected}, received {actual}")));
        }
    }
    Ok(())
}

fn parse_https_url(raw: &str) -> AppResult<Url> {
    let url = Url::parse(raw).map_err(|error| app_error(format!("invalid URL: {error}")))?;
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() || url.port().is_some() {
        return Err(app_error("URL must be plain HTTPS without credentials or an explicit port"));
    }
    Ok(url)
}

fn trusted_redirect_url(url: &Url) -> AppResult<()> {
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() || url.port().is_some() {
        return Err(app_error("redirect URL must be plain HTTPS"));
    }
    match url.host_str() {
        Some(GITHUB_HOST | RELEASE_ASSETS_HOST) => Ok(()),
        _ => Err(app_error("redirect host is not trusted")),
    }
}

fn validate_manifest_url(raw: &str) -> AppResult<Url> {
    let url = parse_https_url(raw)?;
    if url.host_str() != Some(GITHUB_HOST) || url.path() != MANIFEST_PATH || url.query().is_some() || url.fragment().is_some() {
        return Err(app_error("manifest URL does not match configured GitHub release route"));
    }
    Ok(url)
}

fn validate_release_asset_url(raw: &str) -> AppResult<Url> {
    let url = parse_https_url(raw)?;
    if url.host_str() != Some(GITHUB_HOST)
        || !url.path().starts_with(RELEASE_PATH_PREFIX)
        || url.path()[RELEASE_PATH_PREFIX.len()..].split('/').count() != 2
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(app_error("artifact URL does not match the configured GitHub release route"));
    }
    Ok(url)
}

fn decode_manifest_signature(encoded: &str) -> AppResult<String> {
    if encoded.trim().is_empty() { return Err(app_error("manifest signature is empty")); }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| app_error(format!("manifest signature is not base64: {error}")))?;
    let text = String::from_utf8(bytes).map_err(|_| app_error("manifest signature is not UTF-8"))?;
    if text.trim().is_empty() { return Err(app_error("manifest signature is empty after decoding")); }
    Ok(text)
}

fn verify_update_signature(public_key: &str, artifact: &[u8], signature_text: &str) -> AppResult<()> {
    let public_key_text = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .map_err(|error| app_error(format!("configured public key is not base64: {error}")))?;
    let public_key_text = String::from_utf8(public_key_text).map_err(|_| app_error("configured public key is not UTF-8"))?;
    let public_key = PublicKeyBox::from_string(&public_key_text)
        .and_then(PublicKeyBox::into_public_key)
        .map_err(|error| app_error(format!("configured public key is invalid: {error}")))?;
    let signature = SignatureBox::from_string(signature_text)
        .map_err(|error| app_error(format!("invalid minisign signature: {error}")))?;
    verify(&public_key, &signature, Cursor::new(artifact), true, false, false)
        .map_err(|error| app_error(format!("artifact signature verification failed: {error}")))
}

fn http_client(timeout: Duration) -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| AppError::Network(format!("updater HTTP client: {error}")))
}

async fn get_trusted(client: &reqwest::Client, initial: Url, artifact: bool) -> AppResult<reqwest::Response> {
    let mut url = initial;
    for _ in 0..=MAX_REDIRECTS {
        let response = client.get(url.clone()).send().await
            .map_err(|error| AppError::Network(format!("updater download: {error}")))?;
        if response.status().is_redirection() {
            let location = response.headers().get(reqwest::header::LOCATION)
                .ok_or_else(|| app_error("redirect response has no Location header"))?
                .to_str().map_err(|_| app_error("redirect Location is not valid text"))?;
            let next = url.join(location).map_err(|error| app_error(format!("invalid redirect URL: {error}")))?;
            trusted_redirect_url(&next)?;
            url = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(AppError::Network(format!("updater server returned {}", response.status())));
        }
        trusted_redirect_url(response.url())?;
        return Ok(response);
    }
    Err(app_error("too many redirects"))
}

async fn fetch_manifest() -> AppResult<UpdaterManifest> {
    let initial = validate_manifest_url(UPDATER_MANIFEST_URL)?;
    let client = http_client(Duration::from_secs(15))?;
    let response = get_trusted(&client, initial, false).await?;
    response.json().await.map_err(|error| AppError::Network(format!("updater manifest JSON: {error}")))
}

pub async fn check_app_update(_app: AppHandle) -> AppResult<AppUpdateInfo> {
    let manifest = fetch_manifest().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let available = current_platform()
        .and_then(|platform| platform_entry_for(&manifest, platform))
        .is_some() && version_is_newer(&manifest.version, &current_version);
    Ok(AppUpdateInfo { version: manifest.version, current_version, available, notes: manifest.notes })
}

fn installer_extension(url: &Url) -> AppResult<&str> {
    Path::new(url.path()).extension().and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && value.len() <= 10 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .ok_or_else(|| app_error("validated artifact URL has no safe filename extension"))
}

fn verified_installer_path(version: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join("cloakwire-update").join(format!("cloakwire-{version}-{}.{}", uuid::Uuid::new_v4(), extension))
}

pub async fn install_app_update(_app: AppHandle, expected_version: Option<String>) -> AppResult<()> {
    let manifest = fetch_manifest().await?;
    ensure_expected_version(expected_version.as_deref(), &manifest.version)?;
    let current_version = env!("CARGO_PKG_VERSION");
    if !version_is_newer(&manifest.version, current_version) {
        return Err(app_error("manifest does not offer a newer version"));
    }
    let platform = current_platform().ok_or_else(|| app_error("this platform has no supported app updater"))?;
    let entry = platform_entry_for(&manifest, platform).ok_or_else(|| app_error("manifest has no installer for this platform"))?;
    let artifact_url = validate_release_asset_url(&entry.url)?;
    let signature = decode_manifest_signature(&entry.signature)?;
    let extension = installer_extension(&artifact_url)?;
    let client = http_client(Duration::from_secs(600))?;
    let artifact = get_trusted(&client, artifact_url, true).await?.bytes().await
        .map_err(|error| AppError::Network(format!("installer body: {error}")))?;
    verify_update_signature(UPDATER_PUBLIC_KEY, &artifact, &signature)?;

    let path = verified_installer_path(&manifest.version, extension);
    let parent = path.parent().expect("verified installer path has parent");
    std::fs::create_dir_all(parent)?;
    std::fs::write(&path, artifact)?;
    Err(AppError::Spawn(format!(
        "verified installer staged at {}; execution is intentionally deferred until platform dispatch is implemented",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_manifest_matches_tauri_configuration() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        assert_eq!(updater["endpoints"][0].as_str(), Some(UPDATER_MANIFEST_URL));
        assert_eq!(updater["pubkey"].as_str(), Some(UPDATER_PUBLIC_KEY));
        assert!(validate_manifest_url(UPDATER_MANIFEST_URL).is_ok());
    }

    #[test]
    fn accepts_exact_project_release_asset_route() {
        let url = validate_release_asset_url("https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/Cloakwire_1.2.3_x64-setup.exe").unwrap();
        assert_eq!(url.host_str(), Some("github.com"));
    }

    #[test]
    fn rejects_non_https_update_asset() {
        assert!(validate_release_asset_url("http://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe").is_err());
    }

    #[test]
    fn rejects_lookalike_update_asset_host() {
        assert!(validate_release_asset_url("https://github.com.attacker.example/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe").is_err());
    }

    #[test]
    fn rejects_wrong_github_repository_or_route() {
        assert!(validate_release_asset_url("https://github.com/attacker/cloakwire/releases/download/v1.2.3/update.exe").is_err());
        assert!(validate_release_asset_url("https://github.com/markwhite7881-cpu/cloakwire/releases/latest/download/update.exe").is_err());
    }

    #[test]
    fn decodes_full_textual_manifest_signature() {
        let signature = "untrusted comment: signature from minisign secret key\nRURWVEVTVC1TSUdOQVRVUkU=\ntrusted comment: timestamp:1\nR0xPQkFM\n";
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature);
        assert_eq!(decode_manifest_signature(&encoded).unwrap(), signature);
    }

    #[test]
    fn rejects_empty_or_invalid_manifest_signature() {
        assert!(decode_manifest_signature("").is_err());
        assert!(decode_manifest_signature("not-base64").is_err());
        let encoded = base64::engine::general_purpose::STANDARD.encode([0xff]);
        assert!(decode_manifest_signature(&encoded).is_err());
    }

    #[test]
    fn selects_matching_platform_entry() {
        let manifest: UpdaterManifest = serde_json::from_str(r#"{"version":"1.2.3","platforms":{"windows-x86_64":{"url":"https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe","signature":"sig"}}}"#).unwrap();
        assert!(platform_entry_for(&manifest, "windows-x86_64").is_some());
    }

    #[test]
    fn returns_none_when_platform_entry_is_missing() {
        let manifest: UpdaterManifest = serde_json::from_str(r#"{"version":"1.2.3","platforms":{}}"#).unwrap();
        assert!(platform_entry_for(&manifest, "windows-x86_64").is_none());
    }

    #[test]
    fn rejects_expected_version_mismatch() {
        assert!(ensure_expected_version(Some("1.2.2"), "1.2.3").is_err());
    }

    #[test]
    fn accepts_matching_or_absent_expected_version() {
        assert!(ensure_expected_version(Some("1.2.3"), "1.2.3").is_ok());
        assert!(ensure_expected_version(None, "1.2.3").is_ok());
    }
}
