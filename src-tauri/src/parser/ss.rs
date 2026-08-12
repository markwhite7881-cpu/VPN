//! Shadowsocks share-link parser.
//!
//! Two main formats exist:
//!
//! 1. **SIP002** (RFC-style URI):
//!    `ss://userinfo@host:port#name` where `userinfo` is
//!    `base64(method:password)`. Query params (`plugin`, `group`, etc.)
//!    are allowed.
//!
//! 2. **Legacy (v2rayN-style)**:
//!    `ss://base64(method:password@host:port)#name` — the whole
//!    `method:password@host:port` blob is base64-encoded as a single
//!    segment.
//!
//! Some clients also percent-encode the `userinfo` (`%3A` for `:`),
//! which we tolerate.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::{
    b64_try, Outbound, ParseError, SsOut,
};

pub fn parse(raw: &str) -> Result<Outbound, ParseError> {
    let after = raw
        .trim_start_matches("ss://")
        .trim_start_matches("shadowsocks://");

    // Detect legacy form: the blob after the scheme looks like base64
    // with no `@` in it, and the @-form starts with a `userinfo@...`.
    if after.contains('@') {
        parse_sip002(after)
    } else {
        parse_legacy(after)
    }
}

fn parse_sip002(after: &str) -> Result<Outbound, ParseError> {
    // Split fragment.
    let (main, fragment) = match after.find('#') {
        Some(i) => (&after[..i], Some(&after[i + 1..])),
        None => (after, None),
    };
    // Split off query string.
    let (userinfo_host_port, query) = match main.find('?') {
        Some(i) => (&main[..i], Some(&main[i + 1..])),
        None => (main, None),
    };

    let (userinfo, host_port) = userinfo_host_port
        .rsplit_once('@')
        .ok_or(ParseError::Url("missing '@' in ss://".into()))?;

    // In SIP002, userinfo is base64(method:password) (with some clients
    // additionally percent-encoding the base64). Decode that first.
    let userinfo_dec = percent_decode_userinfo(userinfo);
    let userinfo_bytes = b64_try(&userinfo_dec)
        .ok_or_else(|| ParseError::Base64(userinfo_dec.chars().take(40).collect()))?;
    let userinfo_str = String::from_utf8(userinfo_bytes).map_err(|_| ParseError::Utf8)?;
    let (method, password) = decode_userinfo(&userinfo_str)?;
    let (host, port) = split_host_port(host_port)?;

    let mut plugin = None;
    let mut plugin_opts = None;
    if let Some(q) = query {
        for (k, v) in form_iter(q) {
            match k.as_str() {
                "plugin" => {
                    // plugin takes the form "name;opts"
                    if let Some(idx) = v.find(';') {
                        plugin = Some(v[..idx].to_string());
                        plugin_opts = Some(v[idx + 1..].to_string());
                    } else {
                        plugin = Some(v);
                    }
                }
                _ => {}
            }
        }
    }

    let tag = fragment
        .map(|f| percent_decode_userinfo(f))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    Ok(Outbound::Shadowsocks(SsOut {
        tag,
        server: host,
        port,
        method,
        password,
        plugin,
        plugin_opts,
    }))
}

fn parse_legacy(after: &str) -> Result<Outbound, ParseError> {
    // The whole "method:password@host:port" is base64-encoded.
    // There may be a "#fragment" tail that is NOT part of the base64.
    let (payload, fragment) = match after.find('#') {
        Some(i) => (&after[..i], Some(&after[i + 1..])),
        None => (after, None),
    };
    let decoded = STANDARD
        .decode(payload)
        .or_else(|_| b64_try(payload).ok_or(()))
        .map_err(|_| ParseError::Base64(payload.chars().take(40).collect()))?;
    let decoded = String::from_utf8(decoded).map_err(|_| ParseError::Utf8)?;
    // "method:password@host:port"
    let (userinfo, host_port) = decoded
        .rsplit_once('@')
        .ok_or_else(|| ParseError::Url("missing '@' in decoded ss blob".into()))?;
    let (method, password) = decode_userinfo(userinfo)?;
    let (host, port) = split_host_port(host_port)?;

    let tag = fragment
        .map(|f| percent_decode_userinfo(f))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{host}:{port}"));

    Ok(Outbound::Shadowsocks(SsOut {
        tag,
        server: host,
        port,
        method,
        password,
        plugin: None,
        plugin_opts: None,
    }))
}

