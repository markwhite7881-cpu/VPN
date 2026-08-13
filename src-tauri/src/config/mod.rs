//! sing-box config generator (Этап 3).
//!
//! Takes a list of parsed `Outbound`s and user settings, returns a
//! complete `serde_json::Value` ready to be written to a file and
//! passed to `sing-box run -c <path>`.
//!
//! The generator deliberately produces a minimal but real config:
//!   * log section with timestamps
//!   * DNS with two upstreams (DoH via proxy, plain DNS via direct)
//!   * inbounds: TUN (system-wide) AND mixed (socks/http on 127.0.0.1:2080)
//!   * outbounds: parsed profiles + direct + block + selector("proxy") +
//!     urltest("auto") that wraps them
//!   * route: IPv6 reject, optional LAN bypass, final = "proxy"
//!   * experimental.clash_api on 127.0.0.1:9090
//!
//! Reference: <https://sing-box.sagernet.org/configuration/>

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::parser::Outbound;

/// How the client should intercept system traffic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TunnelMode {
    /// System-wide TUN (requires Wintun + admin on Windows).
    Tun,
    /// Local SOCKS/HTTP proxy (no admin needed, system proxy must be
    /// set manually or by a helper).
    #[default]
    SystemProxy,
    /// Both — TUN for system traffic, mixed for local apps.
    Both,
    /// No inbound (only outbounds; useful for tests).
    None,
}

/// Routing 2.0 — flat rule list + rule-set list. The TS side is the
/// source of truth for the *shape*; the Rust side passes the JSON
/// structures through to the generated sing-box config. The `matchers`
/// and `action` fields are free-form `Value` because mirroring the
/// entire sing-box rule vocabulary in typed Rust would be a lot of
/// maintenance for very little gain — sing-box itself is the validator.
///
/// Note: replaces the v0.1.0 boolean flags. The frontend performs a
/// silent v1 → v2 migration in `loadSettings` so existing users keep
/// their routing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutingOptions {
    /// Ordered rule list (first match wins). Empty matchers + action
    /// `"route"` is the "default" rule.
    #[serde(default)]
    pub rules: Vec<Value>,
    /// External rule-sets (Loyalsoldier, meta-rules-dat, custom URL).
    #[serde(default)]
    pub rule_sets: Vec<Value>,
    /// Push `{ action: "sniff" }` at the top of the rules list.
    /// Mirrors the legacy `inbound.sniff` behaviour, now a route action.
    #[serde(default = "default_true")]
    pub sniff: bool,
    /// `route.final` outbound tag. Usually the "proxy" selector.
    pub final_outbound: String,
    /// `route.auto_detect_interface` — required for TUN to avoid
    /// routing loops on Win / Mac / Linux.
    #[serde(default = "default_true")]
    pub auto_detect_interface: bool,
    /// `route.default_domain_resolver` — the tag of the DNS server used
    /// to resolve outbound hostnames. Almost always "local".
    pub default_domain_resolver: String,
}

fn default_true() -> bool {
    true
}

/// Clash API settings (used by the UI for switching + traffic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClashApiOptions {
    pub external_controller: String,
    pub default_controller: String,
    pub secret: Option<String>,
}

impl Default for ClashApiOptions {
    fn default() -> Self {
        Self {
            external_controller: "127.0.0.1:9090".to_string(),
            default_controller: "proxy".to_string(),
            secret: None,
        }
    }
}

/// Full input the user can tweak from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorSettings {
    pub tunnel_mode: TunnelMode,
    pub routing: RoutingOptions,
    pub clash_api: ClashApiOptions,
    /// TUN interface name. Default works on most platforms.
    pub tun_interface_name: Option<String>,
    /// mixed inbound port (only used when SystemProxy / Both).
    pub mixed_port: Option<u16>,
    /// DNS server to use as the "local" upstream (resolved via direct).
    pub local_dns: Option<String>,
    /// DNS-over-HTTPS upstream used as the "remote" (resolved via proxy).
    pub remote_dns: Option<String>,
    /// Pin the `proxy` selector's `default` to this outbound tag.
    ///
    /// - `None` or `Some("auto")` → the `auto` urltest wins (best
    ///   latency, but takes a few seconds to converge and may
    ///   briefly route the first request through a different server).
    /// - `Some("🇳🇱 Нидерланды")` (or any other real tag) → the
    ///   first request after sing-box boots goes straight through
    ///   the picked server. No flash, no race with the urltest.
    ///
    /// The frontend regenerates the config and restarts sing-box
    /// whenever the user picks a different server, so this value
    /// changes with each click in the picker.
    pub default_outbound: Option<String>,
}

