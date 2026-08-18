use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

pub const XRAY_LOCATION_ASSET_ENV: &str = "XRAY_LOCATION_ASSET";
const GEODATA_DIR: &str = "xray-geodata";
const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/Loyalsoldier/v2ray-rules-dat/releases/latest";
const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const DATA_FILES: [&str; 2] = ["geoip.dat", "geosite.dat"];
const CHECKSUM_FILES: [&str; 2] = ["geoip.dat.sha256sum", "geosite.dat.sha256sum"];

#[derive(Debug, Clone)]
pub struct GeoDataDir {
    pub path: PathBuf,
}

impl GeoDataDir {
    pub fn env_pair(&self) -> (OsString, OsString) {
        (
            XRAY_LOCATION_ASSET_ENV.into(),
            self.path.as_os_str().to_os_string(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshDecision {
    UseCached,
    Refresh,
}

#[derive(Debug, Clone)]
struct DownloadedPair {
    tag: String,
    geoip: Vec<u8>,
    geosite: Vec<u8>,
    geoip_sha256: String,
    geosite_sha256: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeoDataState {
    checked_at: DateTime<Utc>,
    tag: String,
    geoip_sha256: String,
    geosite_sha256: String,
}

pub async fn ensure(app: &AppHandle) -> AppResult<GeoDataDir> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir)?;
    let cached = read_cached_state(&dir)
        .ok()
        .filter(|_| has_complete_pair(&dir));
    let cache_age = cached.as_ref().and_then(|state| {
        Utc::now()
            .signed_duration_since(state.checked_at)
            .to_std()
            .ok()
    });
    if refresh_decision(cache_age) == RefreshDecision::UseCached {
        return Ok(GeoDataDir { path: dir });
    }

    match fetch_release_pair(&new_http_client()?).await {
        Ok(pair) => match install_pair(&dir, &pair, Utc::now()) {
            Ok(()) => Ok(GeoDataDir { path: dir }),
            Err(_) if cached.is_some() => Ok(GeoDataDir { path: dir }),
            Err(_) => Err(AppError::EngineUnavailable(
                "Xray routing data is unavailable".into(),
            )),
        },
        Err(_) if cached.is_some() => Ok(GeoDataDir { path: dir }),
        Err(_) => Err(AppError::EngineUnavailable(
            "Xray routing data is unavailable".into(),
        )),
    }
}

fn app_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(GEODATA_DIR))
        .map_err(|error| {
            AppError::EngineUnavailable(format!("Xray data directory unavailable: {error}"))
        })
}

fn new_http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("cloakwire/1.3.0")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            let url = attempt.url();
            if url.scheme() != "https"
                || !matches!(
                    url.host_str(),
                    Some("github.com")
                        | Some("githubusercontent.com")
                        | Some("objects.githubusercontent.com")
                        | Some("release-assets.githubusercontent.com")
                )
                || url.username() != ""
                || url.password().is_some()
            {
                return attempt.stop();
            }
            attempt.follow()
        }))
        .build()
        .map_err(|_| AppError::Network("Xray routing data request failed".into()))
}

async fn fetch_release_pair(client: &reqwest::Client) -> AppResult<DownloadedPair> {
    let release = client
        .get(RELEASES_LATEST_URL)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| AppError::Network("Xray routing data request failed".into()))?
        .error_for_status()
        .map_err(|_| AppError::Network("Xray routing data request failed".into()))?
        .json::<GithubRelease>()
        .await
        .map_err(|_| AppError::Network("Xray routing data response was invalid".into()))?;

    let mut selected = Vec::new();
    for name in DATA_FILES.iter().chain(CHECKSUM_FILES.iter()) {
        let matches = release
            .assets
            .iter()
            .filter(|asset| asset.name == *name)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].size > MAX_ASSET_BYTES as u64 {
            return Err(AppError::Network(
                "Xray routing data release is invalid".into(),
            ));
        }
        if !is_trusted_release_asset_url(&matches[0].browser_download_url, &release.tag_name, name)
        {
            return Err(AppError::Network(
                "Xray routing data release is invalid".into(),
            ));
        }
        selected.push(matches[0].clone());
    }

    let geoip = download_bounded(
        client,
        &selected[0].browser_download_url,
        Duration::from_secs(30),
    )
    .await?;
    let geosite = download_bounded(
        client,
        &selected[1].browser_download_url,
        Duration::from_secs(30),
    )
    .await?;
    let geoip_checksum = download_bounded(
        client,
        &selected[2].browser_download_url,
        Duration::from_secs(30),
    )
    .await?;
    let geosite_checksum = download_bounded(
        client,
        &selected[3].browser_download_url,
        Duration::from_secs(30),
    )
    .await?;
    let geoip_sha256 = parse_checksum(&String::from_utf8_lossy(&geoip_checksum), DATA_FILES[0])?;
    let geosite_sha256 =
        parse_checksum(&String::from_utf8_lossy(&geosite_checksum), DATA_FILES[1])?;
    if sha256_hex(&geoip) != geoip_sha256 || sha256_hex(&geosite) != geosite_sha256 {
        return Err(AppError::Network(
            "Xray routing data checksum failed".into(),
        ));
    }
    Ok(DownloadedPair {
        tag: release.tag_name,
        geoip,
        geosite,
        geoip_sha256,
        geosite_sha256,
    })
}

