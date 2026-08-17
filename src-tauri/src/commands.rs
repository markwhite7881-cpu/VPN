//! Tauri commands exposed to the frontend.
//!
//! Every command is async and returns `AppResult<T>`. The frontend
//! sees errors as `{ kind, message }` (see error.rs).

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::config::{self, GeneratorSettings};
use crate::error::{AppError, AppResult};
use crate::parser::{self, Outbound};
use crate::process::{LogLine, ProcessManager, StatusReport};
use crate::subscriptions::{
    AddSubscriptionInput, LegacySubscriptionInput, RefreshSubscriptionResult, SubscriptionService,
    SubscriptionSnapshot, SubscriptionSummary,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingboxVersion {
    pub version: String,
    pub environment: String,
    pub revision: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryInfo {
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn get_binary_info(app: AppHandle) -> AppResult<BinaryInfo> {
    match ProcessManager::locate_binary(&app) {
        Ok(p) => {
            let meta = std::fs::metadata(&p).ok();
            Ok(BinaryInfo {
                path: p.display().to_string(),
                exists: true,
                size_bytes: meta.map(|m| m.len()).unwrap_or(0),
            })
        }
        Err(_) => Ok(BinaryInfo {
            path: String::new(),
            exists: false,
            size_bytes: 0,
        }),
    }
}

#[tauri::command]
pub async fn get_singbox_version(app: AppHandle) -> AppResult<SingboxVersion> {
    let binary = ProcessManager::locate_binary(&app)?;
    // `sing-box version` is a console-mode binary; without CREATE_NO_WINDOW
    // it pops a black CMD window for a fraction of a second every time
    // the frontend polls. Hide it the same way we do for the long-lived
    // `sing-box run` child in process.rs.
    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("version");
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Spawn(format!("version probe failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut version = String::new();
    let mut env = String::new();
    let mut revision = String::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("sing-box version ") {
            version = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Environment: ") {
            env = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Revision: ") {
            revision = rest.trim().to_string();
        }
    }
    Ok(SingboxVersion {
        version,
        environment: env,
        revision,
        raw: stdout,
    })
}

#[tauri::command]
pub async fn check_config(app: AppHandle, config_path: String) -> AppResult<String> {
    let binary = ProcessManager::locate_binary(&app)?;
    // Same CREATE_NO_WINDOW dance — `sing-box check -c <path>` is a
    // short-lived console-mode spawn and otherwise flashes a CMD window.
    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("check").arg("-c").arg(&config_path);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Spawn(format!("check failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(AppError::Spawn(format!(
            "config check failed (exit {}): {}{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr
        )));
    }
    Ok(format!("{}{}", stdout, stderr))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManagedLaunchInput {
    pub manual_outbounds: Vec<Outbound>,
    pub subscription_links: Option<Vec<crate::subscriptions::SubscriptionLinkRef>>,
    pub select_all_subscription_links: bool,
    pub settings: GeneratorSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedLaunchResult {
    pub status: StatusReport,
    pub config_path: String,
    pub profile_count: usize,
}

/// Resolve opaque subscription references, merge them with manual profiles,
/// and start through the existing sing-box lifecycle.
#[tauri::command]
pub async fn start_managed_singbox(
    app: AppHandle,
    pm: State<'_, Arc<ProcessManager>>,
    subscriptions: State<'_, Arc<SubscriptionService>>,
    input: ManagedLaunchInput,
) -> AppResult<ManagedLaunchResult> {
    let mut outbounds = input.manual_outbounds;
    let subscription_outbounds = if input.select_all_subscription_links {
        subscriptions.resolve_all_links().await?
    } else {
        subscriptions
            .resolve_link_refs(input.subscription_links.as_deref().unwrap_or_default())
            .await?
    };
    outbounds.extend(subscription_outbounds);
    let outbounds = deduplicate_outbounds(outbounds)?;
    let value = config::Config::build(&outbounds, &input.settings);
    let body = serde_json::to_vec_pretty(&value).map_err(AppError::Serde)?;
    let dir = app.path().temp_dir().unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let path = dir.join("config.managed.json");
    std::fs::write(&path, body).map_err(AppError::Io)?;
    let controller_url = format!("http://{}", input.settings.clash_api.external_controller);
    let binary = ProcessManager::locate_binary(&app)?;
    let status = pm
        .start(&app, &binary, &path, Some(controller_url))
        .await?;
    Ok(ManagedLaunchResult {
        status,
        config_path: path.display().to_string(),
        profile_count: outbounds.len(),
    })
}

/// Deduplicate complete outbound definitions without exposing them outside
/// Rust. Serialization is immediately hashed and never returned or logged.
fn deduplicate_outbounds(outbounds: Vec<Outbound>) -> AppResult<Vec<Outbound>> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(outbounds.len());
    for outbound in outbounds {
        let encoded = serde_json::to_vec(&outbound).map_err(AppError::Serde)?;
        let digest = format!("{:x}", Sha256::digest(encoded));
        if seen.insert(digest) {
            unique.push(outbound);
        }
    }
    Ok(unique)
}

#[tauri::command]
pub async fn start_singbox(
    app: AppHandle,
    pm: State<'_, Arc<ProcessManager>>,
    config_path: String,
) -> AppResult<StatusReport> {
    start_singbox_with_config(app, pm, config_path, None).await
}

/// Like `start_singbox` but also records the Clash API controller URL
/// so subsequent `clash_*` commands know where to talk to.
#[tauri::command]
pub async fn start_singbox_with_config(
    app: AppHandle,
    pm: State<'_, Arc<ProcessManager>>,
    config_path: String,
    controller_url: Option<String>,
) -> AppResult<StatusReport> {
    let binary = ProcessManager::locate_binary(&app)?;
    let cfg = PathBuf::from(&config_path);
    if !cfg.exists() {
        return Err(AppError::WriteConfig(format!(
            "config file does not exist: {config_path}"
        )));
    }
    pm.start(&app, &binary, &cfg, controller_url).await
}

#[tauri::command]
pub async fn stop_singbox(pm: State<'_, Arc<ProcessManager>>) -> AppResult<StatusReport> {
    pm.stop().await
}

#[tauri::command]
pub async fn get_status(pm: State<'_, Arc<ProcessManager>>) -> AppResult<StatusReport> {
    Ok(pm.snapshot_status().await)
}

#[tauri::command]
pub async fn get_logs(
    pm: State<'_, Arc<ProcessManager>>,
    limit: Option<usize>,
) -> AppResult<Vec<LogLine>> {
    Ok(pm.snapshot_logs(limit.unwrap_or(500)).await)
}

#[tauri::command]
pub async fn is_running(pm: State<'_, Arc<ProcessManager>>) -> AppResult<bool> {
    Ok(pm.is_running().await)
}

#[tauri::command]
pub async fn get_current_config(pm: State<'_, Arc<ProcessManager>>) -> AppResult<Option<String>> {
    Ok(pm.current_config().await.map(|p| p.display().to_string()))
}

/// Marker command the frontend can call to confirm the IPC layer is alive.
#[tauri::command]
pub async fn ping() -> AppResult<String> {
    Ok("pong".to_string())
}

/// Write a minimal default config to the OS temp directory and return the path.
///
/// The frontend uses this for the very first "just make it boot" smoke test
/// before subscription parsing lands.
#[tauri::command]
pub async fn write_default_config(app: AppHandle) -> AppResult<String> {
    let dir = app
        .path()
        .temp_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let path = dir.join("config.default.json");
    let body = serde_json::json!({
        "log": { "level": "info", "timestamp": true },
        "inbounds": [{
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": 2080
        }],
        "outbounds": [{ "type": "direct", "tag": "direct" }],
        "route": {
            "rules": [{ "ip_version": 6, "action": "reject" }]
        },
        "experimental": {
            "clash_api": {
                "default_mode": "proxy",
                "external_controller": "127.0.0.1:9090"
            },
            "cache_file": { "path": "cache.db", "store_fakeip": false }
        }
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&body).map_err(AppError::Serde)?,
    )
    .map_err(AppError::Io)?;
    Ok(path.display().to_string())
}

/// Force a state into Stopped (used for tests / manual recovery).
#[tauri::command]
pub async fn reset_state(pm: State<'_, Arc<ProcessManager>>) -> AppResult<StatusReport> {
    pm.reset().await;
    Ok(pm.snapshot_status().await)
}

// --- Link parser --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseFailure {
    pub line: String,
    pub error: parser::ParseError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseLinksResult {
    pub outbounds: Vec<Outbound>,
    pub failures: Vec<ParseFailure>,
}

#[tauri::command]
pub async fn parse_link(link: String) -> AppResult<Outbound> {
    parser::parse_link(&link).map_err(AppError::from)
}

#[tauri::command]
pub async fn parse_links(text: String) -> AppResult<ParseLinksResult> {
    let mut outbounds = Vec::new();
    let mut failures = Vec::new();

    // We need per-line reporting, so walk the input ourselves.
    let (raw_lines, was_base64) = split_subscription(&text);
    for line in raw_lines {
        match parser::parse_link(&line) {
            Ok(ob) => outbounds.push(ob),
            Err(e) => failures.push(ParseFailure { line, error: e }),
        }
    }
    let _ = was_base64; // currently informational only
    Ok(ParseLinksResult {
        outbounds,
        failures,
    })
}

/// Parse a user-typed blob that may contain a mix of share-links and
/// subscription URLs. HTTP(S) URLs are surfaced as `Subscription`s in
/// the failures list with a dedicated `kind: "subscription"` so the
/// UI can promote them to the subscriptions list instead of treating
/// them as parse errors.
#[tauri::command]
pub async fn parse_input(text: String) -> AppResult<ParsedInput> {
    let mut outbounds = Vec::new();
    let mut subscriptions = Vec::new();
    let mut failures = Vec::new();

    let (raw_lines, _was_base64) = split_subscription(&text);
    for line in raw_lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            // Treat as a subscription URL — the UI will add it via
            // useSubscriptions.add() once the user reviews.
            subscriptions.push(line);
        } else {
            match parser::parse_link(&line) {
                Ok(ob) => outbounds.push(ob),
                Err(e) => failures.push(ParseFailure { line, error: e }),
            }
        }
    }
    Ok(ParsedInput {
        outbounds,
        subscriptions,
        failures,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedInput {
    pub outbounds: Vec<Outbound>,
    /// HTTP(S) URLs discovered in the input that should be added as
    /// subscription entries rather than parsed as share-links.
    pub subscriptions: Vec<String>,
    pub failures: Vec<ParseFailure>,
}

#[tauri::command]
pub async fn outbound_to_singbox_json(outbound: Outbound) -> AppResult<serde_json::Value> {
    Ok(outbound.to_singbox_json())
}

// --- Config generator ---------------------------------------------------

/// Build a complete sing-box config from the given outbounds + settings.
/// Returns the config as a `serde_json::Value` (also serialised to the
/// frontend as plain JSON). The frontend decides whether to display,
/// copy or persist it.
#[tauri::command]
pub async fn generate_config(
    outbounds: Vec<Outbound>,
    settings: GeneratorSettings,
) -> AppResult<serde_json::Value> {
    Ok(config::Config::build(&outbounds, &settings))
}

/// Persist a config JSON to a file. If `path` is `None`, writes to
/// `<temp_dir>/config.generated.json` and returns that path.
#[tauri::command]
pub async fn save_config_to_path(
    app: AppHandle,
    content: serde_json::Value,
    path: Option<String>,
) -> AppResult<String> {
    let body = serde_json::to_vec_pretty(&content).map_err(AppError::Serde)?;
    let path = match path {
        Some(p) if !p.trim().is_empty() => std::path::PathBuf::from(p),
        _ => {
            let dir = app
                .path()
                .temp_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            dir.join("config.generated.json")
        }
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(AppError::Io)?;
        }
    }
    std::fs::write(&path, body).map_err(AppError::Io)?;
    Ok(path.display().to_string())
}

/// Validate a config JSON with `sing-box check` and return the output.
#[tauri::command]
pub async fn check_config_with_binary(
    app: AppHandle,
    config: serde_json::Value,
) -> AppResult<String> {
    // Write to a temp file (sing-box needs a path), then ask the
    // sidecar to validate it.
    let dir = app
        .path()
        .temp_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let path = dir.join("config.check.json");
    let body = serde_json::to_vec_pretty(&config).map_err(AppError::Serde)?;
    std::fs::write(&path, body).map_err(AppError::Io)?;
    check_config(app, path.display().to_string()).await
}

// --- Clash API ---------------------------------------------------------

async fn api_url(pm: &ProcessManager) -> AppResult<String> {
    match pm.controller_url().await {
        Some(u) => Ok(u),
        None => Err(AppError::Clash("sing-box is not running".to_string())),
    }
}

#[tauri::command]
pub async fn list_proxies(
    pm: State<'_, Arc<ProcessManager>>,
) -> AppResult<crate::clash_api::ProxiesResponse> {
    let base = api_url(&pm).await?;
    let client = crate::clash_api::Client::new();
    client.list_proxies(&base).await
}

#[tauri::command]
pub async fn select_proxy(
    pm: State<'_, Arc<ProcessManager>>,
    group: String,
    member: String,
) -> AppResult<()> {
    let base = api_url(&pm).await?;
    let client = crate::clash_api::Client::new();
    client.select_proxy(&base, &group, &member).await
}

#[tauri::command]
pub async fn test_delay(
    pm: State<'_, Arc<ProcessManager>>,
    name: String,
    timeout_ms: Option<u32>,
) -> AppResult<Option<u32>> {
    let base = api_url(&pm).await?;
    let client = crate::clash_api::Client::new();
    client
        .test_delay(&base, &name, timeout_ms.unwrap_or(3000))
        .await
}

/// Direct TCP-connect ping, independent of sing-box.
///
/// Used by the frontend's per-server latency probe so the user can
/// see the best server *before* they connect — `test_delay` only
/// works while sing-box is running because it goes through the clash
/// API on 127.0.0.1:9090. We measure the time it takes to complete
/// a `TcpStream::connect()` on the outbound's `host:port` instead;
/// for VLESS/Xray protocols this isn't a real round-trip, but it
/// correlates well with TCP reachability and is a good enough
/// signal to rank servers by latency in the picker.
#[tauri::command]
pub async fn ping_endpoint(
    host: String,
    port: u16,
    timeout_ms: Option<u32>,
) -> AppResult<Option<u32>> {
    use std::time::{Duration, Instant};
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(2000) as u64);
    let start = Instant::now();
    let res = tokio::time::timeout(
        timeout,
        tokio::net::TcpStream::connect((host.as_str(), port)),
    )
    .await;
    match res {
        Ok(Ok(_stream)) => Ok(Some(start.elapsed().as_millis() as u32)),
        // Both timeout and connect-error mean "unreachable" — we
        // return Ok(None) so the UI can show "—" instead of a hard
        // error. The probe loop already filters these.
        Ok(Err(_)) | Err(_) => Ok(None),
    }
}

/// Batch IP → ISO country-code lookup, using the free
/// `ip-api.com` HTTP endpoint. The frontend caches the result in
/// `localStorage` keyed by IP, so the second lookup is free and
/// the third+ lookups are incremental.
///
/// We hit `/batch` (one HTTP call for up to 100 IPs) and only
/// request `query` + `countryCode`. Privacy note: ip-api sees the
/// caller's egress IP, not the user's traffic — it just answers
/// "where is this IP geolocated" for the IPs we hand it.
#[tauri::command]
pub async fn lookup_geoip(ips: Vec<String>) -> AppResult<Vec<(String, String)>> {
    if ips.is_empty() {
        return Ok(Vec::new());
    }
    // Up to 100 per request (ip-api's batch limit).
    let chunk: Vec<String> = ips.into_iter().take(100).collect();
    let body = serde_json::json!(chunk
        .iter()
        .map(|ip| serde_json::json!({ "query": ip }))
        .collect::<Vec<_>>());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .user_agent("cloakwire/0.1")
        .build()
        .map_err(|e| AppError::Clash(format!("http client: {e}")))?;
    // `system` resolver is fine for a hostname like ip-api.com that
    // is a public, well-known endpoint. No DoH needed here.
    let resp = client
        .post("http://ip-api.com/batch?fields=query,countryCode,status")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Clash(format!("ip-api request: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::Clash(format!("ip-api http {status}")));
    }
    let arr: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| AppError::Clash(format!("ip-api decode: {e}")))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let ip = item.get("query").and_then(|v| v.as_str()).unwrap_or("");
        if item.get("status").and_then(|v| v.as_str()) != Some("success") {
            continue;
        }
        if let Some(code) = item.get("countryCode").and_then(|v| v.as_str()) {
            if !code.is_empty() && code.len() == 2 {
                out.push((ip.to_string(), code.to_uppercase()));
            }
        }
    }
    Ok(out)
}

// --- Traffic stream -----------------------------------------------------
//
// The traffic stream is auto-started by `start_singbox_with_config`
// when a controller URL is known. These commands are exposed mostly
// for manual control / debugging.

#[tauri::command]
pub async fn start_traffic(app: AppHandle, pm: State<'_, Arc<ProcessManager>>) -> AppResult<()> {
    let base = api_url(&pm).await?;
    pm.traffic().start(app, &base).await
}

#[tauri::command]
pub async fn stop_traffic(pm: State<'_, Arc<ProcessManager>>) -> AppResult<()> {
    pm.traffic().stop().await;
    Ok(())
}

// --- System proxy (Windows only) -------------------------------------
//
// When sing-box runs in system_proxy mode we have to also tell
// Windows to route HTTP/HTTPS traffic through the local listener,
// otherwise nothing reaches 127.0.0.1:<port>. These two thin wrappers
// toggle the WinINET registry value and are invoked from onStart /
// onStop in App.tsx.

#[tauri::command]
pub async fn apply_system_proxy(host: String, port: u16) -> AppResult<()> {
    crate::process::apply_system_proxy(&host, port)
}

#[tauri::command]
pub async fn clear_system_proxy() -> AppResult<()> {
    crate::process::clear_system_proxy()
}

// --- Windows autostart -------------------------------------------------
//
// Thin wrapper around `tauri-plugin-autostart`. We expose
// `get_autostart` / `set_autostart` instead of letting the frontend
// poke the plugin directly so the surface stays symmetrical with the
// other commands and we can swap backends later (e.g. Task Scheduler).

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    mgr.is_enabled()
        .map_err(|e| AppError::Clash(format!("autostart probe failed: {e}")))
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn get_autostart(_app: AppHandle) -> AppResult<bool> {
    Err(AppError::Unsupported("autostart".to_string()))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable()
            .map_err(|e| AppError::Clash(format!("autostart enable failed: {e}")))?;
    } else {
        mgr.disable()
            .map_err(|e| AppError::Clash(format!("autostart disable failed: {e}")))?;
    }
    mgr.is_enabled()
        .map_err(|e| AppError::Clash(format!("autostart recheck failed: {e}")))
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn set_autostart(_app: AppHandle, _enabled: bool) -> AppResult<bool> {
    Err(AppError::Unsupported("autostart".to_string()))
}

// --- Subscriptions -----------------------------------------------------

#[tauri::command]
pub async fn list_subscriptions(
    subscriptions: State<'_, Arc<SubscriptionService>>,
) -> AppResult<SubscriptionSnapshot> {
    subscriptions.list().await
}

#[tauri::command]
pub async fn add_subscription(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    input: AddSubscriptionInput,
) -> AppResult<RefreshSubscriptionResult> {
    subscriptions.add(input).await
}

#[tauri::command]
pub async fn remove_subscription(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    id: String,
) -> AppResult<()> {
    subscriptions.remove(&id).await
}

#[tauri::command]
pub async fn set_subscription_interval(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    id: String,
    interval_minutes: u32,
) -> AppResult<SubscriptionSummary> {
    subscriptions.set_interval(&id, interval_minutes).await
}

#[tauri::command]
pub async fn select_subscription_child(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    id: String,
    child_key: String,
) -> AppResult<SubscriptionSummary> {
    subscriptions.select_child(&id, &child_key).await
}

#[tauri::command]
pub async fn refresh_subscription(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    id: String,
) -> AppResult<RefreshSubscriptionResult> {
    subscriptions.refresh(&id).await
}

#[tauri::command]
pub async fn migrate_legacy_subscriptions(
    subscriptions: State<'_, Arc<SubscriptionService>>,
    inputs: Vec<LegacySubscriptionInput>,
) -> AppResult<SubscriptionSnapshot> {
    subscriptions.migrate_legacy(inputs).await
}

// The persisted HWID intentionally never crosses the command boundary. These
// commands preserve the frontend control surface without serializing its value.
#[tauri::command]
pub async fn get_subscription_hwid(
    subscriptions: State<'_, Arc<SubscriptionService>>,
) -> AppResult<bool> {
    subscriptions.get_hwid().await?;
    Ok(true)
}

#[tauri::command]
pub async fn reset_subscription_hwid(
    subscriptions: State<'_, Arc<SubscriptionService>>,
) -> AppResult<()> {
    subscriptions.reset_hwid().await?;
    Ok(())
}

/// Detect whether the subscription is plain multiline or base64-encoded
/// and return the individual lines.
fn split_subscription(text: &str) -> (Vec<String>, bool) {
    let text = text.trim();
    if text.is_empty() {
        return (Vec::new(), false);
    }
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    if lines.iter().any(|l| l.contains("://")) {
        return (lines, false);
    }
    // Base64 fallback.
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let try_decode = |input: &str| {
        use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
        use base64::Engine as _;
        let pad = |mut t: String| {
            while t.len() % 4 != 0 {
                t.push('=');
            }
            t
        };
        URL_SAFE_NO_PAD
            .decode(input)
            .or_else(|_| URL_SAFE_NO_PAD.decode(pad(input.to_string())))
            .or_else(|_| STANDARD.decode(input))
            .or_else(|_| STANDARD.decode(pad(input.to_string())))
            .ok()
    };
    if let Some(bytes) = try_decode(&cleaned) {
        let decoded = String::from_utf8_lossy(&bytes).into_owned();
        let lines: Vec<String> = decoded
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect();
        return (lines, true);
    }
    (lines, false)
}

// ── Process enumeration (routing process-name picker) ───────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

/// List currently running processes (name + pid).
///
/// Used by the routing-rule "process name" picker so the user can
/// click on a running app instead of typing its .exe name by hand.
/// Sorted by name (case-insensitive), de-duplicated by name, capped
/// at 500 entries — a typical Windows desktop has 150-300 processes
/// so the cap is a safety net for pathological hosts.
#[tauri::command]
pub async fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    use std::collections::BTreeMap;

    let mut sys = sysinfo::System::new();
    // sysinfo 0.32: refresh_processes takes a `ProcessesToUpdate` and
    // a "force_refresh" bool. `All` = every process, no filter.
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut by_name: BTreeMap<String, sysinfo::Pid> = BTreeMap::new();
    for (pid, proc_) in sys.processes().iter() {
        let name = proc_.name().to_string_lossy().trim().to_string();
        if name.is_empty() || name.len() > 64 {
            continue;
        }
        let key = name.to_ascii_lowercase();
        // If two processes share a name, keep the lowest pid (it's the
        // canonical one the user expects — usually the parent).
        by_name
            .entry(key)
            .and_modify(|existing| {
                if *pid < *existing {
                    *existing = *pid;
                }
            })
            .or_insert(*pid);
    }

    let mut out: Vec<ProcessInfo> = by_name
        .into_iter()
        .map(|(_, pid)| {
            let proc_ = sys.process(pid);
            let name = proc_
                .map(|p| p.name().to_string_lossy().to_string())
                .unwrap_or_default();
            ProcessInfo {
                pid: pid.as_u32(),
                name,
            }
        })
        .filter(|p| !p.name.is_empty())
        .collect();
    // Stable display order: by name (case-insensitive), ties broken by pid.
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then(a.pid.cmp(&b.pid))
    });
    out.truncate(500);
    Ok(out)
}