impl Default for RoutingOptions {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            rule_sets: Vec::new(),
            sniff: true,
            final_outbound: "proxy".to_string(),
            auto_detect_interface: true,
            default_domain_resolver: "local".to_string(),
        }
    }
}

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self {
            tunnel_mode: TunnelMode::SystemProxy,
            routing: RoutingOptions::default(),
            clash_api: ClashApiOptions::default(),
            tun_interface_name: None,
            mixed_port: Some(2080),
            local_dns: Some("77.88.8.8".to_string()),
            remote_dns: Some("https://dns.google/dns-query".to_string()),
            // `None` here means "let `auto` (urltest) decide". The
            // frontend switches to a real tag the moment the user
            // picks a server in the picker.
            default_outbound: None,
        }
    }
}

/// A built sing-box configuration. The struct is tiny — most of the
/// work is in `build()`, which returns a `serde_json::Value`.
pub struct Config;

impl Config {
    /// Build a complete sing-box config from the given outbounds + settings.
    pub fn build(outbounds: &[Outbound], settings: &GeneratorSettings) -> Value {
        let supported: Vec<&Outbound> = outbounds
            .iter()
            .filter(|o| !matches!(o, Outbound::Unsupported { .. }))
            .collect();

        // ---- inbounds ----
        let inbounds = build_inbounds(settings);

        // ---- outbounds ----
        let outbound_values: Vec<Value> = supported
            .iter()
            .map(|o| o.to_singbox_json())
            .collect();
        // We need the tag list to wire into the urltest + selector groups.
        let profile_tags: Vec<String> = supported
            .iter()
            .map(|o| match o {
                Outbound::Vless(v) => v.tag.clone(),
                Outbound::Vmess(v) => v.tag.clone(),
                Outbound::Trojan(v) => v.tag.clone(),
                Outbound::Shadowsocks(v) => v.tag.clone(),
                Outbound::Hysteria2(v) => v.tag.clone(),
                Outbound::Tuic(v) => v.tag.clone(),
                Outbound::Unsupported { .. } => unreachable!(),
            })
            .collect();

        // urltest("auto") wraps the profiles; selector("proxy") wraps
        // ["auto", "direct", ...profiles] so the user can pin a single
        // node OR fall back to "auto" (latency-based).
        let mut outbounds_arr: Vec<Value> = Vec::new();

        if !profile_tags.is_empty() {
            outbounds_arr.push(json!({
                "type": "urltest",
                "tag": "auto",
                "outbounds": profile_tags,
                "url": "https://www.gstatic.com/generate_204",
                // Re-measure every 30s and pick strictly the lowest
                // latency. `tolerance: 0` disables the "stick to
                // current unless much better" behaviour — the
                // selector will migrate to any faster server as
                // soon as it appears, which is what the user
                // expects from "Auto (best latency)".
                "interval": "30s",
                "tolerance": 0,
                // Drop the current member from the running set when
                // the urltest picks a faster server, so half-open
                // connections through the now-slower server get
                // re-opened on the new one instead of dragging out
                // a slow tail. Without this flag the switch is
                // mostly cosmetic for already-open sockets — the
                // old server keeps serving the stale streams.
                //
                // Note: this flag was temporarily set to `false`
                // while debugging an unrelated DNS issue (the TUN
                // interface's auto-derived DNS server landed inside
                // the same /30 as the TUN address, so Windows
                // treated it as an on-link neighbour and never
                // actually sent the packets — see the OS-level DNS
                // fix in `process.rs` that explicitly sets
                // `netsh interface ip set dns` for the TUN
                // interface). With the TUN-DNS bug fixed, this
                // flag is safe to keep on and we want it on: the
                // urltest is supposed to actually migrate traffic
                // when a faster server appears.
                "interrupt_exist_connections": true,
            }));
        }

        // Resolve which tag to use as the selector's default. We
        // only honour the user's pick if it actually exists in the
        // current profile set — a stale `default_outbound` from
        // a profile that has since been removed (e.g. subscription
        // refresh shrank the list) silently falls back to `auto`
        // rather than booting a config that crashes on startup.
        let default_tag: String = settings
            .default_outbound
            .as_deref()
            .filter(|t| !t.is_empty() && profile_tags.iter().any(|p| p == *t))
            .map(String::from)
            .unwrap_or_else(|| "auto".to_string());

        outbounds_arr.push(json!({
            "type": "selector",
            "tag": "proxy",
            "outbounds": if profile_tags.is_empty() {
                Value::Array(vec![Value::String("direct".to_string())])
            } else {
                let mut items: Vec<Value> = vec![Value::String("auto".to_string())];
                items.extend(profile_tags.iter().cloned().map(Value::String));
                Value::Array(items)
            },
            "default": default_tag,
        }));

        outbounds_arr.push(json!({ "type": "direct", "tag": "direct" }));
        outbounds_arr.push(json!({ "type": "block", "tag": "block" }));
        outbounds_arr.extend(outbound_values);

        // ---- route ----
        let route = build_route(settings);

        // ---- DNS ----
        let dns = build_dns(settings);

        // ---- clash api ----
        let clash_api = build_clash_api(&settings.clash_api);

        // ---- log ----
        let log = json!({
            "level": "info",
            "timestamp": true,
        });

        // ---- assemble ----
        let mut root = Map::new();
        root.insert("log".into(), log);
        root.insert("dns".into(), dns);
        root.insert("inbounds".into(), Value::Array(inbounds));
        root.insert("outbounds".into(), Value::Array(outbounds_arr));
        root.insert("route".into(), route);
        // Explicit HTTP client for remote rule-set downloads. Using
        // the implicit default is deprecated in sing-box 1.14 and
        // will be removed in 1.16. We leave `detour` unset so the
        // http_client uses the system default route for the underlying
        // TCP connection — pinning `detour: "direct"` would fail
        // because the bare `direct` outbound has no server defined
        // and sing-box rejects it as an "empty" detour.
        root.insert(
            "http_clients".into(),
            json!([{ "tag": "rule-set-fetcher" }]),
        );
        root.insert("experimental".into(), clash_api);
        Value::Object(root)
    }
}

