//! Этап 8 dry-run: берём реальные ссылки, парсим, генерируем
//! sing-box config, прогоняем `sing-box check` на sidecar'е.
//!
//! Запуск:
//!   cargo run --example verify_user_link
//!
//! Что тестируем:
//!   1. Парсер VLESS+Reality+gRPC ссылки (с spx/spider-x).
//!   2. Fetch subscription URL через reqwest (best-effort, нужен
//!      доступ в интернет; если fetch падает — печатаем ошибку, но
//!      не валим весь example).
//!   3. Config generator с подключёнными Этап 7 routing presets
//!      (Block ads + Bypass RU + Block QUIC) — чтобы увидеть, что
//!      новые правила не ломают существующий конфиг.
//!   4. `sing-box check` на sidecar'е — финальная проверка, что
//!      сгенерированный JSON структурно валиден.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use singbox_client_lib::config::{
    Config, GeneratorSettings, RoutingOptions, TunnelMode,
};
use singbox_client_lib::parser::parse_link;

const USER_LINK: &str = "vless://be0589e6-eac2-48cd-94f4-e41ceb8aa3c8@138.124.33.206:443\
?authority=&encryption=none&fp=firefox\
&pbk=GH4VZBxIED3RD2GmW68vdPFj9OaTNnoRRa8X7iWXt3M\
&security=reality&serviceName=&sid=b3b2138fa73c0827\
&sni=www.nvidia.com&spx=%2FNzDAT3TGEOWKLm0&type=grpc\
#rfgrfgr32412-qt9htqyal6ys";

const SUBSCRIPTION_URL: &str = "https://anivka.top/subka/rxin5olbgwh4apryr9fs";

fn main() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let singbox = here
        .join("binaries")
        .join("sing-box-x86_64-pc-windows-msvc.exe");

    // ---- 1. Parse the direct VLESS+Reality+gRPC link -----------------
    println!("\n=== 1. Parsing direct VLESS link ===\n");
    println!("link: {USER_LINK}\n");
    let vless = match parse_link(USER_LINK) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("!! parse failed: {e}");
            return;
        }
    };
    println!(
        "  protocol     = {}\n  tag          = {}\n  server:port  = {}:{}\n",
        vless.protocol(),
        vless.display_name(),
        vless.server().unwrap_or("?"),
        vless.port().unwrap_or(0),
    );
    // Dump the full outbound as JSON so we can eyeball the structured
    // fields (transport, Reality triple, fingerprint, …).
    println!(
        "  structured   = {}\n",
        serde_json::to_string_pretty(&vless).unwrap_or_default()
    );

    // ---- 2. Fetch subscription URL (best-effort) ---------------------
    println!("\n=== 2. Fetching subscription URL ===\n");
    println!("url: {SUBSCRIPTION_URL}\n");
    let sub_outbounds = match fetch_subscription_blocking(SUBSCRIPTION_URL) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("!! subscription fetch failed: {e}\n(continuing with just the direct link)");
            Vec::new()
        }
    };
    println!(
        "  got {} outbounds from subscription\n",
        sub_outbounds.len()
    );
    for (i, o) in sub_outbounds.iter().take(5).enumerate() {
        println!(
            "  [{}] {}  {}  {}:{}",
            i,
            o.protocol(),
            o.display_name(),
            o.server().unwrap_or("?"),
            o.port().unwrap_or(0),
        );
    }
    if sub_outbounds.len() > 5 {
        println!("  … {} more", sub_outbounds.len() - 5);
    }

    // ---- 3. Build a config with the direct link + (maybe) subs ------
    println!("\n=== 3. Building sing-box config ===\n");
    let mut outbounds = vec![vless];
    outbounds.extend(sub_outbounds);

    // Этап 7 routing — Routing 2.0: разные правила (bypass LAN, reject
    // IPv6, block QUIC) + 2 rule-set-based правила (bypass RU, block
    // ads). Чек-тест: генератор должен пройти `sing-box check`.
    use serde_json::json;
    let settings = GeneratorSettings {
        tunnel_mode: TunnelMode::SystemProxy,
        routing: RoutingOptions {
            rules: vec![
                json!({
                    "ip_cidr": [
                        "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16",
                        "127.0.0.0/8", "169.254.0.0/16"
                    ],
                    "action": "route", "outbound": "direct"
                }),
                json!({ "ip_version": 6, "action": "reject" }),
                json!({ "port_range": ["443:443"], "network": "udp", "action": "reject" }),
                json!({ "rule_set": "geoip-ru", "action": "route", "outbound": "direct" }),
                json!({ "rule_set": "geosite-ads", "action": "reject" }),
            ],
            rule_sets: vec![
                json!({
                    "tag": "geoip-ru",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-ru.srs",
                    "update_interval": "1d"
                }),
                json!({
                    "tag": "geosite-ads",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
                    "update_interval": "1d"
                }),
            ],
            sniff: true,
            final_outbound: "proxy".to_string(),
            auto_detect_interface: true,
            default_domain_resolver: "local".to_string(),
        },
        ..GeneratorSettings::default()
    };
    let value = Config::build(&outbounds, &settings);
    let out_path = here.join("examples").join("verify_user_link_output.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).expect("create examples dir");
    }
    let body = serde_json::to_string_pretty(&value).expect("serialise");
    fs::write(&out_path, &body).expect("write output");
    println!(
        "  wrote {} ({} bytes, {} outbounds)\n",
        out_path.display(),
        body.len(),
        outbounds.len(),
    );

    // ---- 4. sing-box check -------------------------------------------
    if !singbox.exists() {
        eprintln!("!! sing-box sidecar not found at {}", singbox.display());
        return;
    }
    println!("=== 4. Running `sing-box check` ===\n");
    let output = Command::new(&singbox)
        .arg("check")
        .arg("-c")
        .arg(&out_path)
        .output();
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            print!("{stdout}{stderr}");
            if o.status.success() {
                println!("\n✅ sing-box check passed");
            } else {
                println!(
                    "\n❌ sing-box check failed (exit {:?})",
                    o.status.code()
                );
            }
        }
        Err(e) => eprintln!("!! could not run sing-box: {e}"),
    }
}