// ── app-shell and sing-box updates ───────────────────────────────────────

/// Fetch the signed app-shell update manifest. URLs and signatures deliberately
/// remain backend-only; the returned metadata is safe to render in the WebView.
#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> AppResult<crate::app_update::AppUpdateInfo> {
    crate::app_update::check_app_update(app).await
}

/// Refetch, validate, download, and verify an app update in Rust. The optional
/// version prevents a stale UI card from selecting a different release.
#[tauri::command]
pub async fn install_app_update(app: AppHandle, expected_version: Option<String>) -> AppResult<()> {
    crate::app_update::install_app_update(app, expected_version).await
}

// ── sing-box auto-update (see `updates` module) ─────────────────────────

/// Frontend-facing wrapper: returns the current + latest sing-box
/// versions and whether an update is available.
#[tauri::command]
pub async fn check_singbox_update(app: AppHandle) -> AppResult<crate::updates::SingboxUpdateInfo> {
    crate::updates::check_singbox_update(&app).await
}

/// Refetch and install a checksum-verified sing-box release selected entirely by
/// Rust. The optional version rejects a stale update card without accepting any
/// browser-controlled URL or asset metadata.
#[tauri::command]
pub async fn apply_singbox_update(
    app: AppHandle,
    expected_version: Option<String>,
) -> AppResult<String> {
    crate::updates::apply_singbox_update(app, expected_version).await
}

