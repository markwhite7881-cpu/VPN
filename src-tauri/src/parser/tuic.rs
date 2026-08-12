//! TUIC share-link parser.
//!
//! Wire format (TUIC v5):
//!   tuic://uuid:password@host:port?congestion_control=bbr&alpn=h2,h3&sni=...#name
//!
//! sing-box 1.9+ supports the v5 variant natively.

use url::Url;

use super::{pct_decode, Outbound, ParseError, TlsCfg, TuicCc, TuicOut, TuicUdp};

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    let swapped = raw.replacen("tuic://", "http://", 1);
    let url = Url::parse(&swapped).map_err(|e| ParseError::Url(e.to_string()))?;

    let host = url
        .host_str()
        .ok_or(ParseError::Missing("host".to_string()))?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| ParseError::Port("missing".into()))?;
    let uuid = {
        let u = url.username();
        if u.is_empty() {
            return Err(ParseError::Missing("uuid".to_string()));
        }
        pct_decode(u)
    };
    // The password is the part after the first ':' in userinfo. The url
    // crate puts it into `password()` only when it parses cleanly.
    let password = url
        .password()
        .map(pct_decode)
        .ok_or(ParseError::Missing("password".to_string()))?;

    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let cc = match params.get("congestion_control").map(String::as_str) {
        Some("bbr") => TuicCc::Bbr,
        Some("new_reno") | Some("new-reno") | Some("newReno") => TuicCc::NewReno,
        Some("cubic") | None => TuicCc::Cubic,
        Some(other) => {
            return Err(ParseError::InvalidValue(
                "congestion_control".to_string(),
                format!("unsupported '{other}'"),
            ));
        }
    };

    let udp = match params.get("udp_relay_mode").map(String::as_str) {
        Some("quic") => TuicUdp::Quic,
        _ => TuicUdp::Native,
    };

    let sni = params.get("sni").or_else(|| params.get("peer")).cloned();
    let alpn: Vec<String> = params
        .get("alpn")
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default();
    let allow_insecure =
        matches!(params.get("allow_insecure").map(String::as_str), Some("1" | "true"));
    let fingerprint = params.get("fp").cloned();

    let tag = url
        .fragment()
        .map(pct_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    Ok(Outbound::Tuic(TuicOut {
        tag,
        server: host,
        port,
        uuid,
        password,
        congestion_control: cc,
        udp_relay_mode: udp,
        tls: TlsCfg {
            enabled: true,
            server_name: sni,
            alpn,
            fingerprint,
            reality: None,
            allow_insecure,
            ech: None,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic() {
        let s = "tuic://11111111-2222-3333-4444-555555555555:pw@tuic.example.com:443?congestion_control=bbr&alpn=h2,h3&sni=tuic.example.com#TUIC-1";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Tuic(t) => {
                assert_eq!(t.uuid, "11111111-2222-3333-4444-555555555555");
                assert_eq!(t.password, "pw");
                assert_eq!(t.congestion_control, TuicCc::Bbr);
                assert_eq!(t.udp_relay_mode, TuicUdp::Native);
                assert_eq!(t.tls.alpn, vec!["h2", "h3"]);
                assert_eq!(t.tag, "TUIC-1");
            }
            _ => panic!("expected TUIC"),
        }
    }

    #[test]
    fn rejects_missing_password() {
        let s = "tuic://uuid@host:443?sni=h#n";
        let err = parse(s).unwrap_err();
        assert!(matches!(&err, ParseError::Missing(field) if field == "password"));
    }
}
