//! Trojan share-link parser.
//!
//! Wire format:
//!   trojan://password@host:port?sni=...&type=ws&security=tls&path=/ws&host=h#name
//!
//! Modern panels also use this scheme for Trojan with Reality, where
//! `security=reality` and `pbk`/`sid` are present.

use url::Url;

use super::{pct_decode, Outbound, ParseError, RealityCfg, TlsCfg, Transport, TrojanOut};

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    let swapped = raw.replacen("trojan://", "http://", 1);
    let url = Url::parse(&swapped).map_err(|e| ParseError::Url(e.to_string()))?;

    let host = url
        .host_str()
        .ok_or(ParseError::Missing("host".to_string()))?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| ParseError::Port("missing".into()))?;
    let password = {
        let u = url.username();
        if u.is_empty() {
            return Err(ParseError::Missing("password".to_string()));
        }
        pct_decode(u)
    };

    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let type_p = params.get("type").map(String::as_str).unwrap_or("tcp");
    let security = params.get("security").map(String::as_str).unwrap_or("tls");

    let transport = match type_p {
        "tcp" | "" => Transport::Tcp,
        "ws" => {
            let path = params.get("path").cloned();
            let host = params.get("host").cloned().unwrap_or_default();
            let mut headers = Vec::new();
            if !host.is_empty() {
                headers.push(("Host".to_string(), host));
            }
            Transport::Ws { path, headers }
        }
        "grpc" => Transport::Grpc {
            service_name: params.get("serviceName").cloned(),
            idle_timeout: None,
            ping_timeout: None,
        },
        other => {
            return Err(ParseError::InvalidValue(
                "type".to_string(),
                format!("unknown transport '{other}'"),
            ));
        }
    };

    let sni = params.get("sni").or_else(|| params.get("peer")).cloned();
    let alpn: Vec<String> = params
        .get("alpn")
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default();
    let fp = params.get("fp").cloned();
    let allow_insecure = matches!(
        params.get("allowInsecure").map(String::as_str),
        Some("1" | "true")
    );

    let reality = if security == "reality" {
        let pbk = params
            .get("pbk")
            .cloned()
            .ok_or(ParseError::Missing("pbk (Reality public key)".to_string()))?;
        let sid = params
            .get("sid")
            .cloned()
            .ok_or(ParseError::Missing("sid (Reality short id)".to_string()))?;
        Some(RealityCfg {
            public_key: pbk,
            short_id: sid,
            spider_x: params.get("spx").cloned(),
        })
    } else {
        None
    };

    let tls = TlsCfg {
        enabled: security != "none",
        server_name: sni,
        alpn,
        fingerprint: fp,
        reality,
        allow_insecure,
        ech: None,
    };

    let tag = url
        .fragment()
        .map(pct_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    Ok(Outbound::Trojan(TrojanOut {
        tag,
        server: host,
        port,
        password,
        transport,
        tls,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_tls() {
        let s = "trojan://supersecret@tr.example.com:443?sni=tr.example.com&alpn=h2#TR-1";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Trojan(t) => {
                assert_eq!(t.password, "supersecret");
                assert_eq!(t.server, "tr.example.com");
                assert_eq!(t.port, 443);
                assert_eq!(t.tag, "TR-1");
                assert!(t.tls.enabled);
                assert_eq!(t.tls.server_name.as_deref(), Some("tr.example.com"));
            }
            _ => panic!("expected Trojan"),
        }
    }

    #[test]
    fn parses_reality() {
        let s = "trojan://pw@host.com:443?security=reality&pbk=K&sid=S&sni=x.com#node";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Trojan(t) => {
                assert!(t.tls.reality.is_some());
                assert_eq!(t.tls.reality.as_ref().unwrap().public_key, "K");
            }
            _ => panic!("expected Trojan"),
        }
    }

    #[test]
    fn rejects_missing_password() {
        let s = "trojan://@host.com:443#t";
        let err = parse(s).unwrap_err();
        assert!(matches!(&err, ParseError::Missing(field) if field == "password"));
    }
}
