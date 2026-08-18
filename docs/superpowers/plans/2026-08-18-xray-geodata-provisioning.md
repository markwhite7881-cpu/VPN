# Xray Geodata Provisioning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provision verified Xray `geoip.dat` and `geosite.dat` into the per-user Cloakwire data directory, refresh them at most once per 24 hours, and make both Xray validation and launch use that same directory.

**Architecture:** Add a focused `engine::xray::geodata` module that owns trusted-release discovery, strict GitHub URL/redirect checks, SHA-256 verification, durable state, and atomic replacement of the asset pair. Xray preparation requests the resulting directory before validation; `LaunchSpec` carries an engine-owned environment map so both `xray run -test` and `xray run` receive `XRAY_LOCATION_ASSET` without exposing it to the WebView.

**Tech Stack:** Rust 2021, Tauri 2 `AppHandle`, `reqwest` 0.12 with rustls, `serde`, `sha2`, `tokio`, `tempfile` test helper, GitHub Releases API.

## Global Constraints

- Desktop Xray fallback only; Android, sing-box primary behavior, subscriptions, routing, profile selection, logs, TUN, and updater behavior remain unchanged.
- User-local assets reside under `<app_data_dir>/xray-geodata`; no `.dat` asset, provider data, URL, HWID, credential, runtime configuration, or secret is committed or returned to the WebView.
- Trust only HTTPS GitHub URLs from `api.github.com`, `github.com/Loyalsoldier/v2ray-rules-dat/releases/download/<tag>/`, and bounded GitHub release-asset redirects.
- Exact allowed names: `geoip.dat`, `geosite.dat`, `geoip.dat.sha256sum`, `geosite.dat.sha256sum`.
- Enforce a 15-second API timeout, 30-second download timeout, 5 redirects, and a 16 MiB maximum per geodata asset/checksum response.
- Verify each downloaded file against the checksum that belongs to the exact filename before atomic replacement; never replace a known-good pair with an incomplete/invalid pair.
- A verified pair younger than 24 hours must not make a network request. On stale refresh failure, use an existing complete verified pair; initial provisioning failure stays sanitized.
- Use `XRAY_LOCATION_ASSET` for both validation and process launch. Preserve existing filtered Xray log behavior.
- Run format, focused tests, full Rust tests, and real Windows Xray smoke checks before claiming completion.

---

### Task 1: Add a focused geodata provisioning module and pure safety tests

**Files:**
- Create: `src-tauri/src/engine/xray/geodata.rs`
- Modify: `src-tauri/src/engine/xray.rs:1-145`
- Modify: `src-tauri/src/lib.rs:18-29`

**Interfaces:**
- Produces `pub async fn ensure(app: &AppHandle) -> AppResult<GeoDataDir>`.
- Produces `#[derive(Debug, Clone)] pub struct GeoDataDir { pub path: PathBuf }`.
- Produces `pub const XRAY_LOCATION_ASSET_ENV: &str = "XRAY_LOCATION_ASSET"`.
- Consumes `tauri::Manager` for `app_data_dir`, `reqwest`, `sha2`, and existing `AppError`/`AppResult`.
- Later tasks consume `GeoDataDir::env_pair() -> (OsString, OsString)` and `ensure`.

- [ ] **Step 1: Write failing pure tests for the trust boundary and checksum parser**

In `src-tauri/src/engine/xray/geodata.rs`, add a `#[cfg(test)] mod tests` that constructs no network client. Add tests with these exact assertions:

```rust
#[test]
fn accepts_only_exact_loyalsoldier_release_asset_route() {
    assert!(is_trusted_release_asset_url(
        "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/geoip.dat",
        "2026.08.18",
        "geoip.dat"
    ));
    assert!(!is_trusted_release_asset_url(
        "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/other.dat",
        "2026.08.18",
        "geoip.dat"
    ));
    assert!(!is_trusted_release_asset_url(
        "http://github.com/Loyalsoldier/v2ray-rules-dat/releases/download/2026.08.18/geoip.dat",
        "2026.08.18",
        "geoip.dat"
    ));
}

#[test]
fn parses_only_one_exact_checksum_entry() {
    let checksum = format!("{}  geoip.dat\n", "a".repeat(64));
    assert_eq!(parse_checksum(&checksum, "geoip.dat").unwrap(), "a".repeat(64));
    assert!(parse_checksum(&format!("{}  other.dat\n", "a".repeat(64)), "geoip.dat").is_err());
    assert!(parse_checksum(&format!("{}  geoip.dat\n{}  geoip.dat\n", "a".repeat(64), "b".repeat(64)), "geoip.dat").is_err());
}

#[test]
fn refresh_decision_uses_cached_pair_for_less_than_one_day() {
    assert_eq!(refresh_decision(Some(Duration::from_secs(86_399))), RefreshDecision::UseCached);
    assert_eq!(refresh_decision(Some(Duration::from_secs(86_400))), RefreshDecision::Refresh);
    assert_eq!(refresh_decision(None), RefreshDecision::Refresh);
}
```