async fn download_bounded(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> AppResult<Vec<u8>> {
    let mut response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|_| AppError::Network("Xray routing data download failed".into()))?
        .error_for_status()
        .map_err(|_| AppError::Network("Xray routing data download failed".into()))?;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::Network("Xray routing data download failed".into()))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_ASSET_BYTES {
            return Err(AppError::Network("Xray routing data is too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn install_pair(dir: &Path, pair: &DownloadedPair, checked_at: DateTime<Utc>) -> AppResult<()> {
    if sha256_hex(&pair.geoip) != pair.geoip_sha256
        || sha256_hex(&pair.geosite) != pair.geosite_sha256
    {
        return Err(AppError::EngineUnavailable(
            "Xray routing data checksum failed".into(),
        ));
    }
    fs::create_dir_all(dir)?;
    let nonce = format!(
        "{}.{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let geoip_tmp = dir.join(format!("geoip.dat.{nonce}.tmp"));
    let geosite_tmp = dir.join(format!("geosite.dat.{nonce}.tmp"));
    let state_tmp = dir.join(format!("state.json.{nonce}.tmp"));
    let state = GeoDataState {
        checked_at,
        tag: pair.tag.clone(),
        geoip_sha256: pair.geoip_sha256.clone(),
        geosite_sha256: pair.geosite_sha256.clone(),
    };
    fs::write(&geoip_tmp, &pair.geoip)?;
    fs::write(&geosite_tmp, &pair.geosite)?;
    fs::write(&state_tmp, serde_json::to_vec_pretty(&state)?)?;
    for path in [&geoip_tmp, &geosite_tmp, &state_tmp] {
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        file.sync_all()?;
    }
    let targets = [
        dir.join("geoip.dat"),
        dir.join("geosite.dat"),
        dir.join("state.json"),
    ];
    let temps = [geoip_tmp.clone(), geosite_tmp.clone(), state_tmp.clone()];
    let previous = [
        dir.join("geoip.dat.previous"),
        dir.join("geosite.dat.previous"),
        dir.join("state.json.previous"),
    ];
    let result = (|| {
        for (target, backup) in targets.iter().zip(previous.iter()) {
            let _ = fs::remove_file(backup);
            if target.exists() {
                fs::rename(target, backup)?;
            }
        }
        for (temp, target) in temps.iter().zip(targets.iter()) {
            fs::rename(temp, target)?;
        }
        for backup in &previous {
            let _ = fs::remove_file(backup);
        }
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        for target in &targets {
            let _ = fs::remove_file(target);
        }
        for (backup, target) in previous.iter().zip(targets.iter()) {
            let _ = fs::rename(backup, target);
        }
    }
    let _ = fs::remove_file(&geoip_tmp);
    let _ = fs::remove_file(&geosite_tmp);
    let _ = fs::remove_file(&state_tmp);
    result.map_err(AppError::Io)
}

fn has_complete_pair(dir: &Path) -> bool {
    let Ok(state) = read_cached_state(dir) else {
        return false;
    };
    let Ok(geoip) = fs::read(dir.join("geoip.dat")) else {
        return false;
    };
    let Ok(geosite) = fs::read(dir.join("geosite.dat")) else {
        return false;
    };
    sha256_hex(&geoip) == state.geoip_sha256 && sha256_hex(&geosite) == state.geosite_sha256
}

fn read_cached_state(dir: &Path) -> AppResult<GeoDataState> {
    Ok(serde_json::from_slice(&fs::read(dir.join("state.json"))?)?)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_trusted_release_asset_url(value: &str, tag: &str, filename: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str() == Some("github.com")
        && url.path() == format!("/Loyalsoldier/v2ray-rules-dat/releases/download/{tag}/{filename}")
        && url.query().is_none()
        && url.fragment().is_none()
}

fn parse_checksum(value: &str, expected_filename: &str) -> AppResult<String> {
    let entries = value
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let filename = parts.next()?;
            (parts.next().is_none()).then_some((hash, filename))
        })
        .filter(|(_, filename)| *filename == expected_filename)
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err(AppError::EngineUnavailable(
            "Xray routing data checksum is invalid".into(),
        ));
    }
    let hash = entries[0].0;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::EngineUnavailable(
            "Xray routing data checksum is invalid".into(),
        ));
    }
    Ok(hash.to_ascii_lowercase())
}

