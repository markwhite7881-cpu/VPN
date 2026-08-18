//! Safe, engine-owned metadata for Xray ready profiles.
//!
//! This module deliberately keeps endpoint extraction and probing inside Rust.
//! Only the two presentation fields below cross the Tauri boundary.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::error::AppResult;
use crate::subscriptions::ResolvedChildProfile;

const PROBE_TIMEOUT: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomeProfileMetadata {
    pub country_code: Option<String>,
    pub latency_ms: Option<u32>,
}

/// Resolve safe metadata without exposing the stored provider configuration.
pub async fn resolve_profile_metadata(
    profile: &ResolvedChildProfile,
) -> AppResult<HomeProfileMetadata> {
    if profile.engine != crate::subscriptions::EngineKind::Xray {
        return Ok(HomeProfileMetadata {
            country_code: None,
            latency_ms: None,
        });
    }

    let country_code = infer_country_code(&profile.name);
    let Some((host, port)) = extract_endpoint(&profile.config) else {
        return Ok(HomeProfileMetadata {
            country_code,
            latency_ms: None,
        });
    };

    let started = Instant::now();
    let connected = tokio::time::timeout(
        PROBE_TIMEOUT,
        tokio::net::TcpStream::connect((host.as_str(), port)),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if !connected {
        return Ok(HomeProfileMetadata {
            country_code,
            latency_ms: None,
        });
    }
    let latency_ms = Some(started.elapsed().as_millis().min(u32::MAX as u128) as u32);

    Ok(HomeProfileMetadata {
        country_code,
        latency_ms,
    })
}

/// Test-facing pure presentation helper. Its input and output are safe to inspect.
pub fn metadata_for_config(config: &Value, probe: Option<(String, u32)>) -> HomeProfileMetadata {
    let country_code = config
        .get("remarks")
        .and_then(Value::as_str)
        .and_then(infer_country_code);
    let Some(_endpoint) = extract_endpoint(config) else {
        return HomeProfileMetadata {
            country_code: None,
            latency_ms: None,
        };
    };
    HomeProfileMetadata {
        country_code,
        latency_ms: probe.map(|(_, latency)| latency),
    }
}

/// Extract only a host and port for supported Xray outbound forms.
pub fn extract_endpoint(config: &Value) -> Option<(String, u16)> {
    if let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) {
        for outbound in outbounds {
            if let Some(endpoint) = extract_from_outbound(outbound) {
                return Some(endpoint);
            }
        }
    }
    extract_from_outbound(config)
}

fn extract_from_outbound(value: &Value) -> Option<(String, u16)> {
    let protocol = value.get("protocol").and_then(Value::as_str)?;
    if !matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks") {
        return value
            .get("outbounds")
            .and_then(Value::as_array)
            .and_then(|nested| nested.iter().find_map(extract_from_outbound));
    }
    let settings = value.get("settings")?;
    let servers = settings
        .get("vnext")
        .or_else(|| settings.get("servers"))
        .and_then(Value::as_array)?;
    let first = servers.first()?;
    let host = first
        .get("address")
        .or_else(|| first.get("server"))
        .and_then(Value::as_str)?
        .trim();
    let port = first.get("port").and_then(Value::as_u64)?;
    if host.is_empty() || host.len() > 253 || port == 0 || port > u16::MAX as u64 {
        return None;
    }
    Some((host.to_owned(), port as u16))
}

fn infer_country_code(label: &str) -> Option<String> {
    let regional_indicators = label
        .chars()
        .filter(|character| ('🇦'..='🇿').contains(character))
        .collect::<Vec<_>>();
    if let [first, second, ..] = regional_indicators.as_slice() {
        let first = (*first as u32).checked_sub('🇦' as u32)?;
        let second = (*second as u32).checked_sub('🇦' as u32)?;
        if first < 26 && second < 26 {
            return Some(format!(
                "{}{}",
                (b'A' + first as u8) as char,
                (b'A' + second as u8) as char,
            ));
        }
    }

    let normalized = label.to_lowercase();
    for (name, code) in COUNTRY_NAMES {
        if normalized.contains(name) {
            return Some((*code).to_owned());
        }
    }

    label
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|part| part.len() == 2)
        .map(str::to_ascii_uppercase)
        .find(|part| is_country_code(part))
}