// ---- builders ------------------------------------------------------

fn build_inbounds(settings: &GeneratorSettings) -> Vec<Value> {
    let mut arr: Vec<Value> = Vec::new();
    let mode = settings.tunnel_mode;
    let want_tun = matches!(mode, TunnelMode::Tun | TunnelMode::Both);
    let want_mixed = matches!(mode, TunnelMode::SystemProxy | TunnelMode::Both);

    if want_tun {
        let interface = settings
            .tun_interface_name
            .clone()
            .unwrap_or_else(|| "singbox-tun".to_string());
        arr.push(json!({
            "type": "tun",
            "tag": "tun-in",
            // sing-box 1.12+ removed the legacy `inet4_address` /
            // `inet6_address` pair; both must be passed via `address`.
            "address": [
                "172.19.0.1/30",
                "fdfe:dcba:9876::1/126"
            ],
            "auto_route": true,
            "strict_route": true,
            "stack": "system",
            "mtu": 9000,
            "endpoint_independent_nat": false,
            "udp_timeout": "5m",
            "interface_name": interface,
        }));
    }

    if want_mixed {
        let port = settings.mixed_port.unwrap_or(2080);
        // sing-box 1.13: sniffing is configured as a route action
        // (`action: "sniff"`), not as legacy inbound fields. We keep
        // the inbound minimal and add a sniff rule in `build_route`.
        arr.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": port,
        }));
    }

    if arr.is_empty() {
        // TunnelMode::None — we still emit a placeholder so the
        // config is valid for `sing-box check`. SOCKS-only on a
        // non-privileged port.
        arr.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": settings.mixed_port.unwrap_or(2080),
        }));
    }
    arr
}

fn build_route(settings: &GeneratorSettings) -> Value {
    // Routing 2.0: the user edits a list of rule objects (sing-box JSON
    // shape) and a list of rule-set objects. We pass them through
    // verbatim, with three small extras that we always prepend:
    //
    //   1. `{ network: "dns", action: "direct" }` — never route DNS
    //      through the proxy. Without this, a flaky DoH upstream would
    //      black-hole every connection in TUN mode.
    //   2. `{ action: "sniff" }` (if the user has it on) — required so
    //      later rules can match by domain.
    //   3. Disabled rules and rule-sets are skipped here (not at
    //      frontend edit time), so the Rust side is the authority.
    let r = &settings.routing;

    let mut rules: Vec<Value> = Vec::new();
    // 0. DNS bypass — always on, hard-coded.
    rules.push(json!({ "network": "dns", "action": "direct" }));
    // 1. Optional sniff action.
    if r.sniff {
        rules.push(json!({ "action": "sniff" }));
    }
    // 2. User-defined rules.
    for rule in &r.rules {
        if !is_rule_enabled(rule) {
            continue;
        }
        // Drop empty / no-op rules (would fail `sing-box check`).
        let cleaned = strip_empty_fields(rule);
        if has_meaningful_matchers(&cleaned) {
            rules.push(cleaned);
        }
    }

    let rule_sets: Vec<Value> = r
        .rule_sets
        .iter()
        .filter(|rs| is_rule_set_enabled(rs))
        .cloned()
        .collect();

    let mut m = Map::new();
    m.insert("rules".into(), Value::Array(rules));
    if !rule_sets.is_empty() {
        m.insert("rule_set".into(), Value::Array(rule_sets));
        // Tell route to use the explicit `http_clients` entry for
        // downloads. Without this sing-box falls back to the
        // (deprecated) implicit default HTTP client.
        m.insert(
            "default_http_client".into(),
            Value::String("rule-set-fetcher".into()),
        );
    }
    m.insert("final".into(), Value::String(r.final_outbound.clone()));
    m.insert(
        "auto_detect_interface".into(),
        Value::Bool(r.auto_detect_interface),
    );
    m.insert(
        "default_domain_resolver".into(),
        Value::String(r.default_domain_resolver.clone()),
    );
    Value::Object(m)
}