fn refresh_decision(age: Option<Duration>) -> RefreshDecision {
    match age {
        Some(age) if age < REFRESH_AFTER => RefreshDecision::UseCached,
        _ => RefreshDecision::Refresh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn current_geoip_release_fits_the_download_limit() {
        // Loyalsoldier geoip.dat was 17,344,919 bytes on 2026-08-18.
        assert!(MAX_ASSET_BYTES >= 17_344_919);
    }

    #[test]
    fn accepts_only_exact_loyalsoldier_release_asset_route() {
        assert!(is_trusted_release_asset_url("https://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/geoip.dat", "2026.08.18", "geoip.dat"));
        assert!(!is_trusted_release_asset_url("https://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/other.dat", "2026.08.18", "geoip.dat"));
        assert!(!is_trusted_release_asset_url(
            "http://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/geoip.dat",
            "2026.08.18",
            "geoip.dat"
        ));
    }

    #[test]
    fn parses_only_one_exact_checksum_entry() {
        let checksum = format!("{}  geoip.dat\n", "a".repeat(64));
        assert_eq!(
            parse_checksum(&checksum, "geoip.dat").unwrap(),
            "a".repeat(64)
        );
        assert!(parse_checksum(&format!("{}  other.dat\n", "a".repeat(64)), "geoip.dat").is_err());
        assert!(parse_checksum(
            &format!(
                "{}  geoip.dat\n{}  geoip.dat\n",
                "a".repeat(64),
                "b".repeat(64)
            ),
            "geoip.dat"
        )
        .is_err());
    }

    #[test]
    fn refresh_decision_uses_cached_pair_for_less_than_one_day() {
        assert_eq!(
            refresh_decision(Some(Duration::from_secs(86_399))),
            RefreshDecision::UseCached
        );
        assert_eq!(
            refresh_decision(Some(Duration::from_secs(86_400))),
            RefreshDecision::Refresh
        );
        assert_eq!(refresh_decision(None), RefreshDecision::Refresh);
    }

    #[test]
    fn installs_a_verified_pair() {
        let temp = tempfile::tempdir().unwrap();
        let geoip = b"geoip".to_vec();
        let geosite = b"geosite".to_vec();
        let pair = DownloadedPair {
            tag: "test".into(),
            geoip_sha256: sha256_hex(&geoip),
            geosite_sha256: sha256_hex(&geosite),
            geoip,
            geosite,
        };

        install_pair(temp.path(), &pair, Utc::now()).unwrap();

        assert!(has_complete_pair(temp.path()));
    }

    #[test]
    fn failed_candidate_never_replaces_complete_pair() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("geoip.dat"), b"old-geoip").unwrap();
        fs::write(temp.path().join("geosite.dat"), b"old-geosite").unwrap();
        fs::write(temp.path().join("state.json"), b"{}\n").unwrap();
        let pair = DownloadedPair {
            tag: "test".into(),
            geoip: b"new-geoip".to_vec(),
            geosite: b"new-geosite".to_vec(),
            geoip_sha256: "bad".into(),
            geosite_sha256: "bad".into(),
        };
        assert!(install_pair(temp.path(), &pair, Utc::now()).is_err());
        assert_eq!(
            fs::read(temp.path().join("geoip.dat")).unwrap(),
            b"old-geoip"
        );
        assert_eq!(
            fs::read(temp.path().join("geosite.dat")).unwrap(),
            b"old-geosite"
        );
    }

    #[test]
    fn complete_pair_requires_state_record() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("geoip.dat"), b"geoip").unwrap();
        fs::write(temp.path().join("geosite.dat"), b"geosite").unwrap();
        assert!(!has_complete_pair(temp.path()));
    }
}