fn is_country_code(code: &str) -> bool {
    matches!(
        code,
        "AT" | "AU"
            | "BE"
            | "BR"
            | "CA"
            | "CH"
            | "CN"
            | "CZ"
            | "DE"
            | "DK"
            | "ES"
            | "FI"
            | "FR"
            | "GB"
            | "HK"
            | "IE"
            | "IL"
            | "IN"
            | "IT"
            | "JP"
            | "KR"
            | "LT"
            | "LU"
            | "NL"
            | "NO"
            | "NZ"
            | "PL"
            | "PT"
            | "RO"
            | "SE"
            | "SG"
            | "TR"
            | "TW"
            | "UA"
            | "US"
            | "VN"
            | "KZ"
            | "RU"
    )
}

const COUNTRY_NAMES: &[(&str, &str)] = &[
    ("united kingdom", "GB"),
    ("great britain", "GB"),
    ("england", "GB"),
    ("netherlands", "NL"),
    ("holland", "NL"),
    ("germany", "DE"),
    ("france", "FR"),
    ("spain", "ES"),
    ("italy", "IT"),
    ("poland", "PL"),
    ("sweden", "SE"),
    ("finland", "FI"),
    ("norway", "NO"),
    ("denmark", "DK"),
    ("ireland", "IE"),
    ("belgium", "BE"),
    ("portugal", "PT"),
    ("greece", "GR"),
    ("ukraine", "UA"),
    ("belarus", "BY"),
    ("japan", "JP"),
    ("korea", "KR"),
    ("china", "CN"),
    ("taiwan", "TW"),
    ("hong kong", "HK"),
    ("singapore", "SG"),
    ("india", "IN"),
    ("turkey", "TR"),
    ("canada", "CA"),
    ("australia", "AU"),
    ("switzerland", "CH"),
    ("austria", "AT"),
    ("великобритания", "GB"),
    ("англия", "GB"),
    ("нидерланды", "NL"),
    ("голландия", "NL"),
    ("германия", "DE"),
    ("франция", "FR"),
    ("испания", "ES"),
    ("италия", "IT"),
    ("польша", "PL"),
    ("швеция", "SE"),
    ("финляндия", "FI"),
    ("норвегия", "NO"),
    ("дания", "DK"),
    ("ирландия", "IE"),
    ("бельгия", "BE"),
    ("португалия", "PT"),
    ("греция", "GR"),
    ("украина", "UA"),
    ("беларусь", "BY"),
    ("япония", "JP"),
    ("корея", "KR"),
    ("китай", "CN"),
    ("тайвань", "TW"),
    ("гонконг", "HK"),
    ("сингапур", "SG"),
    ("индия", "IN"),
    ("турция", "TR"),
    ("канада", "CA"),
    ("австралия", "AU"),
    ("швейцария", "CH"),
    ("австрия", "AT"),
    ("казахстан", "KZ"),
    ("россия", "RU"),
];

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_endpoint, metadata_for_config, HomeProfileMetadata};
    use crate::subscriptions::{EngineKind, ResolvedChildProfile};

    #[test]
    fn safe_metadata_contains_only_country_and_latency() {
        let metadata = metadata_for_config(
            &json!({
                "remarks": "DE synthetic profile",
                "outbounds": [{
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {"vnext": [{"address": "de.synthetic.invalid", "port": 443, "users": [{"id": "synthetic-uuid"}]}]}
                }]
            }),
            Some(("DE".into(), 47)),
        );
        assert_eq!(metadata.country_code.as_deref(), Some("DE"));
        assert_eq!(metadata.latency_ms, Some(47));
        assert_eq!(
            serde_json::to_string(&metadata).unwrap(),
            r#"{"country_code":"DE","latency_ms":47}"#
        );
    }

    #[test]
    fn supported_xray_outbound_shapes_extract_endpoints() {
        let fixtures = [
            json!({"protocol":"vless","settings":{"vnext":[{"address":"vless.synthetic.invalid","port":443}]}}),
            json!({"protocol":"vmess","settings":{"vnext":[{"address":"vmess.synthetic.invalid","port":8443}]}}),
            json!({"protocol":"trojan","settings":{"servers":[{"address":"trojan.synthetic.invalid","port":443}]}}),
            json!({"protocol":"shadowsocks","settings":{"servers":[{"address":"ss.synthetic.invalid","port":8388}]}}),
            json!({"outbounds":[{"protocol":"vless","settings":{"vnext":[{"address":"nested.synthetic.invalid","port":443}]}}]}),
        ];
        let endpoints = fixtures
            .iter()
            .map(|fixture| extract_endpoint(fixture).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(endpoints[0], ("vless.synthetic.invalid".into(), 443));
        assert_eq!(endpoints[1], ("vmess.synthetic.invalid".into(), 8443));
        assert_eq!(endpoints[2], ("trojan.synthetic.invalid".into(), 443));
        assert_eq!(endpoints[3], ("ss.synthetic.invalid".into(), 8388));
        assert_eq!(endpoints[4], ("nested.synthetic.invalid".into(), 443));
    }

    #[test]
    fn country_inference_accepts_flag_emoji_and_russian_names() {
        let emoji_metadata = metadata_for_config(
            &json!({
                "remarks": "🇩🇪 Германия — Reality",
                "outbounds": [{
                    "protocol": "vless",
                    "settings": {"vnext": [{"address": "de.synthetic.invalid", "port": 443}]}
                }]
            }),
            Some(("DE".into(), 47)),
        );
        let russian_metadata = metadata_for_config(
            &json!({
                "remarks": "Нидерланды 01",
                "outbounds": [{
                    "protocol": "vless",
                    "settings": {"vnext": [{"address": "nl.synthetic.invalid", "port": 443}]}
                }]
            }),
            Some(("NL".into(), 52)),
        );

        assert_eq!(emoji_metadata.country_code.as_deref(), Some("DE"));
        assert_eq!(russian_metadata.country_code.as_deref(), Some("NL"));
    }

    #[tokio::test]
    async fn country_code_survives_failed_latency_probe() {
        let metadata = super::resolve_profile_metadata(&ResolvedChildProfile {
            key: "profile-1".into(),
            name: "🇩🇪 Reality".into(),
            engine: EngineKind::Xray,
            config: json!({
                "outbounds": [{
                    "protocol": "vless",
                    "settings": {"vnext": [{"address": "127.0.0.1", "port": 1}]}
                }]
            }),
        })
        .await
        .expect("metadata resolution should be non-fatal");

        assert_eq!(metadata.country_code.as_deref(), Some("DE"));
        assert_eq!(metadata.latency_ms, None);
    }

    #[test]
    fn unsupported_or_missing_endpoint_returns_null_metadata() {
        let metadata = metadata_for_config(
            &json!({"remarks":"unknown", "outbounds":[{"protocol":"freedom"}]}),
            Some(("DE".into(), 47)),
        );
        assert_eq!(
            metadata,
            HomeProfileMetadata {
                country_code: None,
                latency_ms: None
            }
        );
    }

    #[test]
    fn serialized_metadata_cannot_contain_host_port_url_uuid_or_raw_config() {
        let metadata = metadata_for_config(
            &json!({
                "remarks": "US synthetic",
                "outbounds": [{"protocol":"vmess","settings":{"vnext":[{"address":"secret.synthetic.invalid","port":443,"users":[{"id":"synthetic-secret-uuid"}]}]}}],
                "url": "https://secret.invalid/subscription"
            }),
            Some(("US".into(), 12)),
        );
        let serialized = serde_json::to_string(&metadata).unwrap();
        let debug = format!("{metadata:?}");
        for forbidden in [
            "secret.synthetic.invalid",
            "443",
            "https://secret.invalid",
            "synthetic-secret-uuid",
            "outbounds",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "serialized leaked {forbidden}"
            );
            assert!(!debug.contains(forbidden), "debug leaked {forbidden}");
        }
    }
}
