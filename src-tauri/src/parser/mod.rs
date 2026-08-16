//! Protocol link parsers (Этап 2).
//!
//! Each protocol (`vless://`, `vmess://`, `trojan://`, `ss://`,
//! `hy2://`, `tuic://`) is parsed into a strongly-typed `Outbound`,
//! which is later serialised to a sing-box `outbounds[]` entry by
//! [`Outbound::to_singbox_json`].
//!
//! Why a custom parser instead of the `vpn-link-serde` crate?
//! 1. The crate is sparsely maintained and lags behind protocol evolution
//!    (e.g. no clean support for VLESS Reality `pbk`/`sid`/`spx` triplets).
//! 2. We need extra metadata (display name, origin format, warnings) for
//!    the UI, which serde-crate-only types don't expose cleanly.
//! 3. We need a single error type that round-trips through Tauri.

mod error;
mod hy2;
mod ss;
mod to_json;
mod trojan;
mod tuic;
mod vless;
mod vmess;

use std::str::FromStr;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};

pub use error::ParseError;

/// Recognised transport layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    Tcp,
    Ws {
        path: Option<String>,
        headers: Vec<(String, String)>,
    },
    Http {
        host: Vec<String>,
        path: Option<String>,
    },
    /// sing-box 1.11+ HTTP/2-based "xhttp" transport. Wire-compatible
    /// with `http` for the basic fields, but sing-box distinguishes
    /// them server-side, so we keep it as its own variant.
    Xhttp {
        host: Vec<String>,
        path: Option<String>,
        /// Optional `mode` (e.g. `"auto"`, `"packet-up"`, `"stream-up"`).
        mode: Option<String>,
    },
    Grpc {
        service_name: Option<String>,
        #[serde(default)]
        idle_timeout: Option<String>,
        #[serde(default)]
        ping_timeout: Option<String>,
    },
    /// For Hysteria2 / TUIC — a UDP-based "transport-less" path.
    Udp,
}

impl Default for Transport {
    fn default() -> Self {
        Self::Tcp
    }
}

/// TLS settings. `reality` and `utls` are mutually compatible extras.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TlsCfg {
    pub enabled: bool,
    pub server_name: Option<String>,
    pub alpn: Vec<String>,
    pub fingerprint: Option<String>,
    /// Reality triple.
    pub reality: Option<RealityCfg>,
    /// Self-signed / allow-insecure.
    pub allow_insecure: bool,
    /// ECH (Encrypted Client Hello) — only used by some clients.
    pub ech: Option<EchCfg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RealityCfg {
    pub public_key: String,
    pub short_id: String,
    /// Optional spider X (path) used by some panels.
    pub spider_x: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EchCfg {
    pub config: String,
}

/// Outbound types we currently emit. Mirrors sing-box's `type` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum Outbound {
    Vless(VlessOut),
    Vmess(VmessOut),
    Trojan(TrojanOut),
    Shadowsocks(SsOut),
    Hysteria2(Hy2Out),
    Tuic(TuicOut),
    /// Sentinel: when the link is unsupported we surface it instead of
    /// dropping silently. The UI can then offer to skip / report.
    Unsupported {
        raw: String,
        reason: String,
    },
}

impl Outbound {
    /// Short human label, e.g. "vless" / "vmess" / "ss".
    pub fn protocol(&self) -> &'static str {
        match self {
            Outbound::Vless(_) => "vless",
            Outbound::Vmess(_) => "vmess",
            Outbound::Trojan(_) => "trojan",
            Outbound::Shadowsocks(_) => "shadowsocks",
            Outbound::Hysteria2(_) => "hysteria2",
            Outbound::Tuic(_) => "tuic",
            Outbound::Unsupported { .. } => "unsupported",
        }
    }

    /// Display name (server + tag fallback).
    pub fn display_name(&self) -> String {
        match self {
            Outbound::Vless(o) => o.label(),
            Outbound::Vmess(o) => o.label(),
            Outbound::Trojan(o) => o.label(),
            Outbound::Shadowsocks(o) => o.label(),
            Outbound::Hysteria2(o) => o.label(),
            Outbound::Tuic(o) => o.label(),
            Outbound::Unsupported { .. } => "unsupported link".to_string(),
        }
    }

    /// Server host (for quick display in lists). `None` for unsupported.
    pub fn server(&self) -> Option<&str> {
        match self {
            Outbound::Vless(o) => Some(&o.server),
            Outbound::Vmess(o) => Some(&o.server),
            Outbound::Trojan(o) => Some(&o.server),
            Outbound::Shadowsocks(o) => Some(&o.server),
            Outbound::Hysteria2(o) => Some(&o.server),
            Outbound::Tuic(o) => Some(&o.server),
            Outbound::Unsupported { .. } => None,
        }
    }

    /// Server port.
    pub fn port(&self) -> Option<u16> {
        match self {
            Outbound::Vless(o) => Some(o.port),
            Outbound::Vmess(o) => Some(o.port),
            Outbound::Trojan(o) => Some(o.port),
            Outbound::Shadowsocks(o) => Some(o.port),
            Outbound::Hysteria2(o) => Some(o.port),
            Outbound::Tuic(o) => Some(o.port),
            Outbound::Unsupported { .. } => None,
        }
    }
}

// --- per-protocol structs ---------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VlessOut {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    /// `flow` (only `xtls-rprx-vision` is currently meaningful in sing-box).
    pub flow: Option<String>,
    pub transport: Transport,
    pub tls: TlsCfg,
}

