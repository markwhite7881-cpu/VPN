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
    {
        Some("windows-x86_64")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some("darwin-aarch64")
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Some("darwin-x86_64")
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Some("linux-x86_64")
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

fn platform_entry_for<'a>(
    manifest: &'a UpdaterManifest,
    platform: &str,
) -> Option<&'a PlatformEntry> {
    manifest.platforms.get(platform)
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    let parse = |value: &str| {
        value
            .split('.')
            .map(|part| part.parse::<u64>())
            .collect::<Result<Vec<_>, _>>()
    };
    let (Ok(candidate), Ok(current)) = (parse(candidate), parse(current)) else {
        return false;
    };
    let length = candidate.len().max(current.len());
    (0..length)
        .find_map(|index| {
            let left = candidate.get(index).copied().unwrap_or(0);
            let right = current.get(index).copied().unwrap_or(0);
            (left != right).then_some(left > right)
        })
        .unwrap_or(false)
}

fn ensure_expected_version(expected: Option<&str>, actual: &str) -> AppResult<()> {
    if let Some(expected) = expected.filter(|version| !version.is_empty()) {
        if expected != actual {
            return Err(app_error(format!(
                "expected manifest version {expected}, received {actual}"
            )));
        }
    }
    Ok(())
}

fn parse_https_url(raw: &str) -> AppResult<Url> {
    let url = Url::parse(raw).map_err(|error| app_error(format!("invalid URL: {error}")))?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(app_error(
            "URL must be plain HTTPS without credentials or an explicit port",
        ));
    }
    Ok(url)
}

fn validate_redirect_url(url: &Url) -> AppResult<()> {
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(app_error("redirect URL must be plain HTTPS"));
    }
    Ok(())
}

fn tagged_manifest_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(RELEASE_PATH_PREFIX) else {
        return false;
    };
    let mut segments = rest.split('/');
    matches!((segments.next(), segments.next(), segments.next()), (Some(tag), Some("latest.json"), None) if !tag.is_empty())
}

fn validate_manifest_redirect(current: &Url, next: &Url) -> AppResult<()> {
    validate_redirect_url(next)?;
    match (current.host_str(), next.host_str()) {
        (Some(GITHUB_HOST), Some(GITHUB_HOST))
            if current.path() == MANIFEST_PATH && tagged_manifest_path(next.path()) =>
        {
            Ok(())
        }
        (Some(GITHUB_HOST), Some(RELEASE_ASSETS_HOST))
            if tagged_manifest_path(current.path()) && !next.path().is_empty() =>
        {
            Ok(())
        }
        _ => Err(app_error(
            "manifest redirect escapes the configured release-manifest route",
        )),
    }
}

fn validate_artifact_redirect(current: &Url, next: &Url) -> AppResult<()> {
    validate_redirect_url(next)?;
    if current.host_str() == Some(GITHUB_HOST)
        && next.host_str() == Some(RELEASE_ASSETS_HOST)
        && !next.path().is_empty()
    {
        return Ok(());
    }
    Err(app_error(
        "artifact redirect escapes the configured release asset route",
    ))
}

fn validate_final_response(requested: &Url, final_url: &Url, artifact: bool) -> AppResult<()> {
    if requested != final_url {
        return Err(app_error(
            "final response URL differs from its validated request URL",
        ));
    }
    if artifact {
        if final_url.host_str() != Some(RELEASE_ASSETS_HOST) || final_url.path().is_empty() {
            return Err(app_error(
                "artifact final response is not the bound release-assets URL",
            ));
        }
    } else if final_url.host_str() != Some(RELEASE_ASSETS_HOST)
        && !tagged_manifest_path(final_url.path())
    {
        return Err(app_error(
            "manifest final response is not a bound release manifest URL",
        ));
    }
    Ok(())
}

