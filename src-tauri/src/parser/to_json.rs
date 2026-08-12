//! Convert a parsed [`Outbound`] into a sing-box `outbounds[]` entry.
//!
//! Sing-box's outbound schemas differ per protocol. This module centralises
//! the field mapping so the rest of the codebase only deals with
//! [`Outbound`] values.
//!
//! Reference:
//!   * https://sing-box.sagernet.org/configuration/outbound/vless/
//!   * https://sing-box.sagernet.org/configuration/outbound/vmess/
//!   * https://sing-box.sagernet.org/configuration/outbound/trojan/
//!   * https://sing-box.sagernet.org/configuration/outbound/shadowsocks/
//!   * https://sing-box.sagernet.org/configuration/outbound/hysteria2/
//!   * https://sing-box.sagernet.org/configuration/outbound/tuic/

use serde_json::{json, Map, Value};

use super::{
    Outbound, Transport, TuicCc, TuicUdp, VmessCipher,
};

impl Outbound {
    /// Render this outbound as a sing-box JSON object.
    pub fn to_singbox_json(&self) -> Value {
        match self {
            Outbound::Vless(v) => vless_to_json(v),
            Outbound::Vmess(v) => vmess_to_json(v),
            Outbound::Trojan(v) => trojan_to_json(v),
            Outbound::Shadowsocks(v) => ss_to_json(v),
            Outbound::Hysteria2(v) => hy2_to_json(v),
            Outbound::Tuic(v) => tuic_to_json(v),
            Outbound::Unsupported { .. } => {
                // Should never reach a live config — but emit a placeholder
                // so the JSON is at least syntactically valid.
                json!({
                    "type": "block",
                    "tag": "unsupported-placeholder"
                })
            }
        }
    }
}

// -- helpers ----------------------------------------------------------

fn transport_json(t: &Transport) -> Option<Value> {
    match t {
        Transport::Tcp => None,
        Transport::Ws { path, headers } => {
            let mut o = Map::new();
            o.insert("type".into(), Value::String("ws".into()));
            if let Some(p) = path {
                o.insert("path".into(), Value::String(p.clone()));
            }
            if !headers.is_empty() {
                let mut h = Map::new();
                for (k, v) in headers {
                    h.insert(k.clone(), Value::String(v.clone()));
                }
                o.insert("headers".into(), Value::Object(h));
            }
            Some(Value::Object(o))
        }
        Transport::Http { host, path } => {
            let mut o = Map::new();
            o.insert("type".into(), Value::String("http".into()));
            if !host.is_empty() {
                o.insert(
                    "host".into(),
                    Value::Array(host.iter().cloned().map(Value::String).collect()),
                );
            }
            if let Some(p) = path {
                o.insert("path".into(), Value::String(p.clone()));
            }
            Some(Value::Object(o))
        }
        Transport::Xhttp { host, path, mode } => {
            let mut o = Map::new();
            o.insert("type".into(), Value::String("xhttp".into()));
            // sing-box-lx (and sing-box 1.14+) expect `host` as a
            // single string, not an array like the legacy `http`
            // transport. Use the first entry, fall back to the
            // outbound's server_name-equivalent if empty.
            if let Some(first) = host.first() {
                o.insert("host".into(), Value::String(first.clone()));
            }
            if let Some(p) = path {
                o.insert("path".into(), Value::String(p.clone()));
            }
            if let Some(m) = mode {
                o.insert("mode".into(), Value::String(m.clone()));
            }
            Some(Value::Object(o))
        }
        Transport::Grpc {
            service_name,
            idle_timeout,
            ping_timeout,
        } => {
            let mut o = Map::new();
            o.insert("type".into(), Value::String("grpc".into()));
            if let Some(s) = service_name {
                o.insert("service_name".into(), Value::String(s.clone()));
            }
            if let Some(t) = idle_timeout {
                o.insert("idle_timeout".into(), Value::String(t.clone()));
            }
            if let Some(t) = ping_timeout {
                o.insert("ping_timeout".into(), Value::String(t.clone()));
            }
            Some(Value::Object(o))
        }
        Transport::Udp => None,
    }
}