#[cfg(test)]
mod managed_launch_tests {
    use super::deduplicate_outbounds;
    use crate::parser::Outbound;

    #[test]
    fn deduplicates_manual_and_subscription_outbounds_in_first_seen_order() {
        let manual = Outbound::Unsupported {
            raw: "manual-first".into(),
            reason: "test".into(),
        };
        let subscription = Outbound::Unsupported {
            raw: "subscription-second".into(),
            reason: "test".into(),
        };
        let unique = deduplicate_outbounds(vec![
            manual.clone(),
            manual,
            subscription.clone(),
            subscription,
        ])
        .unwrap();

        assert_eq!(unique.len(), 2);
        assert!(matches!(unique[0], Outbound::Unsupported { ref raw, .. } if raw == "manual-first"));
        assert!(matches!(unique[1], Outbound::Unsupported { ref raw, .. } if raw == "subscription-second"));
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;

    /// Smoke test: the command runs without panicking and returns a
    /// well-formed list on a normal host. We don't assert specific
    /// process names (machine-dependent) — only the invariants.
    #[tokio::test]
    async fn list_processes_returns_well_formed_entries() {
        let procs = list_processes().await.expect("list_processes ok");
        let mut prev: Option<String> = None;
        for p in &procs {
            assert!(!p.name.is_empty(), "process name must not be empty");
            if let Some(p_name) = &prev {
                assert!(
                    p_name.to_ascii_lowercase() <= p.name.to_ascii_lowercase(),
                    "list_processes must be sorted by name (case-insensitive)"
                );
            }
            prev = Some(p.name.clone());
        }
        // Cap of 500 — sanity check.
        assert!(procs.len() <= 500, "list_processes must cap at 500 entries");
    }
}