fn validate_manifest_url(raw: &str) -> AppResult<Url> {
    let url = parse_https_url(raw)?;
    if url.host_str() != Some(GITHUB_HOST)
        || url.path() != MANIFEST_PATH
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(app_error(
            "manifest URL does not match configured GitHub release route",
        ));
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
        return Err(app_error(
            "artifact URL does not match the configured GitHub release route",
        ));
    }
    Ok(url)
}

fn decode_manifest_signature(encoded: &str) -> AppResult<String> {
    if encoded.trim().is_empty() {
        return Err(app_error("manifest signature is empty"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| app_error(format!("manifest signature is not base64: {error}")))?;
    let text =
        String::from_utf8(bytes).map_err(|_| app_error("manifest signature is not UTF-8"))?;
    if text.trim().is_empty() {
        return Err(app_error("manifest signature is empty after decoding"));
    }
    Ok(text)
}

fn verify_update_signature(
    public_key: &str,
    artifact: &[u8],
    signature_text: &str,
) -> AppResult<()> {
    let public_key_text = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .map_err(|error| app_error(format!("configured public key is not base64: {error}")))?;
    let public_key_text = String::from_utf8(public_key_text)
        .map_err(|_| app_error("configured public key is not UTF-8"))?;
    let public_key = PublicKeyBox::from_string(&public_key_text)
        .and_then(PublicKeyBox::into_public_key)
        .map_err(|error| app_error(format!("configured public key is invalid: {error}")))?;
    let signature = SignatureBox::from_string(signature_text)
        .map_err(|error| app_error(format!("invalid minisign signature: {error}")))?;
    verify(
        &public_key,
        &signature,
        Cursor::new(artifact),
        true,
        false,
        false,
    )
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

async fn get_trusted(
    client: &reqwest::Client,
    initial: Url,
    artifact: bool,
) -> AppResult<reqwest::Response> {
    let mut url = initial;
    for _ in 0..=MAX_REDIRECTS {
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| AppError::Network(format!("updater download: {error}")))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| app_error("redirect response has no Location header"))?
                .to_str()
                .map_err(|_| app_error("redirect Location is not valid text"))?;
            let next = url
                .join(location)
                .map_err(|error| app_error(format!("invalid redirect URL: {error}")))?;
            if artifact {
                validate_artifact_redirect(&url, &next)?;
            } else {
                validate_manifest_redirect(&url, &next)?;
            }
            url = next;
            continue;
        }
        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "updater server returned {}",
                response.status()
            )));
        }
        validate_final_response(&url, response.url(), artifact)?;
        return Ok(response);
    }
    Err(app_error("too many redirects"))
}

async fn fetch_manifest() -> AppResult<UpdaterManifest> {
    let initial = validate_manifest_url(UPDATER_MANIFEST_URL)?;
    let client = http_client(Duration::from_secs(15))?;
    let response = get_trusted(&client, initial, false).await?;
    response
        .json()
        .await
        .map_err(|error| AppError::Network(format!("updater manifest JSON: {error}")))
}

