//! Hysteria2 share-link parser.
//!
//! Wire format (SagerNet-style):
//!   hy2://password@host:port?sni=...&obfs=salamander&obfs-password=...&insecure=0#name
//!
//! Some panels also accept `hysteria2://` and `hysteria://` (for v1) as
//! scheme aliases. We treat all three as v2 since v1 has a different
//! wire format that sing-box doesn't speak anyway.

use url::Url;

use super::{pct_decode, Hy2Obfs, Hy2Out, Outbound, ParseError, TlsCfg};

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    // Strip whichever scheme variant is present.
    let scheme = if let Some(rest) = raw.strip_prefix("hy2://") {
        ("hy2", rest)
    } else if let Some(rest) = raw.strip_prefix("hysteria2://") {
        ("hysteria2", rest)
    } else if let Some(rest) = raw.strip_prefix("hysteria://") {
        ("hysteria", rest)
    } else {
        return Err(ParseError::InvalidValue(
            "scheme".to_string(),
            "expected hy2://, hysteria2:// or hysteria://".into(),
        ));
    };
    let swapped = format!("http://{}", scheme.1);
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

    let sni = params.get("sni").or_else(|| params.get("peer")).cloned();
    let alpn: Vec<String> = params
        .get("alpn")
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default();
    let allow_insecure = matches!(
        params.get("insecure").map(String::as_str),
        Some("1" | "true")
    );

    let obfs = match params.get("obfs").map(String::as_str) {
        Some("salamander") => Some(Hy2Obfs {
            r#type: "salamander".to_string(),
            password: params
                .get("obfs-password")
                .cloned()
                .ok_or(ParseError::Missing("obfs-password".to_string()))?,
        }),
        Some(other) => {
            return Err(ParseError::InvalidValue(
                "obfs".to_string(),
                format!("unsupported obfs type '{other}'"),
            ));
        }
        None => None,
    };

    let up_mbps = params.get("upmbps").and_then(|s| s.parse().ok());
    let down_mbps = params.get("downmbps").and_then(|s| s.parse().ok());

    let tag = url
        .fragment()
        .map(pct_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    Ok(Outbound::Hysteria2(Hy2Out {
        tag,
        server: host,
        port,
        password,
        tls: TlsCfg {
            enabled: true,
            server_name: sni,
            alpn,
            fingerprint: None,
            reality: None,
            allow_insecure,
            ech: None,
        },
        obfs,
        up_mbps,
        down_mbps,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic() {
        let s = "hy2://mysecret@hy.example.com:443?sni=hy.example.com&obfs=salamander&obfs-password=p#Hy-Node";
        let out = parse(s).expect("parse");
        match out {
            Outbound::Hysteria2(h) => {
                assert_eq!(h.password, "mysecret");
                assert_eq!(h.port, 443);
                assert_eq!(h.tag, "Hy-Node");
                assert!(h.obfs.is_some());
                assert_eq!(h.obfs.as_ref().unwrap().password, "p");
            }
            _ => panic!("expected Hysteria2"),
        }
    }

    #[test]
    fn parses_hysteria2_alias() {
        let s = "hysteria2://pw@x.com:443?sni=x.com#n";
        let out = parse(s).expect("parse");
        assert!(matches!(out, Outbound::Hysteria2(_)));
    }

    #[test]
    fn rejects_missing_obfs_password() {
        let s = "hy2://pw@x.com:443?obfs=salamander#n";
        let err = parse(s).unwrap_err();
        assert!(
            matches!(&err, ParseError::Missing(field) if field == "obfs-password"),
            "unexpected error: {err:?}"
        );
    }
}