fn tls_json(
    enabled: bool,
    sni: Option<&String>,
    alpn: &[String],
    fp: Option<&String>,
    reality: Option<&super::RealityCfg>,
    allow_insecure: bool,
) -> Option<Value> {
    if !enabled && reality.is_none() {
        return None;
    }
    let mut o = Map::new();
    o.insert("enabled".into(), Value::Bool(true));
    if let Some(s) = sni {
        o.insert("server_name".into(), Value::String(s.clone()));
    }
    if !alpn.is_empty() {
        o.insert(
            "alpn".into(),
            Value::Array(alpn.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(r) = reality {
        o.insert(
            "reality".into(),
            json!({
                "enabled": true,
                "public_key": r.public_key,
                "short_id": r.short_id,
            }),
        );
    }
    // sing-box 1.13 demands explicit `enabled: true` on utls.
    if let Some(f) = fp {
        o.insert(
            "utls".into(),
            json!({ "enabled": true, "fingerprint": f }),
        );
    } else if reality.is_some() {
        // Reality clients must have a uTLS fingerprint; default to chrome.
        o.insert(
            "utls".into(),
            json!({ "enabled": true, "fingerprint": "chrome" }),
        );
    }
    if allow_insecure {
        o.insert("insecure".into(), Value::Bool(true));
    }
    Some(Value::Object(o))
}

// -- per-protocol conversions -----------------------------------------

fn vless_to_json(v: &super::VlessOut) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("vless".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("uuid".into(), Value::String(v.uuid.clone()));
    if let Some(flow) = &v.flow {
        o.insert("flow".into(), Value::String(flow.clone()));
    }
    if let Some(t) = transport_json(&v.transport) {
        o.insert("transport".into(), t);
    }
    if let Some(t) = tls_json(
        v.tls.enabled,
        v.tls.server_name.as_ref(),
        &v.tls.alpn,
        v.tls.fingerprint.as_ref(),
        v.tls.reality.as_ref(),
        v.tls.allow_insecure,
    ) {
        o.insert("tls".into(), t);
    }
    Value::Object(o)
}

fn vmess_to_json(v: &super::VmessOut) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("vmess".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("uuid".into(), Value::String(v.uuid.clone()));
    o.insert("alter_id".into(), json!(v.alter_id));
    if v.cipher != VmessCipher::Auto {
        o.insert(
            "security".into(),
            Value::String(cipher_to_str(v.cipher).to_string()),
        );
    }
    if let Some(t) = transport_json(&v.transport) {
        o.insert("transport".into(), t);
    }
    if let Some(t) = tls_json(
        v.tls.enabled,
        v.tls.server_name.as_ref(),
        &v.tls.alpn,
        v.tls.fingerprint.as_ref(),
        None,
        v.tls.allow_insecure,
    ) {
        o.insert("tls".into(), t);
    }
    Value::Object(o)
}

fn cipher_to_str(c: VmessCipher) -> &'static str {
    match c {
        VmessCipher::Auto => "auto",
        VmessCipher::Aes128Gcm => "aes-128-gcm",
        VmessCipher::Chacha20Poly1305 => "chacha20-poly1305",
        VmessCipher::None => "none",
    }
}

fn trojan_to_json(v: &super::TrojanOut) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("trojan".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("password".into(), Value::String(v.password.clone()));
    if let Some(t) = transport_json(&v.transport) {
        o.insert("transport".into(), t);
    }
    if let Some(t) = tls_json(
        v.tls.enabled,
        v.tls.server_name.as_ref(),
        &v.tls.alpn,
        v.tls.fingerprint.as_ref(),
        v.tls.reality.as_ref(),
        v.tls.allow_insecure,
    ) {
        o.insert("tls".into(), t);
    }
    Value::Object(o)
}

fn ss_to_json(v: &super::SsOut) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("shadowsocks".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("method".into(), Value::String(v.method.clone()));
    o.insert("password".into(), Value::String(v.password.clone()));
    if let Some(plugin) = &v.plugin {
        o.insert("plugin".into(), Value::String(plugin.clone()));
    }
    if let Some(opts) = &v.plugin_opts {
        o.insert("plugin_opts".into(), Value::String(opts.clone()));
    }
    Value::Object(o)
}

fn hy2_to_json(v: &super::Hy2Out) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("hysteria2".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("password".into(), Value::String(v.password.clone()));
    if let Some(obfs) = &v.obfs {
        o.insert(
            "obfs".into(),
            json!({
                "type": obfs.r#type,
                "password": obfs.password,
            }),
        );
    }
    if let Some(up) = v.up_mbps {
        o.insert("up_mbps".into(), json!(up));
    }
    if let Some(down) = v.down_mbps {
        o.insert("down_mbps".into(), json!(down));
    }
    if let Some(t) = tls_json(
        v.tls.enabled,
        v.tls.server_name.as_ref(),
        &v.tls.alpn,
        v.tls.fingerprint.as_ref(),
        None,
        v.tls.allow_insecure,
    ) {
        o.insert("tls".into(), t);
    }
    Value::Object(o)
}

fn tuic_to_json(v: &super::TuicOut) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), Value::String("tuic".into()));
    o.insert("tag".into(), Value::String(v.tag.clone()));
    o.insert("server".into(), Value::String(v.server.clone()));
    o.insert("server_port".into(), json!(v.port));
    o.insert("uuid".into(), Value::String(v.uuid.clone()));
    o.insert("password".into(), Value::String(v.password.clone()));
    o.insert(
        "congestion_control".into(),
        Value::String(match v.congestion_control {
            TuicCc::Cubic => "cubic".into(),
            TuicCc::NewReno => "new_reno".into(),
            TuicCc::Bbr => "bbr".into(),
        }),
    );
    o.insert(
        "udp_relay_mode".into(),
        Value::String(match v.udp_relay_mode {
            TuicUdp::Native => "native".into(),
            TuicUdp::Quic => "quic".into(),
        }),
    );
    if let Some(t) = tls_json(
        v.tls.enabled,
        v.tls.server_name.as_ref(),
        &v.tls.alpn,
        v.tls.fingerprint.as_ref(),
        None,
        v.tls.allow_insecure,
    ) {
        o.insert("tls".into(), t);
    }
    Value::Object(o)
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_link;

    #[test]
    fn vless_reality_serialises() {
        let link = "vless://b2a3d6c8-1111-2222-3333-444455556666@de.example.com:443?type=tcp&security=reality&pbk=PUB&sid=ABCD&fp=chrome&sni=cdn.example.com&flow=xtls-rprx-vision#n";
        let out = parse_link(link).expect("parse");
        let json = out.to_singbox_json();
        assert_eq!(json["type"], "vless");
        assert_eq!(json["server"], "de.example.com");
        assert_eq!(json["server_port"], 443);
        assert_eq!(json["uuid"], "b2a3d6c8-1111-2222-3333-444455556666");
        assert_eq!(json["flow"], "xtls-rprx-vision");
        assert_eq!(json["tls"]["reality"]["public_key"], "PUB");
        assert_eq!(json["tls"]["reality"]["short_id"], "ABCD");
    }

    #[test]
    fn hy2_obfs_serialises() {
        let link = "hy2://pw@x.com:443?sni=x.com&obfs=salamander&obfs-password=op#n";
        let out = parse_link(link).expect("parse");
        let json = out.to_singbox_json();
        assert_eq!(json["type"], "hysteria2");
        assert_eq!(json["obfs"]["type"], "salamander");
        assert_eq!(json["obfs"]["password"], "op");
    }
}