pub async fn check_app_update(_app: AppHandle) -> AppResult<AppUpdateInfo> {
    let manifest = fetch_manifest().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let available = current_platform()
        .and_then(|platform| platform_entry_for(&manifest, platform))
        .and_then(|entry| validate_release_asset_url(&entry.url).ok())
        .and_then(|url| installer_kind_from_url(&url).ok())
        .is_some_and(|kind| kind.is_supported_on_current_platform())
        && version_is_newer(&manifest.version, &current_version);
    Ok(AppUpdateInfo {
        version: manifest.version,
        current_version,
        available,
        notes: manifest.notes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerKind {
    Exe,
    Msi,
    Deb,
    AppImage,
    Dmg,
}

impl InstallerKind {
    fn extension(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Msi => "msi",
            Self::Deb => "deb",
            Self::AppImage => "AppImage",
            Self::Dmg => "dmg",
        }
    }

    fn is_supported_on_current_platform(self) -> bool {
        match self {
            Self::Exe | Self::Msi => cfg!(windows),
            Self::Deb | Self::AppImage => cfg!(target_os = "linux"),
            Self::Dmg => cfg!(target_os = "macos"),
        }
    }
}

fn installer_kind_from_asset_name(asset_name: &str) -> AppResult<InstallerKind> {
    if asset_name.is_empty()
        || Path::new(asset_name)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(asset_name)
        || !asset_name.starts_with("Cloakwire_")
    {
        return Err(app_error(
            "artifact name is not a recognized Cloakwire installer",
        ));
    }

    if asset_name.ends_with(".AppImage") {
        return Ok(InstallerKind::AppImage);
    }
    match Path::new(asset_name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("exe") => Ok(InstallerKind::Exe),
        Some("msi") => Ok(InstallerKind::Msi),
        Some("deb") => Ok(InstallerKind::Deb),
        Some("dmg") => Ok(InstallerKind::Dmg),
        _ => Err(app_error("artifact name has an unsupported installer kind")),
    }
}

fn installer_kind_from_url(url: &Url) -> AppResult<InstallerKind> {
    let asset_name = Path::new(url.path())
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| app_error("validated artifact URL has no asset filename"))?;
    installer_kind_from_asset_name(asset_name)
}

fn verified_installer_path(version: &str, kind: InstallerKind) -> PathBuf {
    std::env::temp_dir().join("cloakwire-update").join(format!(
        "cloakwire-{version}-{}.{}",
        uuid::Uuid::new_v4(),
        kind.extension()
    ))
}

fn spawn_installer_and_exit(app: &AppHandle, kind: InstallerKind, path: &Path) -> AppResult<()> {
    if !kind.is_supported_on_current_platform() {
        return Err(app_error(
            "verified installer kind is incompatible with this platform",
        ));
    }

    #[cfg(windows)]
    {
        let mut command = match kind {
            InstallerKind::Exe => std::process::Command::new(path),
            InstallerKind::Msi => {
                let mut command = std::process::Command::new("msiexec");
                command.arg("/i").arg(path);
                command
            }
            _ => {
                return Err(app_error(
                    "verified installer kind is incompatible with Windows",
                ))
            }
        };
        command.spawn().map_err(|error| {
            AppError::Spawn(format!(
                "launch verified installer {}: {error}",
                path.display()
            ))
        })?;
    }

    #[cfg(target_os = "linux")]
    {
        let mut command = match kind {
            InstallerKind::Deb => {
                let mut command = std::process::Command::new("pkexec");
                command.arg("dpkg").arg("-i").arg(path);
                command
            }
            InstallerKind::AppImage => {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(
                    |error| {
                        AppError::Spawn(format!(
                            "make AppImage executable {}: {error}",
                            path.display()
                        ))
                    },
                )?;
                std::process::Command::new(path)
            }
            _ => {
                return Err(app_error(
                    "verified installer kind is incompatible with Linux",
                ))
            }
        };
        command.spawn().map_err(|error| {
            AppError::Spawn(format!(
                "launch verified installer {}: {error}",
                path.display()
            ))
        })?;
    }

    #[cfg(target_os = "macos")]
    {
        if kind != InstallerKind::Dmg {
            return Err(app_error(
                "verified installer kind is incompatible with macOS",
            ));
        }
        let log_path = path.with_extension("update-helper.log");
        let helper = r#"set -eu
exec >> "$2" 2>&1
mount_point=$(mktemp -d /tmp/cloakwire-update.XXXXXX)
mounted=0
backup_moved=0
cleanup() {
  set +e
  if [ "$mounted" = 1 ]; then hdiutil detach "$mount_point"; fi
  rmdir "$mount_point"
}
rollback() {
  if [ "$backup_moved" = 1 ]; then
    rm -rf /Applications/Cloakwire.app || return 1
    mv "$backup" /Applications/Cloakwire.app || return 1
    backup_moved=0
  fi
}
fail_after_backup() {
  message="$1"
  echo "$message"
  if ! rollback; then echo 'rollback of Cloakwire.app failed'; fi
  if [ "$mounted" = 1 ]; then
    if hdiutil detach "$mount_point"; then mounted=0; else echo 'DMG detach failed after rollback'; fi
  fi
  exit 1
}
trap cleanup EXIT
hdiutil attach -nobrowse -mountpoint "$mount_point" "$1"
mounted=1
if [ ! -d "$mount_point/Cloakwire.app" ]; then
  echo 'Cloakwire.app is missing from verified DMG'
  exit 1
fi
temporary=/Applications/.Cloakwire.app.update.$$
backup=/Applications/Cloakwire.app.previous.$$
if ! ditto "$mount_point/Cloakwire.app" "$temporary"; then
  echo 'copy of Cloakwire.app from verified DMG failed'
  exit 1
fi
if [ -e /Applications/Cloakwire.app ]; then
  if ! mv /Applications/Cloakwire.app "$backup"; then
    echo 'backup of existing Cloakwire.app failed'
    exit 1
  fi
  backup_moved=1
fi
if ! mv "$temporary" /Applications/Cloakwire.app; then
  fail_after_backup 'replacement of Cloakwire.app failed'
fi
if ! hdiutil detach "$mount_point"; then
  fail_after_backup 'DMG detach failed after replacement'
fi
mounted=0
if [ "$backup_moved" = 1 ]; then
  if ! rm -rf "$backup"; then echo "updated app installed but preserved backup at $backup"; fi
fi
rmdir "$mount_point" || echo "updated app installed but cleanup directory remains at $mount_point"
trap - EXIT
"#;
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(helper)
            .arg("cloakwire-update-helper")
            .arg(path)
            .arg(&log_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| {
                AppError::Spawn(format!(
                    "launch macOS update helper {}: {error}",
                    path.display()
                ))
            })?;
    }

    app.exit(0);
    Ok(())
}

pub async fn install_app_update(app: AppHandle, expected_version: Option<String>) -> AppResult<()> {
    let manifest = fetch_manifest().await?;
    ensure_expected_version(expected_version.as_deref(), &manifest.version)?;
    let current_version = env!("CARGO_PKG_VERSION");
    if !version_is_newer(&manifest.version, current_version) {
        return Err(app_error("manifest does not offer a newer version"));
    }
    let platform = current_platform()
        .ok_or_else(|| app_error("this platform has no supported app updater"))?;
    let entry = platform_entry_for(&manifest, platform)
        .ok_or_else(|| app_error("manifest has no installer for this platform"))?;
    let artifact_url = validate_release_asset_url(&entry.url)?;
    let signature = decode_manifest_signature(&entry.signature)?;
    let installer_kind = installer_kind_from_url(&artifact_url)?;
    if !installer_kind.is_supported_on_current_platform() {
        return Err(app_error(
            "artifact installer kind is incompatible with this platform",
        ));
    }
    let client = http_client(Duration::from_secs(600))?;
    let artifact = get_trusted(&client, artifact_url, true)
        .await?
        .bytes()
        .await
        .map_err(|error| AppError::Network(format!("installer body: {error}")))?;
    verify_update_signature(UPDATER_PUBLIC_KEY, &artifact, &signature)?;

    let path = verified_installer_path(&manifest.version, installer_kind);
    let parent = path.parent().expect("verified installer path has parent");
    std::fs::create_dir_all(parent)?;
    std::fs::write(&path, artifact)?;
    spawn_installer_and_exit(&app, installer_kind, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_kind_is_derived_from_validated_asset_name() {
        assert_eq!(
            installer_kind_from_asset_name("Cloakwire_1.2.1_amd64.deb").unwrap(),
            InstallerKind::Deb
        );
        assert_eq!(
            installer_kind_from_asset_name("Cloakwire_1.2.1_amd64.AppImage").unwrap(),
            InstallerKind::AppImage
        );
        assert_eq!(
            installer_kind_from_asset_name("Cloakwire_1.2.1_x64-setup.exe").unwrap(),
            InstallerKind::Exe
        );
        assert_eq!(
            installer_kind_from_asset_name("Cloakwire_1.2.1_x64.msi").unwrap(),
            InstallerKind::Msi
        );
        assert_eq!(
            installer_kind_from_asset_name("Cloakwire_1.2.1_aarch64.dmg").unwrap(),
            InstallerKind::Dmg
        );
    }

    #[test]
    fn rejects_unknown_or_unsafe_installer_asset_names() {
        assert!(installer_kind_from_asset_name("Cloakwire_1.2.1.tar.gz").is_err());
        assert!(installer_kind_from_asset_name("../Cloakwire_1.2.1_amd64.deb").is_err());
        assert!(installer_kind_from_asset_name("other_1.2.1_amd64.deb").is_err());
    }

    #[test]
    fn accepts_tagged_manifest_redirect_only() {
        let current = validate_manifest_url(UPDATER_MANIFEST_URL).unwrap();
        let tagged = Url::parse(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/latest.json",
        )
        .unwrap();
        assert!(validate_manifest_redirect(&current, &tagged).is_ok());
    }

    #[test]
    fn rejects_manifest_redirect_path_escape() {
        let current = validate_manifest_url(UPDATER_MANIFEST_URL).unwrap();
        let escaped = Url::parse(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/installer.exe",
        )
        .unwrap();
        assert!(validate_manifest_redirect(&current, &escaped).is_err());
    }

    #[test]
    fn rejects_artifact_redirect_repository_escape() {
        let current = validate_release_asset_url(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe",
        )
        .unwrap();
        let escaped =
            Url::parse("https://github.com/attacker/cloakwire/releases/download/v1.2.3/update.exe")
                .unwrap();
        assert!(validate_artifact_redirect(&current, &escaped).is_err());
    }

    #[test]
    fn rejects_artifact_redirect_path_escape() {
        let current = validate_release_asset_url(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe",
        )
        .unwrap();
        let escaped = Url::parse(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/other.exe",
        )
        .unwrap();
        assert!(validate_artifact_redirect(&current, &escaped).is_err());
    }

    #[test]
    fn accepts_bound_release_assets_redirect() {
        let current = validate_release_asset_url(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe",
        )
        .unwrap();
        let redirect = Url::parse("https://release-assets.githubusercontent.com/github-production-release-asset/123?sig=bound").unwrap();
        assert!(validate_artifact_redirect(&current, &redirect).is_ok());
    }

    #[test]
    fn trusted_update_constants_are_exact_release_routes() {
        assert!(validate_manifest_url(UPDATER_MANIFEST_URL).is_ok());
        let key = base64::engine::general_purpose::STANDARD
            .decode(UPDATER_PUBLIC_KEY)
            .unwrap();
        assert!(String::from_utf8(key)
            .unwrap()
            .starts_with("untrusted comment: minisign public key:"));
    }

    #[test]
    fn accepts_exact_project_release_asset_route() {
        let url = validate_release_asset_url("https://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/Cloakwire_1.2.3_x64-setup.exe").unwrap();
        assert_eq!(url.host_str(), Some("github.com"));
    }

    #[test]
    fn rejects_non_https_update_asset() {
        assert!(validate_release_asset_url(
            "http://github.com/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe"
        )
        .is_err());
    }

    #[test]
    fn rejects_lookalike_update_asset_host() {
        assert!(validate_release_asset_url("https://github.com.attacker.example/markwhite7881-cpu/cloakwire/releases/download/v1.2.3/update.exe").is_err());
    }

    #[test]
    fn rejects_wrong_github_repository_or_route() {
        assert!(validate_release_asset_url(
            "https://github.com/attacker/cloakwire/releases/download/v1.2.3/update.exe"
        )
        .is_err());
        assert!(validate_release_asset_url(
            "https://github.com/markwhite7881-cpu/cloakwire/releases/latest/download/update.exe"
        )
        .is_err());
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
        let manifest: UpdaterManifest =
            serde_json::from_str(r#"{"version":"1.2.3","platforms":{}}"#).unwrap();
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
