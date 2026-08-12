//! VLESS share-link parser.
//!
//! Wire format:
//!   vless://UUID@host:port?type=...&security=...&flow=...&sni=...&fp=...&pbk=...&sid=...&spx=...
//!         &alpn=...&path=...&host=...&serviceName=...#name
//!
//! `type` ∈ {tcp, ws, grpc, http, h2}
//! `security` ∈ {none, tls, reality}
//! `flow` — only "xtls-rprx-vision" is meaningful; pass-through otherwise.

use url::Url;

use super::{
    pct_decode, EchCfg, Outbound, ParseError, RealityCfg, TlsCfg, Transport, VlessOut,
};

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    // url::Url doesn't accept `vless://` (it's not a registered scheme
    // and the URL crate is strict). Workaround: swap the scheme, parse,
    // then re-read.
    let swapped = raw.replacen("vless://", "http://", 1);
    let url = Url::parse(&swapped).map_err(|e| ParseError::Url(e.to_string()))?;

    let host = url
        .host_str()
        .ok_or(ParseError::Missing("host".to_string()))?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| ParseError::Port("missing".into()))?;

    let uuid = match url.username() {
        "" => return Err(ParseError::Missing("uuid".to_string())),
        u => pct_decode(u),
    };
    // url::Url percent-decodes the username; we don't need pct_decode here
    // for `uuid`, but the rest of the params come pre-decoded.

    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let type_p = params
        .get("type")
        .map(String::as_str)
        .unwrap_or("tcp");
    let security = params
        .get("security")
        .map(String::as_str)
        .unwrap_or("none");
    let flow = params.get("flow").cloned();
    let tag = url
        .fragment()
        .map(pct_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    let transport = parse_transport(type_p, &params)?;
    let tls = parse_tls(security, &params)?;

    Ok(Outbound::Vless(VlessOut {
        tag,
        server: host,
        port,
        uuid,
        flow,
        transport,
        tls,
    }))
}

fn parse_transport(
    type_p: &str,
    p: &std::collections::HashMap<String, String>,
) -> Result<Transport, ParseError> {
    match type_p {
        "tcp" | "" => Ok(Transport::Tcp),
        "ws" => {
            let path = p.get("path").cloned();
            let host = p.get("host").cloned().unwrap_or_default();
            let mut headers = Vec::new();
            if !host.is_empty() {
                headers.push(("Host".to_string(), host));
            }
            Ok(Transport::Ws { path, headers })
        }
        "http" | "h2" => {
            let host = p
                .get("host")
                .map(|h| h.split(',').map(str::trim).map(String::from).collect())
                .unwrap_or_default();
            Ok(Transport::Http {
                host,
                path: p.get("path").cloned(),
            })
        }
        // sing-box 1.11+ introduces the "xhttp" transport (HTTP/2 with
        // optional upgrade). It is wire-compatible with `http` for
        // the basic fields, but sing-box distinguishes them server-side.
        "xhttp" => {
            let host = p
                .get("host")
                .map(|h| h.split(',').map(str::trim).map(String::from).collect())
                .unwrap_or_default();
            Ok(Transport::Xhttp {
                host,
                path: p.get("path").cloned(),
                mode: p.get("mode").cloned(),
            })
        }
        "grpc" => Ok(Transport::Grpc {
            service_name: p.get("serviceName").cloned(),
            idle_timeout: p.get("idleTimeout").cloned(),
            ping_timeout: p.get("pingTimeout").cloned(),
        }),
        other => Err(ParseError::InvalidValue(
            "type".to_string(),
            format!("unknown transport '{other}'"),
        )),
    }
}