impl VlessOut {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VmessOut {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    /// Legacy VMess "alter ID" for anti-replay; modern configs use 0.
    pub alter_id: u16,
    pub cipher: VmessCipher,
    pub transport: Transport,
    pub tls: TlsCfg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VmessCipher {
    #[default]
    Auto,
    Aes128Gcm,
    Chacha20Poly1305,
    None,
}

impl VmessOut {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrojanOut {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub password: String,
    pub transport: Transport,
    pub tls: TlsCfg,
}

impl TrojanOut {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SsOut {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub method: String,
    pub password: String,
    /// SIP002 plugin (e.g. "obfs-local", "v2ray-plugin").
    pub plugin: Option<String>,
    pub plugin_opts: Option<String>,
}

impl SsOut {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hy2Out {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub password: String,
    pub tls: TlsCfg,
    pub obfs: Option<Hy2Obfs>,
    /// Optional bandwidth up/down (`50mbps` style).
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hy2Obfs {
    pub r#type: String, // "salamander" only in practice
    pub password: String,
}

impl Hy2Out {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuicOut {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    pub password: String,
    pub congestion_control: TuicCc,
    pub udp_relay_mode: TuicUdp,
    pub tls: TlsCfg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TuicCc {
    #[default]
    Cubic,
    NewReno,
    Bbr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TuicUdp {
    #[default]
    Native,
    Quic,
}

impl TuicOut {
    pub fn label(&self) -> String {
        if !self.tag.is_empty() {
            return self.tag.clone();
        }
        format!("{}:{}", self.server, self.port)
    }
}

// --- public dispatch --------------------------------------------------

/// Parse a single share-link. Accepts the URL-form for all protocols;
/// for VMess the legacy `vmess://...` (base64 of a JSON object) is
/// also recognised.
pub fn parse_link(raw: &str) -> Result<Outbound, ParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    // Some subscription providers prefix with extra junk (e.g. "ss://...".
    // with a leading whitespace or trailing null byte). Normalise.
    let cleaned = trimmed.trim_end_matches('\0').trim();

    let scheme = scheme_of(cleaned)
        .ok_or_else(|| ParseError::UnknownScheme(cleaned.chars().take(40).collect()))?;
    match scheme {
        "vless" => vless::parse(cleaned),
        "vmess" => vmess::parse(cleaned),
        "trojan" => trojan::parse(cleaned),
        "ss" | "shadowsocks" => ss::parse(cleaned),
        "hy2" | "hysteria2" | "hysteria" => hy2::parse(cleaned),
        "tuic" => tuic::parse(cleaned),
        other => Err(ParseError::UnsupportedScheme(other.to_string())),
    }
}

/// Parse a multiline blob (subscription or pasted list).
///
/// Accepts:
/// * one URL per line (`\n` separator, also handles `\r\n`),
/// * or a single base64 blob containing the same newline format,
/// * or a Clash YAML (recognised by leading `port:` after YAML header —
///   delegated to Этап 3's config generator).
pub fn parse_links(text: &str) -> Result<Vec<Outbound>, ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    // Try direct multiline first.
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if lines.iter().any(|l| l.contains("://")) {
        return lines.into_iter().map(parse_link).collect();
    }
    // Otherwise assume base64. Pick STANDARD vs URL_SAFE based on charset.
    let decoded = decode_base64_loose(text)
        .map_err(|_| ParseError::Base64(text.chars().take(40).collect()))?;
    let decoded = String::from_utf8_lossy(&decoded).into_owned();
    let lines: Vec<&str> = decoded
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    lines.into_iter().map(parse_link).collect()
}

/// Extract the URL scheme (`vless`, `ss`, ...) from a string like
/// `vless://...`. Returns `None` for anything that doesn't look like a URL.
fn scheme_of(s: &str) -> Option<&str> {
    let end = s.find("://")?;
    let scheme = &s[..end];
    if scheme.is_empty() || scheme.len() > 16 {
        return None;
    }
    if !scheme.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(scheme)
}

/// Best-effort base64 decode. Tries URL_SAFE first, then STANDARD,
/// both with and without padding.
fn decode_base64_loose(s: &str) -> Result<Vec<u8>, ()> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    // Try with padding restored first.
    let pad = |mut t: String| {
        while t.len() % 4 != 0 {
            t.push('=');
        }
        t
    };
    if let Ok(b) = URL_SAFE_NO_PAD.decode(&cleaned) {
        return Ok(b);
    }
    if let Ok(b) = URL_SAFE_NO_PAD.decode(pad(cleaned.clone())) {
        return Ok(b);
    }
    if let Ok(b) = STANDARD.decode(&cleaned) {
        return Ok(b);
    }
    if let Ok(b) = STANDARD.decode(pad(cleaned)) {
        return Ok(b);
    }
    Err(())
}

/// Percentage-decode a string. Errors are non-fatal — we return the
/// original on failure rather than panicking.
pub fn pct_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

#[allow(dead_code)]
pub fn b64_try(s: &str) -> Option<Vec<u8>> {
    decode_base64_loose(s).ok()
}

impl FromStr for Outbound {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_link(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scheme() {
        assert_eq!(scheme_of("vless://x"), Some("vless"));
        assert_eq!(scheme_of("SS://x"), Some("SS"));
        assert_eq!(scheme_of("hy2://x"), Some("hy2"));
        assert_eq!(scheme_of("foo bar"), None);
        assert_eq!(scheme_of("abc://"), Some("abc"));
    }

    #[test]
    fn b64_roundtrip() {
        let s = "hello world";
        let enc = STANDARD.encode(s);
        assert_eq!(decode_base64_loose(&enc).unwrap(), s.as_bytes());
    }

    #[test]
    fn empty_input_is_error() {
        assert!(matches!(parse_link(""), Err(ParseError::Empty)));
        assert!(matches!(parse_link("   "), Err(ParseError::Empty)));
    }
}