/// CustomRule on the wire is `{"enabled": true, "matchers": {...},
/// "action": "route|reject|...", "invert": false, ...}`. Treat the whole
/// thing as opaque JSON and only check `enabled`.
fn is_rule_enabled(rule: &Value) -> bool {
    rule.get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

fn is_rule_set_enabled(rs: &Value) -> bool {
    rs.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// Recursively strip `null` values and empty arrays/objects from a JSON
/// rule. Empty matchers would fail `sing-box check` (every rule needs
/// at least one matcher).
fn strip_empty_fields(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                if k == "enabled" {
                    // UI-only, never emitted to sing-box.
                    continue;
                }
                let cleaned = strip_empty_fields(val);
                match &cleaned {
                    Value::Null => continue,
                    Value::Array(a) if a.is_empty() => continue,
                    Value::Object(o) if o.is_empty() => continue,
                    _ => {
                        out.insert(k.clone(), cleaned);
                    }
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(strip_empty_fields).collect())
        }
        other => other.clone(),
    }
}

/// A rule is "meaningful" if it has at least one matcher field, or
/// if its action is one that doesn't need a matcher (rare in sing-box —
/// most actions require a matcher).
fn has_meaningful_matchers(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    // Any non-action / non-outbound / non-invert key counts as a matcher.
    for (k, _) in obj {
        if k == "action" || k == "outbound" || k == "invert" {
            continue;
        }
        return true;
    }
    false
}

fn build_dns(settings: &GeneratorSettings) -> Value {
    // sing-box 1.12+ requires typed DNS server entries.
    //
    // Important: we set `final: "local"` and the local server to a
    // plain UDP resolver. Going through DoH (`dns.google`) is
    // unreliable in many regions (blocked / throttled) and would
    // black-hole the entire internet the moment it fails — every
    // connection relies on a working DNS lookup.
    let local_input = settings
        .local_dns
        .clone()
        .unwrap_or_else(|| "77.88.8.8".to_string());
    let remote_input = settings
        .remote_dns
        .clone()
        .unwrap_or_else(|| "https://dns.google/dns-query".to_string());

    let (local_type, local_server) = classify_dns(&local_input);
    let (remote_type, remote_server) = classify_dns(&remote_input);

    let mut local_obj = Map::new();
    local_obj.insert("type".into(), Value::String(local_type));
    local_obj.insert("tag".into(), Value::String("local".into()));
    local_obj.insert("server".into(), Value::String(local_server));

    let mut remote_obj = Map::new();
    remote_obj.insert("type".into(), Value::String(remote_type.clone()));
    remote_obj.insert("tag".into(), Value::String("remote".into()));
    remote_obj.insert("server".into(), Value::String(remote_server));
    if remote_type == "https" {
        // Resolve the DoH hostname via our plain-UDP local resolver.
        remote_obj.insert(
            "domain_resolver".into(),
            Value::String("local".into()),
        );
    } else {
        // Resolve the DoT/DoQ hostname via direct.
        remote_obj.insert("detour".into(), Value::String("direct".into()));
    }

    json!({
        "servers": [Value::Object(local_obj), Value::Object(remote_obj)],
        // Use the local resolver as the catch-all. This guarantees
        // DNS works even if the DoH server is unreachable.
        "final": "local",
        "strategy": "prefer_ipv4"
    })
}

/// Classify a user-provided DNS string into a `(type, server)` pair
/// compatible with sing-box 1.12+ typed DNS servers.
fn classify_dns(s: &str) -> (String, String) {
    if let Some(rest) = s.strip_prefix("https://") {
        // Strip path; keep host:port.
        let host_port = rest.split('/').next().unwrap_or(rest);
        return ("https".to_string(), host_port.to_string());
    }
    if let Some(rest) = s.strip_prefix("tls://") {
        let host_port = rest.split('/').next().unwrap_or(rest);
        return ("tls".to_string(), host_port.to_string());
    }
    if let Some(rest) = s.strip_prefix("quic://") {
        let host_port = rest.split('/').next().unwrap_or(rest);
        return ("quic".to_string(), host_port.to_string());
    }
    // Default: treat as a plain IP (UDP).
    ("udp".to_string(), s.to_string())
}

fn build_clash_api(opts: &ClashApiOptions) -> Value {
    let mut m = Map::new();
    m.insert(
        "external_controller".into(),
        Value::String(opts.external_controller.clone()),
    );
    if let Some(secret) = &opts.secret {
        m.insert("secret".into(), Value::String(secret.clone()));
    }
    // sing-box 1.13: only `external_controller` + `default_mode` are
    // required; persistence is configured separately via
    // `experimental.cache_file` (added below).
    m.insert(
        "default_mode".into(),
        Value::String(opts.default_controller.clone()),
    );
    let mut root = Map::new();
    root.insert("clash_api".into(), Value::Object(m));
    // We deliberately do NOT enable `experimental.cache_file` here.
    //
    // sing-box's default for `cache_file.path` is the relative
    // string `"cache.db"`, which it resolves against the *current
    // working directory* of the process. When the app is launched
    // via `tauri dev` the cwd is `src-tauri/`, so sing-box writes
    // `cache.db` there on every startup, which makes Tauri dev's
    // file watcher trigger a full Rust recompile and restart the
    // app. Disabling the file cache sidesteps that entirely —
    // sing-box falls back to an in-memory cache, which is fine
    // for a desktop client (no fakeip in use, and the per-run
    // DNS cache lives entirely in RAM).
    Value::Object(root)
}