/// Fetch the subscription URL with a synchronous reqwest call.
///
/// We don't have tokio here (this is an example, not the Tauri bin),
/// so we use reqwest's blocking client. Returns the parsed outbounds
/// (with per-line failure tolerance) or an error string describing
/// the transport-level failure.
fn fetch_subscription_blocking(url: &str) -> Result<Vec<singbox_client_lib::parser::Outbound>, String> {
    let body = reqwest::blocking::Client::builder()
        .user_agent("singbox-client/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("client build: {e}"))?
        .get(url)
        .send()
        .map_err(|e| format!("GET: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP: {e}"))?
        .text()
        .map_err(|e| format!("read body: {e}"))?;
    let trimmed = body.trim();
    println!(
        "  body: {} bytes, first 120 chars = {:?}",
        trimmed.len(),
        trimmed.chars().take(120).collect::<String>(),
    );

    // Per-line parser — the strict `parse_links` aborts on the first
    // unknown scheme (e.g. `incy://`), which is wrong for real
    // subscriptions. Replicates the helper in `commands.rs`.
    let lines = split_subscription(&body);
    println!("  split into {} non-empty lines", lines.len());
    let mut out = Vec::new();
    for (i, line) in lines.into_iter().enumerate() {
        match singbox_client_lib::parser::parse_link(&line) {
            Ok(o) => {
                println!(
                    "  [{}] ok   {} {}  {}:{}",
                    i,
                    o.protocol(),
                    o.display_name(),
                    o.server().unwrap_or("?"),
                    o.port().unwrap_or(0),
                );
                out.push(o);
            }
            Err(e) => {
                println!(
                    "  [{}] err  {}  ({})",
                    i,
                    line.chars().take(60).collect::<String>(),
                    e,
                );
            }
        }
    }
    Ok(out)
}

/// Per-line splitter (mirrors `commands::split_subscription_text`).
/// Tries plain newlines first, falls back to base64 decoding.
fn split_subscription(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    if lines.iter().any(|l| l.contains("://")) {
        return lines;
    }
    // base64 fallback
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let pad = |mut t: String| {
        while t.len() % 4 != 0 {
            t.push('=');
        }
        t
    };
    for engine in [URL_SAFE_NO_PAD, STANDARD] {
        for input in [cleaned.clone(), pad(cleaned.clone())] {
            if let Ok(bytes) = engine.decode::<String>(input) {
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    return s
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(String::from)
                        .collect();
                }
            }
        }
    }
    lines
}