- [ ] **Step 2: Run the new tests and verify compilation fails before implementation**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo test engine::xray::geodata::tests --lib
```

Expected: FAIL because `geodata` module/functions/types do not exist.

- [ ] **Step 3: Implement pure types, path construction, strict URL checks, checksum parsing, and refresh decision**

Create `engine/xray/geodata.rs` with these core declarations:

```rust
pub const XRAY_LOCATION_ASSET_ENV: &str = "XRAY_LOCATION_ASSET";
const GEODATA_DIR: &str = "xray-geodata";
const RELEASES_LATEST_URL: &str = "https://api.github.com/repos/Loyalsoldier/v2ray-rules-dat/releases/latest";
const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const DATA_FILES: [&str; 2] = ["geoip.dat", "geosite.dat"];

#[derive(Debug, Clone)]
pub struct GeoDataDir { pub path: PathBuf }

impl GeoDataDir {
    pub fn env_pair(&self) -> (OsString, OsString) {
        (XRAY_LOCATION_ASSET_ENV.into(), self.path.as_os_str().to_os_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshDecision { UseCached, Refresh }
```

Implement `app_data_dir(app) -> AppResult<PathBuf>` as `app.path().app_data_dir()?.join(GEODATA_DIR)`. Implement `is_trusted_release_asset_url`, `is_trusted_redirect_url`, `parse_checksum`, `sha256_hex`, and `refresh_decision` with no browser-controlled input. Reject duplicate checksum entries, `*filename` marker syntax, malformed hashes, non-HTTPS links, query strings, fragments, credentials, and a tag/filename mismatch.

- [ ] **Step 4: Run focused tests and format**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo fmt --check
cargo test engine::xray::geodata::tests --lib
```

Expected: PASS.

- [ ] **Step 5: Commit the module safety baseline**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git add -- 'src-tauri/src/engine/xray.rs' 'src-tauri/src/engine/xray/geodata.rs'
git commit -m 'feat: add Xray geodata trust policy'
```

### Task 2: Implement verified release retrieval and atomic local persistence

**Files:**
- Modify: `src-tauri/src/engine/xray/geodata.rs`

**Interfaces:**
- Consumes the pure helpers from Task 1.
- Produces `ensure(app)` returning `GeoDataDir` only when both files form a complete verified pair.
- Produces `async fn fetch_release_pair(client: &reqwest::Client) -> AppResult<DownloadedPair>` for testable I/O separation.
- Produces `fn install_pair(dir: &Path, pair: &DownloadedPair, checked_at: DateTime<Utc>) -> AppResult<()>`.

- [ ] **Step 1: Write failing persistence and stale-fallback tests**

Add tests that use `tempfile::tempdir()` and construct `DownloadedPair` with fixed bytes/hashes:

```rust
#[test]
fn failed_candidate_never_replaces_complete_pair() {
    let temp = tempfile::tempdir().unwrap();
    write_valid_pair(temp.path(), b"old-geoip", b"old-geosite");
    let invalid = DownloadedPair::for_test(b"new-geoip", b"new-geosite", false);
    assert!(install_pair(temp.path(), &invalid, Utc::now()).is_err());
    assert_eq!(std::fs::read(temp.path().join("geoip.dat")).unwrap(), b"old-geoip");
    assert_eq!(std::fs::read(temp.path().join("geosite.dat")).unwrap(), b"old-geosite");
}

#[test]
fn complete_pair_is_usable_when_refresh_is_stale_or_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_valid_pair(temp.path(), b"geoip", b"geosite");
    assert!(has_complete_pair(temp.path()));
}
```

- [ ] **Step 2: Run the persistence tests and verify they fail**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo test engine::xray::geodata::tests --lib
```

Expected: FAIL because pair persistence helpers are missing.

- [ ] **Step 3: Implement release retrieval with bounded response bodies**

Add private release structs:

```rust
#[derive(Debug, Deserialize)]
struct GithubRelease { tag_name: String, assets: Vec<GithubAsset> }
#[derive(Debug, Clone, Deserialize)]
struct GithubAsset { name: String, browser_download_url: String, size: u64 }
```

Implement `fetch_latest_release` with the exact `RELEASES_LATEST_URL`, `cloakwire/<version>` user agent, 15-second timeout, and no caller-provided URL. Select exactly one matching asset for each name in `DATA_FILES` and its `.sha256sum` companion. Download with `reqwest::redirect::Policy::none()`, a 30-second timeout, at most five validated redirects, and a streaming byte cap of `MAX_ASSET_BYTES`; do not use an unbounded `.bytes()` response. Parse the two checksum bodies, hash the two data bodies, and return only a fully verified `DownloadedPair`.

- [ ] **Step 4: Implement atomic pair persistence and state**

Implement `state.json` as:

```rust
#[derive(Debug, Serialize, Deserialize)]
struct GeoDataState {
    checked_at: DateTime<Utc>,
    tag: String,
    geoip_sha256: String,
    geosite_sha256: String,
}
```

Write both verified candidates and `state.json` to unique same-directory temporary paths, `sync_all` each file, rename the existing canonical files to `*.previous`, rename both candidates and state into place, then remove previous files. If any rename fails, restore any previous canonical file before returning an error. Treat the cache as complete only if both canonical data files and a parseable state record exist.

Implement `ensure(app)` as:

```rust
pub async fn ensure(app: &AppHandle) -> AppResult<GeoDataDir> {
    let dir = app_data_dir(app)?;
    std::fs::create_dir_all(&dir)?;
    let cached = read_cached_state(&dir).ok().filter(|_| has_complete_pair(&dir));
    if cached
        .as_ref()
        .is_some_and(|state| Utc::now().signed_duration_since(state.checked_at) < ChronoDuration::hours(24))
    {
        return Ok(GeoDataDir { path: dir });
    }
    match fetch_release_pair(&new_http_client()?).await.and_then(|pair| install_pair(&dir, &pair, Utc::now())) {
        Ok(()) => Ok(GeoDataDir { path: dir }),
        Err(_) if cached.is_some() => Ok(GeoDataDir { path: dir }),
        Err(_) => Err(AppError::EngineUnavailable("Xray routing data is unavailable".into())),
    }
}
```

Internally log only generic diagnostic categories; never log profile data or paths to frontend-accessible logs.

- [ ] **Step 5: Run module tests and the full Rust suite**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo fmt --check
cargo test engine::xray::geodata::tests --lib
cargo test --lib
```

Expected: PASS.

- [ ] **Step 6: Commit verified persistence**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git add -- 'src-tauri/src/engine/xray/geodata.rs'
git commit -m 'feat: provision verified Xray geodata'
```

### Task 3: Thread the user-local asset directory into Xray validation and launch

**Files:**
- Modify: `src-tauri/src/engine/mod.rs:1-18`
- Modify: `src-tauri/src/engine/xray.rs:67-100`
- Modify: `src-tauri/src/process.rs:246-257`
- Modify: `src-tauri/src/process.rs:1415-1436`
- Modify: `src-tauri/src/commands.rs:178-236,350-375,443-454`

**Interfaces:**
- Consumes `geodata::ensure(&app).await?` and `GeoDataDir::env_pair()` from Task 2.
- Extends `LaunchSpec` with `pub environment: Vec<(OsString, OsString)>`.
- Changes `xray::validate_config(binary, config_path, environment)` to receive the same environment used for launch.
- Produces no new frontend command or browser-visible path.

- [ ] **Step 1: Write failing tests for asset environment propagation**

In `engine/xray.rs` tests, add:

```rust
#[test]
fn xray_asset_environment_uses_the_standard_variable() {
    let dir = GeoDataDir { path: PathBuf::from("C:/state/xray-geodata") };
    let (name, value) = dir.env_pair();
    assert_eq!(name, XRAY_LOCATION_ASSET_ENV);
    assert_eq!(value, PathBuf::from("C:/state/xray-geodata").into_os_string());
}
```

In `process.rs` tests, update `test_xray_spec()` to set `environment` and add a test for the helper that applies environment values to a `tokio::process::Command` without modifying the parent process environment.

- [ ] **Step 2: Run focused tests and verify failure**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo test 'engine::xray::tests::xray_asset_environment_uses_the_standard_variable' --lib
```

Expected: FAIL because `GeoDataDir` and `LaunchSpec::environment` wiring are absent.

- [ ] **Step 3: Extend LaunchSpec and apply environment only to spawned children**

In `engine/mod.rs`:

```rust
pub environment: Vec<(OsString, OsString)>,
```

In `process.rs`, immediately after `Command::new(&spec.binary)`, add:

```rust
cmd.envs(&spec.environment);
```

Populate `environment: Vec::new()` at every sing-box `LaunchSpec` creation. Keep environment ownership in the spec; do not call `std::env::set_var`.

In `engine/xray.rs`, change validation to accept `environment: &[(OsString, OsString)]` and call `command.envs(environment)` before `.output()`.

- [ ] **Step 4: Provision assets before validating an Xray ready profile**

In `commands.rs` inside the `EngineKind::Xray` branch, after `xray::locate_binary(&app)?` and before constructing the return tuple:

```rust
let geodata = xray::geodata::ensure(&app).await?;
let environment = vec![geodata.env_pair()];
```

Carry `environment` through the tuple. Pass it to `xray::validate_config(&binary, &path, &environment).await` and into the Xray `LaunchSpec`. Do not provision geodata for the sing-box branch.

Update the existing compatibility launch path that can start `EngineKind::Xray` to either provision geodata first or reject raw Xray config launch with the same sanitized engine error; it must never run an Xray profile without the asset environment.

- [ ] **Step 5: Run focused and full tests**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo fmt --check
cargo test engine::xray::tests --lib
cargo test process::tests --lib
cargo test --lib
```

Expected: PASS.

- [ ] **Step 6: Commit runtime integration**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git add -- 'src-tauri/src/engine/mod.rs' 'src-tauri/src/engine/xray.rs' 'src-tauri/src/engine/xray/geodata.rs' 'src-tauri/src/process.rs' 'src-tauri/src/commands.rs'
git commit -m 'feat: supply verified geodata to Xray profiles'
```

### Task 4: Add provenance/attribution and perform Windows evidence checks

**Files:**
- Modify: `README.md` (or create `THIRD_PARTY_NOTICES.md` if no appropriate dependency notice section exists)
- Modify: `docs/superpowers/specs/2026-08-18-xray-geodata-provisioning-design.md` only if an implementation detail changed from the approved design

**Interfaces:**
- Consumes completed Tasks 1-3.
- Produces a concise attribution for `Loyalsoldier/v2ray-rules-dat` and the license link/reference without copying the geodata files into the repository.

- [ ] **Step 1: Add attribution without copying assets**

Add a short notice naming `Loyalsoldier/v2ray-rules-dat`, explaining that only user-local, checksum-verified geodata is retrieved at runtime for Xray fallback, and link/reference its upstream license. Do not add downloaded assets, checksums, URLs containing credentials, or release artifacts to Git.

- [ ] **Step 2: Run static repository hygiene checks**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git diff --check
git status --short
Get-ChildItem -Path 'src-tauri\binaries','src-tauri\target\debug' -Filter '*.dat' -File -ErrorAction SilentlyContinue
```

Expected: no whitespace errors; no `.dat` files under sidecar/build directories; downloaded data is only in `%APPDATA%`/Tauri app data.

- [ ] **Step 3: Build and run the real Windows Xray smoke test**

1. Confirm the debug Xray sidecar hash still matches `scripts/xray-core.json` with `scripts/test-xray-sidecar.ps1`.
2. Temporarily move any existing user-local `xray-geodata` cache aside, never into the repository.
3. Start Vite and confirm `http://localhost:1420` returns HTTP 200 before opening the elevated debug executable.
4. Start a ready Xray subscription profile through the UI. Verify that the pair appears under the app-data directory and that `xray run -test` completes through the application without exposing its raw output in the UI.
5. Confirm Xray reaches Running, system proxy is set only after the child starts, Stop clears the proxy, and a second Start/Stop succeeds.
6. Validate all available ready Xray profiles with the real sidecar. Record only counts and pass/fail; do not persist or display raw profiles, hostnames, UUIDs, provider URLs, or runtime config paths.

- [ ] **Step 4: Run the full Windows Rust suite**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray\src-tauri'
cargo test --lib
```

Expected: all tests pass.

- [ ] **Step 5: Commit attribution and test-related source/docs only**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git add -- 'README.md' 'THIRD_PARTY_NOTICES.md' 'docs/superpowers/specs/2026-08-18-xray-geodata-provisioning-design.md'
git commit -m 'docs: attribute Xray geodata source'
```

### Task 5: Review and integration preparation

**Files:**
- Review only: all files changed by Tasks 1-4

**Interfaces:**
- Consumes passing tests and Windows smoke evidence.
- Produces a clean feature branch suitable for independent review and PR creation.

- [ ] **Step 1: Inspect scope against the approved design**

Run:

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git diff --check origin/main...HEAD
git diff --stat origin/main...HEAD
git status --short --branch
```

Verify changed paths are limited to geodata module/runtime wiring/tests/attribution plus the already-approved HWID/Xray branch work; confirm no generated data, binaries, credentials, or unrelated dirty files are staged.

- [ ] **Step 2: Run the independent review gate**

Use the repository's review workflow or a verifier agent on `origin/main...HEAD`; require findings to reference `file:line`. Address only confirmed in-scope defects, then rerun the affected tests.

- [ ] **Step 3: Push and open a PR only after all gates pass**

```powershell
Set-Location 'C:\Users\Public\cwdev\cloakwire-hwid-xray'
git push origin feature/hwid-xray-subscriptions
```

Create a PR whose description includes: safe user-local geodata provisioning, source provenance, 24-hour refresh behavior, no-bundle/no-WebView-data boundary, test command output, Windows smoke result, and the fact that generated geodata remains untracked. Do not merge until review and CI complete.
