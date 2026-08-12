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

/// Which built-in rule-sets / behaviours to enable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RoutingOptions {
    /// Route RFC1918 / loopback / link-local addresses directly.
    pub bypass_lan: bool,
    /// Reject all IPv6 (sing-box default for many users).
    pub reject_ipv6: bool,
    /// Reject outbound UDP/443 (QUIC). Useful for some DPI setups.
    pub block_quic: bool,
    /// Reject ad-related domains via a `route.rule_set` reference.
    /// sing-box 1.12+ removed the in-binary `geosite:` matcher, so this
    /// requires an external rule-set file (see `RULE_SET_*` URLs).
    /// The rule is **not** emitted when this flag is set without a
    /// matching `route.rule_set` entry, otherwise `sing-box check`
    /// fails. Currently always disabled in code; see `build_route`.
    pub block_ads: bool,
    /// Send China geo sites / IPs direct. Same caveat as `block_ads`.
    pub bypass_cn: bool,
    /// Send RU geo IPs direct. Same caveat.
    pub bypass_ru: bool,
    /// What `route.final` points to. Usually the selector.
    pub final_outbound: String,
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

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self {
            tunnel_mode: TunnelMode::SystemProxy,
            routing: RoutingOptions {
                bypass_lan: true,
                reject_ipv6: true,
                block_quic: false,
                block_ads: false,
                bypass_cn: false,
                bypass_ru: false,
                final_outbound: "proxy".to_string(),
            },
            clash_api: ClashApiOptions::default(),
            tun_interface_name: None,
            mixed_port: Some(2080),
            local_dns: Some("223.5.5.5".to_string()),
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
                // Drop the current member from the running set
                // when we switch, so half-open connections
                // through the now-slower server get re-opened on
                // the new one instead of dragging out a slow
                // tail. Without this the switch is mostly
                // cosmetic for already-open sockets.
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
    let mut rules: Vec<Value> = Vec::new();
    let mut rule_sets: Vec<Value> = Vec::new();
    let r = &settings.routing;

    // Order matters — first matching rule wins. We want:
    //   0. DNS bypass — never route DNS queries through the proxy.
    //      Without this rule, a flaky DoH upstream would black-hole
    //      every connection in TUN mode.
    //   1. sniff protocol (so later rules can match by domain)
    //   2. drop IPv6 / QUIC
    //   3. direct for LAN
    //   4. rule_set-based: bypass CN/RU, block ads (all optional)
    //   5. everything else → route.final (the "proxy" selector)
    rules.push(json!({ "network": "dns", "action": "direct" }));
    rules.push(json!({ "action": "sniff" }));

    if r.reject_ipv6 {
        rules.push(json!({ "ip_version": 6, "action": "reject" }));
    }

    if r.block_quic {
        // Reject UDP/443 — QUIC. Prevents the GFW/DPI from upgrading
        // the connection before our TLS dial.
        rules.push(json!({
            "port_range": ["443:443"],
            "network": "udp",
            "action": "reject"
        }));
    }

    if r.bypass_lan {
        // RFC1918 + loopback + link-local
        rules.push(json!({
            "ip_cidr": [
                "10.0.0.0/8",
                "172.16.0.0/12",
                "192.168.0.0/16",
                "127.0.0.0/8",
                "169.254.0.0/16"
            ],
            "action": "direct"
        }));
    }

    // sing-box 1.12+ requires external rule-set files for `geosite:` /
    // `geoip:` matchers. We pull them from SagerNet's official `rule-set`
    // branches (sing-geosite for domains, sing-geoip for IPs), which are
    // the canonical sing-box 1.14+ sources — Loyalsoldier/v2ray-rules-dat
    // never shipped sing-box format and its 404s would crash the service.
    //
    // Each rule-set uses the route-level `default_http_client` (defined
    // in `build`) — `rule-set-fetcher` — which fetches via the system
    // network (chicken-and-egg trap: can't route rule-set download
    // through the proxy the rule-set is configuring). `download_detour`
    // is gone (deprecated in 1.14, will be removed in 1.16).
    if r.bypass_cn {
        rule_sets.push(json!({
            "tag": "rs-cn",
            "type": "remote",
            "format": "binary",
            "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
            "update_interval": "1d",
        }));
        rule_sets.push(json!({
            "tag": "rs-cn-ip",
            "type": "remote",
            "format": "binary",
            "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs",
            "update_interval": "1d",
        }));
        rules.push(json!({ "rule_set": ["rs-cn", "rs-cn-ip"], "action": "direct" }));
    }
    if r.bypass_ru {
        // SagerNet/sing-geosite does not publish a `geosite-ru.srs` —
        // v2fly/domain-list-community has no `ru` category. We only
        // bypass Russian IPs; Russian domains fall through to the
        // proxy selector (acceptable trade-off — most Russian traffic
        // is geo-targeted by IP anyway).
        rule_sets.push(json!({
            "tag": "rs-ru",
            "type": "remote",
            "format": "binary",
            "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-ru.srs",
            "update_interval": "1d",
        }));
        rules.push(json!({ "rule_set": "rs-ru", "action": "direct" }));
    }
    if r.block_ads {
        rule_sets.push(json!({
            "tag": "rs-ads",
            "type": "remote",
            "format": "binary",
            "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
            "update_interval": "1d",
        }));
        rules.push(json!({ "rule_set": "rs-ads", "action": "reject" }));
    }

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
    m.insert("auto_detect_interface".into(), Value::Bool(true));
    // sing-box 1.13: every outbound (or the route) must declare a
    // domain_resolver, otherwise resolving the proxy server hostname
    // warns. We default to the local plain-UDP server.
    m.insert(
        "default_domain_resolver".into(),
        Value::String("local".into()),
    );
    Value::Object(m)
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
        .unwrap_or_else(|| "223.5.5.5".to_string());
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
        let s = GeneratorSettings::default();
        let rules = s
            .routing
            .bypass_lan;
        assert!(rules, "LAN bypass should default to true");
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rules = cfg["route"]["rules"].as_array().unwrap();
        // expect at least: ipv6 reject + LAN cidr direct
        assert!(rules.iter().any(|r| r.get("ip_version").is_some()));
        assert!(rules.iter().any(|r| r.get("ip_cidr").is_some()));
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
        // Local default is 223.5.5.5 — type=udp, no detour.
        let local = &servers[0];
        assert_eq!(local["tag"], "local");
        assert_eq!(local["type"], "udp");
        assert_eq!(local["server"], "223.5.5.5");
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
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                block_ads: true,
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        // The matching rule references the rule-set by tag.
        assert!(rs.iter().any(|r| {
            r["rule_set"] == "rs-ads" && r["action"] == "reject"
        }));
        // And the rule-set entry is a remote `.srs` download from
        // SagerNet/sing-geosite (canonical sing-box 1.14+ source).
        assert_eq!(rss.len(), 1);
        assert_eq!(rss[0]["tag"], "rs-ads");
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
                bypass_cn: true,
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        assert!(rs.iter().any(|r| {
            r["rule_set"] == json!(["rs-cn", "rs-cn-ip"])
                && r["action"] == "direct"
        }));
        assert_eq!(rss.len(), 2);
        let tags: Vec<&str> = rss
            .iter()
            .map(|r| r["tag"].as_str().unwrap())
            .collect();
        assert!(tags.contains(&"rs-cn"));
        assert!(tags.contains(&"rs-cn-ip"));
        let cn = rss.iter().find(|r| r["tag"] == "rs-cn").unwrap();
        assert!(cn["url"].as_str().unwrap().ends_with("geosite-cn.srs"));
        let cn_ip = rss.iter().find(|r| r["tag"] == "rs-cn-ip").unwrap();
        assert!(cn_ip["url"].as_str().unwrap().ends_with("geoip-cn.srs"));
    }

    #[test]
    fn bypass_ru_emits_remote_ruleset_and_rule() {
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                bypass_ru: true,
                ..RoutingOptions::default()
            },
        );
        let cfg = Config::build(&fixture_outbounds(), &s);
        let rs = rules(&cfg);
        let rss = rule_sets(&cfg);
        assert!(rs.iter().any(|r| {
            r["rule_set"] == "rs-ru" && r["action"] == "direct"
        }));
        assert_eq!(rss.len(), 1);
        assert_eq!(rss[0]["tag"], "rs-ru");
        assert!(rss[0]["url"].as_str().unwrap().ends_with("geoip-ru.srs"));
    }

    #[test]
    fn block_quic_emits_udp_443_reject() {
        let s = routing(
            GeneratorSettings::default(),
            RoutingOptions {
                block_quic: true,
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
                block_ads: true,
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