// ---- tests ---------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_link;

    fn fixture_outbounds() -> Vec<Outbound> {
        let raw = [
            "vless://b2a3d6c8-1111-2222-3333-444455556666@de.example.com:443?type=tcp&security=reality&pbk=PUB&sid=ABCD&sni=cdn.example.com&flow=xtls-rprx-vision#DE-1",
            "hy2://pw@nl.example.org:443?sni=nl.example.org&obfs=salamander&obfs-password=op#NL-1",
            "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpzZWNyZXQ@1.2.3.4:8388#SG-1",
        ];
        raw.iter()
            .map(|s| parse_link(s).expect("parse"))
            .collect()
    }

    #[test]
    fn produces_all_sections() {
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let root = cfg.as_object().expect("root object");
        for k in ["log", "dns", "inbounds", "outbounds", "route", "experimental"] {
            assert!(root.contains_key(k), "missing section: {k}");
        }
    }

    #[test]
    fn wraps_profiles_with_selector_and_urltest() {
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let outs = cfg["outbounds"].as_array().unwrap();
        // Expected order: urltest("auto"), selector("proxy"),
        // direct, block, then 3 profiles.
        assert_eq!(outs[0]["type"], "urltest");
        assert_eq!(outs[0]["tag"], "auto");
        assert_eq!(outs[1]["type"], "selector");
        assert_eq!(outs[1]["tag"], "proxy");
        assert_eq!(outs[2]["type"], "direct");
        assert_eq!(outs[3]["type"], "block");
        assert_eq!(outs[4]["type"], "vless");
        assert_eq!(outs[5]["type"], "hysteria2");
        assert_eq!(outs[6]["type"], "shadowsocks");
    }

    #[test]
    fn selector_includes_auto_and_all_profiles() {
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let sel = cfg["outbounds"][1].clone();
        // sing-box 1.13: outbounds is a flat `[]string` array (no
        // nested `items` / `if_empty` wrapper).
        let items = sel["outbounds"].as_array().expect("outbounds is array");
        let tags: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(tags[0], "auto");
        assert!(tags.contains(&"DE-1"));
        assert!(tags.contains(&"NL-1"));
        assert!(tags.contains(&"SG-1"));
    }

    #[test]
    fn tunnel_mode_both_emits_two_inbounds() {
        let s = GeneratorSettings {
            tunnel_mode: TunnelMode::Both,
            ..GeneratorSettings::default()
        };
        let cfg = Config::build(&fixture_outbounds(), &s);
        let inbounds = cfg["inbounds"].as_array().unwrap();
        assert_eq!(inbounds.len(), 2);
        assert_eq!(inbounds[0]["type"], "tun");
        assert_eq!(inbounds[1]["type"], "mixed");
    }

    #[test]
    fn tunnel_mode_none_falls_back_to_placeholder() {
        let s = GeneratorSettings {
            tunnel_mode: TunnelMode::None,
            ..GeneratorSettings::default()
        };
        let cfg = Config::build(&fixture_outbounds(), &s);
        let inbounds = cfg["inbounds"].as_array().unwrap();
        assert!(!inbounds.is_empty());
    }

    #[test]
    fn routing_includes_ipv6_reject_and_lan_bypass() {
        // Routing 2.0 default: hard-coded DNS-bypass + sniff + empty
        // user rules. Verify those two system rules are present.
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let rules = cfg["route"]["rules"].as_array().unwrap();
        // DNS bypass (hard-coded)
        assert!(rules
            .iter()
            .any(|r| r.get("network") == Some(&json!("dns"))));
        // Sniff action (since sniff=true by default)
        assert!(rules.iter().any(|r| r.get("action") == Some(&json!("sniff"))));
    }

    #[test]
    fn tun_dns_server_is_outside_tun_subnet() {
        // Regression test for the TUN-DNS bug: Windows auto-derives a
        // DNS server from the TUN's own /30 (e.g. 172.19.0.1/30 →
        // 172.19.0.2) and treats it as an on-link neighbour — ARP
        // never succeeds, every direct DNS call hangs. The fix is
        // OS-level (see `process::set_tun_dns_from_config`), but we
        // also want the *configured* DNS server to be far away from
        // the TUN's address range, so the sing-box-side `dns.servers`
        // block never accidentally agrees with the auto-derived
        // value.
        //
        // The actual mechanism: assert that the IPv4 address of
        // `dns.servers[0]` is NOT inside the IPv4 network of the
        // TUN's first `address` field (the one that the wintun
        // driver will land in), and likewise for IPv6 / /126.
        //
        // Use Tun mode explicitly — the GeneratorSettings default is
        // SystemProxy, which has no TUN inbound at all.
        let s = GeneratorSettings {
            tunnel_mode: TunnelMode::Tun,
            ..GeneratorSettings::default()
        };
        let cfg = Config::build(&fixture_outbounds(), &s);

        // 1) extract the TUN's IPv4 /30 and IPv6 /126.
        let tun_addr = cfg["inbounds"]
            .as_array()
            .and_then(|arr| arr.iter().find(|i| i.get("type") == Some(&json!("tun"))))
            .and_then(|i| i.get("address"))
            .and_then(|a| a.as_array())
            .expect("tun inbound has address[]");

        let v4_str = tun_addr[0].as_str().expect("tun ipv4 cidr");
        let v6_str = tun_addr[1].as_str().expect("tun ipv6 cidr");
        let (tun_v4, prefix_v4) = parse_cidr_v4(v4_str);
        let (tun_v6, prefix_v6) = parse_cidr_v6(v6_str);

        // 2) extract the local DNS server.
        let local_dns = cfg["dns"]["servers"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|s| s.get("server"))
            .and_then(|s| s.as_str())
            .expect("dns.servers[0].server is a string")
            .to_string();

        // 3) assert it is outside the TUN's /30. The default
        // `77.88.8.8` (Yandex DNS) is obviously outside
        // `172.19.0.0/30`, and this test will catch a regression
        // where someone accidentally reverts the default to
        // `223.5.5.5` (which is also outside, but a different
        // class of problem) or worse to something like
        // `172.19.0.2` (which would put the DNS server *inside*
        // the TUN's own /30 and re-create the original bug at
        // the sing-box level).
        let dns_v4 = parse_ipv4(&local_dns).unwrap_or_else(|| {
            panic!("local DNS {local_dns} should be an IPv4 literal")
        });
        assert!(
            !same_subnet_v4(tun_v4, prefix_v4, dns_v4, 32),
            "local DNS {local_dns} must NOT be in TUN IPv4 {v4_str} (auto-derivation bug)"
        );

        // 4) if the local DNS happens to be IPv6 (it isn't today,
        // but a future default could be), also assert it's not in
        // the TUN's /126.
        if let Some(dns_v6) = parse_ipv6(&local_dns) {
            assert!(
                !same_subnet_v6(tun_v6, prefix_v6, dns_v6, 128),
                "local DNS {local_dns} must NOT be in TUN IPv6 {v6_str}"
            );
        }
    }

    // ---- CIDR helpers used by `tun_dns_server_is_outside_tun_subnet` --

    /// Parse `a.b.c.d/n` into `(u32 address, u8 prefix)`. Host-order.
    fn parse_cidr_v4(s: &str) -> (u32, u8) {
        let (ip, prefix) = s.split_once('/').expect("cidr has /");
        let p: u8 = prefix.parse().expect("prefix parses");
        let addr = parse_ipv4(ip).expect("ipv4 parses");
        (addr, p)
    }

    /// Parse `a.b.c.d` into a host-order u32. Returns `None` on
    /// malformed input (e.g. an IPv6 literal).
    fn parse_ipv4(s: &str) -> Option<u32> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return None;
        }
        let mut out: u32 = 0;
        for p in parts {
            let n: u32 = p.parse().ok()?;
            if n > 255 {
                return None;
            }
            out = (out << 8) | n;
        }
        Some(out)
    }

    /// `true` if `addr` is in `net/prefix`. Both addresses are
    /// host-order u32.
    fn same_subnet_v4(net: u32, prefix: u8, addr: u32, _addr_prefix: u8) -> bool {
        if prefix == 0 {
            return true;
        }
        let mask = if prefix >= 32 { u32::MAX } else { u32::MAX << (32 - prefix) };
        (net & mask) == (addr & mask)
    }

    /// Parse `a:b:c:d:e:f:g:h/n` into `(u128 address, u8 prefix)`.
    fn parse_cidr_v6(s: &str) -> (u128, u8) {
        let (ip, prefix) = s.split_once('/').expect("cidr has /");
        let p: u8 = prefix.parse().expect("prefix parses");
        let addr = parse_ipv6(ip).expect("ipv6 parses");
        (addr, p)
    }

    /// Parse an IPv6 literal into a u128 (host-order). No zone IDs,
    /// no embedded IPv4 — the TUN's `fdfe:dcba:9876::1/126` literal
    /// doesn't need either, and Yandex's `77.88.8.8` doesn't even
    /// reach this function.
    fn parse_ipv6(s: &str) -> Option<u128> {
        if s.contains(':') == false {
            return None;
        }
        // split on `::` once: head groups + tail groups
        let (head, tail) = match s.split_once("::") {
            Some((h, t)) => (h, Some(t)),
            None => (s, None),
        };
        let head_groups: Vec<u16> = if head.is_empty() {
            Vec::new()
        } else {
            head.split(':').map(|g| u16::from_str_radix(g, 16).ok()).collect::<Option<_>>()?
        };
        let tail_groups: Vec<u16> = match tail {
            Some(t) if !t.is_empty() => {
                t.split(':').map(|g| u16::from_str_radix(g, 16).ok()).collect::<Option<_>>()?
            }
            _ => Vec::new(),
        };
        let total = head_groups.len() + tail_groups.len();
        if total > 8 {
            return None;
        }
        let mut out: u128 = 0;
        for g in head_groups {
            out = (out << 16) | g as u128;
        }
        let fill = 8 - total;
        out <<= fill * 16;
        for g in tail_groups {
            out = (out << 16) | g as u128;
        }
        Some(out)
    }

    /// `true` if `addr` is in `net/prefix`. Both addresses are
    /// host-order u128.
    fn same_subnet_v6(net: u128, prefix: u8, addr: u128, _addr_prefix: u8) -> bool {
        if prefix == 0 {
            return true;
        }
        let shift = 128 - prefix as u32;
        let mask = if shift >= 128 { 0u128 } else { u128::MAX << shift };
        (net & mask) == (addr & mask)
    }

    #[test]
    fn empty_profiles_yields_valid_skeleton() {
        let cfg = Config::build(&[], &GeneratorSettings::default());
        let outs = cfg["outbounds"].as_array().unwrap();
        // Without profiles, we skip the urltest wrapper.
        assert!(outs.iter().any(|o| o["tag"] == "proxy"));
        assert!(outs.iter().any(|o| o["tag"] == "direct"));
        assert!(outs.iter().any(|o| o["tag"] == "block"));
    }

    #[test]
    fn dns_uses_typed_servers() {
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let servers = cfg["dns"]["servers"].as_array().unwrap();
        assert_eq!(servers.len(), 2);
        // Local default is 77.88.8.8 (Yandex DNS) — type=udp, no detour.
        let local = &servers[0];
        assert_eq!(local["tag"], "local");
        assert_eq!(local["type"], "udp");
        assert_eq!(local["server"], "77.88.8.8");
        // Remote default is https://dns.google/dns-query — type=https, host stripped.
        let remote = &servers[1];
        assert_eq!(remote["tag"], "remote");
        assert_eq!(remote["type"], "https");
        assert_eq!(remote["server"], "dns.google");
        // DoH should reference local domain_resolver.
        assert_eq!(remote["domain_resolver"], "local");
    }

    #[test]
    fn classify_dns_handles_schemes() {
        assert_eq!(
            classify_dns("https://dns.google/dns-query"),
            ("https".to_string(), "dns.google".to_string())
        );
        assert_eq!(
            classify_dns("tls://1.1.1.1"),
            ("tls".to_string(), "1.1.1.1".to_string())
        );
        assert_eq!(
            classify_dns("1.1.1.1"),
            ("udp".to_string(), "1.1.1.1".to_string())
        );
    }

    // --- Routing presets (Этап 7) ---------------------------------

    fn rules(cfg: &Value) -> &Vec<Value> {
        cfg["route"]["rules"].as_array().expect("rules array")
    }

    fn rule_sets(cfg: &Value) -> Vec<&Value> {
        cfg["route"]
            .get("rule_set")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default()
    }

    fn routing(mut s: GeneratorSettings, r: RoutingOptions) -> GeneratorSettings {
        s.routing = r;
        s
    }

    #[test]
    fn ads_block_emits_remote_ruleset_and_rule() {
        // User enables `geosite-ads` rule-set + a rule that rejects it.
        // The generator should emit both the rule and the rule-set
        // entry (SagerNet/sing-geosite is the canonical source for
        // sing-box 1.14+ .srs rule-sets).
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rules: vec![json!({
                    "rule_set": "geosite-ads",
                    "action": "reject"
                })],
                rule_sets: vec![json!({
                    "tag": "geosite-ads",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
                    "update_interval": "1d"
                })],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        // The matching rule references the rule-set by tag.
        assert!(rs.iter().any(|r| {
            r["rule_set"] == "geosite-ads" && r["action"] == "reject"
        }));
        // And the rule-set entry is a remote `.srs` download.
        assert_eq!(rss.len(), 1);
        assert_eq!(rss[0]["tag"], "geosite-ads");
        assert_eq!(rss[0]["type"], "remote");
        assert_eq!(rss[0]["format"], "binary");
        assert!(rss[0]["url"]
            .as_str()
            .unwrap()
            .ends_with("geosite-category-ads-all.srs"));
        // `download_detour` was removed in sing-box 1.14 — the implicit
        // default HTTP client (first outbound = `direct`) handles the
        // download. Asserting absence keeps us honest if anyone tries
        // to re-introduce the deprecated field.
        assert!(rss[0].get("download_detour").is_none());
    }

    #[test]
    fn bypass_cn_emits_two_rulesets_domains_and_ips() {
        // Bypass CN should match BOTH Chinese domains (geosite-cn) and
        // Chinese IPs (geoip-cn) — the rule references both tags.
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rules: vec![json!({
                    "rule_set": ["geosite-cn", "geoip-cn"],
                    "action": "route",
                    "outbound": "direct"
                })],
                rule_sets: vec![
                    json!({
                        "tag": "geosite-cn",
                        "type": "remote",
                        "format": "binary",
                        "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
                        "update_interval": "1d"
                    }),
                    json!({
                        "tag": "geoip-cn",
                        "type": "remote",
                        "format": "binary",
                        "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs",
                        "update_interval": "1d"
                    }),
                ],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        assert!(rs.iter().any(|r| {
            r["rule_set"] == json!(["geosite-cn", "geoip-cn"])
                && r["action"] == "route"
                && r["outbound"] == "direct"
        }));
        assert_eq!(rss.len(), 2);
        let tags: Vec<&str> = rss
            .iter()
            .map(|r| r["tag"].as_str().unwrap())
            .collect();
        assert!(tags.contains(&"geosite-cn"));
        assert!(tags.contains(&"geoip-cn"));
        let cn = rss.iter().find(|r| r["tag"] == "geosite-cn").unwrap();
        assert!(cn["url"].as_str().unwrap().ends_with("geosite-cn.srs"));
        let cn_ip = rss.iter().find(|r| r["tag"] == "geoip-cn").unwrap();
        assert!(cn_ip["url"].as_str().unwrap().ends_with("geoip-cn.srs"));
    }

    #[test]
    fn bypass_ru_emits_remote_ruleset_and_rule() {
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rules: vec![json!({
                    "rule_set": "geoip-ru",
                    "action": "route",
                    "outbound": "direct"
                })],
                rule_sets: vec![json!({
                    "tag": "geoip-ru",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-ru.srs",
                    "update_interval": "1d"
                })],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        assert!(rs.iter().any(|r| {
            r["rule_set"] == "geoip-ru" && r["action"] == "route"
        }));
        assert_eq!(rss.len(), 1);
        assert_eq!(rss[0]["tag"], "geoip-ru");
        assert!(rss[0]["url"].as_str().unwrap().ends_with("geoip-ru.srs"));
    }

    #[test]
    fn block_quic_emits_udp_443_reject() {
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rules: vec![json!({
                    "port_range": ["443:443"],
                    "network": "udp",
                    "action": "reject"
                })],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        assert!(rs.iter().any(|r| {
            r["port_range"] == json!(["443:443"])
                && r["network"] == "udp"
                && r["action"] == "reject"
        }));
    }

    #[test]
    fn disabled_rules_are_skipped() {
        // `enabled: false` on a rule means the generator must not
        // emit it. Same for disabled rule-sets.
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rules: vec![
                    json!({
                        "enabled": false,
                        "ip_version": 6,
                        "action": "reject"
                    }),
                    json!({
                        "ip_version": 6,
                        "action": "reject"
                    }),
                ],
                rule_sets: vec![
                    json!({
                        "tag": "geosite-ads",
                        "type": "remote",
                        "format": "binary",
                        "url": "https://example/ads.srs",
                        "enabled": false
                    }),
                ],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        // Only the enabled IPv6 reject survives.
        assert_eq!(rs.iter().filter(|r| r.get("ip_version").is_some()).count(), 1);
        // Rule-set is disabled → not emitted.
        assert!(rss.is_empty(), "disabled rule-sets should not be emitted");
    }

    #[test]
    fn all_optional_toggles_disabled_yields_no_ruleset() {
        // With all rule-set toggles off, route.rule_set should be
        // absent entirely.
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let rss = rule_sets(&cfg);
        assert!(rss.is_empty(), "no rule_set expected, got {rss:#?}");
    }

    #[test]
    fn http_clients_always_emitted_for_future_proofing() {
        // sing-box 1.14 deprecates the implicit default HTTP client
        // (gone in 1.16). We always emit our own `rule-set-fetcher`
        // entry so the warning never appears regardless of whether
        // the user enabled any rule-set toggle.
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        let clients = cfg["http_clients"].as_array().expect("http_clients array");
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["tag"], "rule-set-fetcher");
    }

    #[test]
    fn default_http_client_set_when_rulesets_present() {
        // When ANY rule-set is enabled, route.default_http_client
        // must point at our fetcher so the deprecated implicit
        // default isn't used.
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                rule_sets: vec![json!({
                    "tag": "geosite-ads",
                    "type": "remote",
                    "format": "binary",
                    "url": "https://example/ads.srs"
                })],
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        assert_eq!(
            cfg["route"]["default_http_client"], "rule-set-fetcher"
        );
    }

    #[test]
    fn default_http_client_absent_when_no_rulesets() {
        // No rule-set toggles on → no `default_http_client` in route
        // (sing-box would error on an unknown http_client ref).
        let cfg = Config::build(&fixture_outbounds(), &GeneratorSettings::default());
        assert!(
            cfg["route"].get("default_http_client").is_none(),
            "default_http_client should be absent when no rule-sets"
        );
    }
}