fn decode_userinfo(s: &str) -> Result<(String, String), ParseError> {
    // Some clients percent-encode the userinfo. We always try percent-decode
    // first; if the result still has a ':' we take it; else we treat the
    // original as `method:password` and split on the FIRST ':'.
    let dec = percent_decode_userinfo(s);
    let candidate = if dec.contains(':') { dec } else { s.to_string() };
    let (method, password) = candidate
        .split_once(':')
        .ok_or(ParseError::InvalidValue(
            "userinfo".to_string(),
            "expected method:password".into(),
        ))?;
    if method.is_empty() {
        return Err(ParseError::Missing("method".to_string()));
    }
    if password.is_empty() {
        return Err(ParseError::Missing("password".to_string()));
    }
    Ok((method.to_string(), password.to_string()))
}

fn split_host_port(s: &str) -> Result<(String, u16), ParseError> {
    // IPv6 literal: [::1]:8388
    if let Some(rest) = s.strip_prefix('[') {
        let (host, rest) = rest
            .split_once(']')
            .ok_or_else(|| ParseError::Url("unterminated '[...]'".into()))?;
        let port = rest
            .strip_prefix(':')
            .ok_or_else(|| ParseError::Port("missing port after ']'".into()))?;
        return Ok((
            host.to_string(),
            port.parse().map_err(|_| ParseError::Port(port.to_string()))?,
        ));
    }
    // Could still be a bare IPv6 without brackets — heuristic: more than
    // one ':' and no surrounding brackets is invalid; error.
    if s.matches(':').count() > 1 {
        return Err(ParseError::Url(format!(
            "bare IPv6 literal needs brackets: '{s}'"
        )));
    }
    let (host, port) = s
        .rsplit_once(':')
        .ok_or_else(|| ParseError::Url(format!("missing port in '{s}'")))?;
    let port: u16 = port
        .parse()
        .map_err(|_| ParseError::Port(port.to_string()))?;
    if host.is_empty() {
        return Err(ParseError::Missing("host".to_string()));
    }
    Ok((host.to_string(), port))
}

fn percent_decode_userinfo(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn form_iter(q: &str) -> impl Iterator<Item = (String, String)> + '_ {
    q.split('&').filter_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        let v = percent_encoding::percent_decode_str(v)
            .decode_utf8_lossy()
            .into_owned();
        Some((k.to_string(), v))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sip002_simple() {
        // userinfo: base64("chacha20-ietf-poly1305:secret")
        let userinfo = STANDARD.encode("chacha20-ietf-poly1305:secret");
        let link = format!("ss://{userinfo}@1.2.3.4:8388#MyNode");
        let out = parse(&link).expect("parse");
        match out {
            Outbound::Shadowsocks(s) => {
                assert_eq!(s.method, "chacha20-ietf-poly1305");
                assert_eq!(s.password, "secret");
                assert_eq!(s.server, "1.2.3.4");
                assert_eq!(s.port, 8388);
                assert_eq!(s.tag, "MyNode");
            }
            _ => panic!("expected Shadowsocks"),
        }
    }

    #[test]
    fn parses_sip002_with_plugin() {
        let userinfo = STANDARD.encode("aes-128-gcm:hello");
        let link = format!("ss://{userinfo}@example.com:443?plugin=obfs-local%3Bobfs%3Dhttp#Proxy");
        let out = parse(&link).expect("parse");
        match out {
            Outbound::Shadowsocks(s) => {
                assert_eq!(s.plugin.as_deref(), Some("obfs-local"));
                assert_eq!(s.plugin_opts.as_deref(), Some("obfs=http"));
            }
            _ => panic!("expected Shadowsocks"),
        }
    }

    #[test]
    fn parses_legacy_whole_blob() {
        // base64("aes-256-gcm:password@5.6.7.8:8388")
        let blob = STANDARD.encode("aes-256-gcm:password@5.6.7.8:8388");
        let link = format!("ss://{blob}#legacy-node");
        let out = parse(&link).expect("parse");
        match out {
            Outbound::Shadowsocks(s) => {
                assert_eq!(s.method, "aes-256-gcm");
                assert_eq!(s.password, "password");
                assert_eq!(s.server, "5.6.7.8");
                assert_eq!(s.port, 8388);
                assert_eq!(s.tag, "legacy-node");
            }
            _ => panic!("expected Shadowsocks"),
        }
    }

    #[test]
    fn parses_ipv6() {
        let userinfo = STANDARD.encode("chacha20-ietf-poly1305:abc");
        let link = format!("ss://{userinfo}@[2001:db8::1]:8388#v6");
        let out = parse(&link).expect("parse");
        match out {
            Outbound::Shadowsocks(s) => {
                assert_eq!(s.server, "2001:db8::1");
                assert_eq!(s.port, 8388);
            }
            _ => panic!("expected Shadowsocks"),
        }
    }

    #[test]
    fn rejects_missing_method() {
        let userinfo = STANDARD.encode(":password");
        let link = format!("ss://{userinfo}@1.2.3.4:8388");
        let err = parse(&link).unwrap_err();
        assert!(matches!(&err, ParseError::Missing(field) if field == "method"));
    }
}
