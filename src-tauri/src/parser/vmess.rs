//! VMess share-link parser.
//!
//! Two formats circulate in the wild:
//! 1. **Standard (v2rayN-style)**: `vmess://<base64(JSON)>`
//! 2. **v2rayNG-style URI**: `vmess://<base64(whole URI without scheme)>`
//!
//! We handle both. The decoded JSON looks like:
//! ```json
//! {
//!   "v": "2", "ps": "name", "add": "host", "port": "443",
//!   "id": "uuid", "aid": "0", "scy": "auto",
//!   "net": "ws", "type": "none", "host": "header.host",
//!   "path": "/ws", "tls": "tls", "sni": "sni.host",
//!   "alpn": "h2", "fp": "chrome"
//! }
//! ```

// Engine trait is required at module scope for the `tests` submodule
// which uses `STANDARD.encode(...)` and shares scope via `use super::*`.
#[allow(unused_imports)]
use base64::engine::general_purpose::STANDARD;
#[allow(unused_imports)]
use base64::Engine as _;
use serde::Deserialize;

use super::{b64_try, Outbound, ParseError, TlsCfg, Transport, VmessCipher, VmessOut};

#[derive(Debug, Deserialize)]
struct RawVmess {
    #[serde(default)]
    ps: Option<String>,
    add: String,
    /// Some panels serialize port as a number, others as a string.
    /// We accept both via a custom parser below; this field is unused.
    #[allow(dead_code)]
    port: serde_json::Value,
    id: String,
    #[serde(default)]
    aid: Option<serde_json::Value>,
    #[serde(default)]
    scy: Option<String>,
    #[serde(default)]
    net: Option<String>,
    /// Header type for HTTP transport ("none" or "http"). Reserved for
    /// future use; some panels emit it but the underlying sing-box
    /// transport doesn't need it once we split host/path.
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    tls: Option<String>,
    #[serde(default)]
    sni: Option<String>,
    #[serde(default)]
    alpn: Option<String>,
    #[serde(default)]
    fp: Option<String>,
}

fn json_to_port(v: &serde_json::Value) -> Result<u16, ParseError> {
    if let Some(n) = v.as_u64() {
        return u16::try_from(n).map_err(|_| ParseError::Port(n.to_string()));
    }
    if let Some(s) = v.as_str() {
        return s.parse().map_err(|_| ParseError::Port(s.to_string()));
    }
    Err(ParseError::InvalidValue(
        "port".to_string(),
        format!("expected number or string, got {v}"),
    ))
}

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    let after = raw.strip_prefix("vmess://").ok_or_else(|| {
        ParseError::InvalidValue("scheme".to_string(), "expected vmess://".into())
    })?;
    // Some clients embed the whole URI in base64, others embed a JSON.
    // Try the JSON variant first (it has '{' or 'v' as the first char).
    let bytes =
        b64_try(after).ok_or_else(|| ParseError::Base64(after.chars().take(40).collect()))?;
    let decoded = String::from_utf8(bytes).map_err(|_| ParseError::Utf8)?;
    let decoded = decoded.trim();

    // If the decoded payload itself contains `://`, it was a URI
    // (e.g. `vmess://host:port?`...). We don't bother supporting that
    // legacy form — modern subscriptions use JSON.

    let raw: RawVmess = serde_json::from_str(decoded)
        .map_err(|e| ParseError::InvalidValue("vmess_json".to_string(), e.to_string()))?;
    let port = json_to_port(&raw.port)?;
    let alter_id = raw
        .aid
        .as_ref()
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(0);

    let net = raw.net.as_deref().unwrap_or("tcp");
    let transport = match net {
        "tcp" => Transport::Tcp,
        "ws" => {
            let mut headers = Vec::new();
            if let Some(h) = &raw.host {
                headers.push(("Host".to_string(), h.clone()));
            }
            Transport::Ws {
                path: raw.path.clone(),
                headers,
            }
        }
        "http" | "h2" => {
            let host = raw
                .host
                .as_deref()
                .map(|h| h.split(',').map(str::trim).map(String::from).collect())
                .unwrap_or_default();
            Transport::Http {
                host,
                path: raw.path.clone(),
            }
        }
        "grpc" => Transport::Grpc {
            service_name: raw.path.clone(),
            idle_timeout: None,
            ping_timeout: None,
        },
        other => {
            return Err(ParseError::InvalidValue(
                "net".to_string(),
                format!("unknown transport '{other}'"),
            ));
        }
    };

    let tls_enabled = matches!(raw.tls.as_deref(), Some("tls" | "reality"));
    let alpn: Vec<String> = raw
        .alpn
        .as_deref()
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default();

    let tls = TlsCfg {
        enabled: tls_enabled,
        server_name: raw.sni.clone(),
        alpn,
        fingerprint: raw.fp.clone(),
        reality: None,
        allow_insecure: false,
        ech: None,
    };

    Ok(Outbound::Vmess(VmessOut {
        tag: raw.ps.unwrap_or_default(),
        server: raw.add,
        port,
        uuid: raw.id,
        alter_id,
        cipher: parse_cipher(raw.scy.as_deref()),
        transport,
        tls,
    }))
}

fn parse_cipher(s: Option<&str>) -> VmessCipher {
    match s.map(str::to_ascii_lowercase).as_deref() {
        Some("aes-128-gcm") | Some("aes128gcm") => VmessCipher::Aes128Gcm,
        Some("chacha20-poly1305") | Some("chacha20-ietf-poly1305") => VmessCipher::Chacha20Poly1305,
        Some("none") => VmessCipher::None,
        _ => VmessCipher::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v2rayn_json() {
        let json = r#"{"v":"2","ps":"🇩🇪 DE-node","add":"de.example.com","port":"443","id":"11111111-2222-3333-4444-555555555555","aid":"0","scy":"auto","net":"ws","type":"none","host":"de.example.com","path":"/ws","tls":"tls","sni":"de.example.com","alpn":"h2,http/1.1","fp":"chrome"}"#;
        let enc = STANDARD.encode(json);
        let link = format!("vmess://{enc}");
        let out = parse(&link).expect("parse");
        match out {
            Outbound::Vmess(v) => {
                assert_eq!(v.server, "de.example.com");
                assert_eq!(v.port, 443);
                assert_eq!(v.uuid, "11111111-2222-3333-4444-555555555555");
                assert_eq!(v.alter_id, 0);
                assert_eq!(v.cipher, VmessCipher::Auto);
                assert!(v.tls.enabled);
                assert!(v.tag.starts_with("🇩🇪"));
            }
            _ => panic!("expected VMess"),
        }
    }

    #[test]
    fn rejects_invalid_base64() {
        let link = "vmess://!!!not-base64!!!";
        assert!(matches!(parse(link), Err(ParseError::Base64(_))));
    }

    #[test]
    fn rejects_invalid_json_after_decode() {
        let enc = STANDARD.encode("not json");
        let link = format!("vmess://{enc}");
        let err = parse(&link).unwrap_err();
        assert!(
            matches!(&err, ParseError::InvalidValue(field, _) if field == "vmess_json"),
            "unexpected error: {err:?}"
        );
    }
}