fn parse_tls(
    security: &str,
    p: &std::collections::HashMap<String, String>,
) -> Result<TlsCfg, ParseError> {
    if security == "none" || security.is_empty() {
        return Ok(TlsCfg::default());
    }
    let sni = p.get("sni").or_else(|| p.get("peer")).cloned();
    let alpn: Vec<String> = p
        .get("alpn")
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default();
    let fp = p.get("fp").cloned();
    let allow_insecure = matches!(p.get("allowInsecure").map(String::as_str), Some("1" | "true"));

    let reality = if security == "reality" {
        let pbk = p
            .get("pbk")
            .cloned()
            .ok_or(ParseError::Missing("pbk (Reality public key)".to_string()))?;
        let sid = p
            .get("sid")
            .cloned()
            .ok_or(ParseError::Missing("sid (Reality short id)".to_string()))?;
        Some(RealityCfg {
            public_key: pbk,
            short_id: sid,
            spider_x: p.get("spx").cloned().filter(|s| !s.is_empty()),
        })
    } else {
        None
    };

    let ech = p.get("ech").map(|cfg| EchCfg { config: cfg.clone() });

    Ok(TlsCfg {
        enabled: true,
        server_name: sni,
        alpn,
        fingerprint: fp,
        reality,
        allow_insecure,
        ech,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reality_vision() {
        let s = "vless://b2a3d6c8-1111-2222-3333-444455556666@de.example.com:443?type=tcp&security=reality&pbk=PUBKEY&sid=ABCD&fp=chrome&sni=cdn.example.com&flow=xtls-rprx-vision&alpn=h2,http/1.1#%F0%9F%87%A9%F0%9F%87%AA%20DE-1";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Vless(v) => {
                assert_eq!(v.server, "de.example.com");
                assert_eq!(v.port, 443);
                assert_eq!(v.uuid, "b2a3d6c8-1111-2222-3333-444455556666");
                assert_eq!(v.flow.as_deref(), Some("xtls-rprx-vision"));
                assert!(v.tls.reality.is_some());
                assert_eq!(v.tls.reality.as_ref().unwrap().public_key, "PUBKEY");
                assert_eq!(v.tls.reality.as_ref().unwrap().short_id, "ABCD");
                assert_eq!(v.tls.alpn, vec!["h2", "http/1.1"]);
                assert_eq!(v.tls.fingerprint.as_deref(), Some("chrome"));
                assert_eq!(v.tls.server_name.as_deref(), Some("cdn.example.com"));
                assert!(v.tag.contains("DE-1"));
            }
            _ => panic!("expected VLESS"),
        }
    }

    #[test]
    fn parses_websocket() {
        let s = "vless://11111111-2222-3333-4444-555555555555@ws.example.org:8443?type=ws&security=tls&sni=ws.example.org&path=/ws&host=ws.example.org#WS-Node";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Vless(v) => {
                assert_eq!(v.port, 8443);
                assert_eq!(v.tag, "WS-Node");
                match &v.transport {
                    Transport::Ws { path, headers } => {
                        assert_eq!(path.as_deref(), Some("/ws"));
                        assert_eq!(headers, &vec![("Host".to_string(), "ws.example.org".to_string())]);
                    }
                    _ => panic!("expected WS transport"),
                }
            }
            _ => panic!("expected VLESS"),
        }
    }

    #[test]
    fn rejects_missing_uuid() {
        let s = "vless://@de.example.com:443?type=tcp#tag";
        let err = parse(s).unwrap_err();
        assert!(matches!(&err, ParseError::Missing(field) if field == "uuid"));
    }

    #[test]
    fn rejects_missing_reality_pbk() {
        let s = "vless://11111111-2222-3333-4444-555555555555@x.com:443?security=reality#t";
        let err = parse(s).unwrap_err();
        assert!(
            matches!(&err, ParseError::Missing(field) if field == "pbk (Reality public key)"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn parses_xhttp_transport() {
        let s = "vless://11111111-2222-3333-4444-555555555555@x.com:443?type=xhttp&security=tls&sni=x.com&path=/x&host=x.com&mode=auto#X-Node";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Vless(v) => {
                assert_eq!(v.tag, "X-Node");
                match &v.transport {
                    Transport::Xhttp { host, path, mode } => {
                        assert_eq!(host, &vec!["x.com".to_string()]);
                        assert_eq!(path.as_deref(), Some("/x"));
                        assert_eq!(mode.as_deref(), Some("auto"));
                    }
                    other => panic!("expected Xhttp, got {other:?}"),
                }
            }
            _ => panic!("expected VLESS"),
        }
    }
}
