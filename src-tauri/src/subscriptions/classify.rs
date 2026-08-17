use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::Value;

use crate::commands::{ParseFailure, ParseLinksResult};
use crate::error::{AppError, AppResult};
use crate::parser;

#[derive(Clone)]
pub struct ClassifiedChild {
    pub key: String,
    pub name: String,
    pub config: Value,
}

#[derive(Clone)]
pub enum ClassifiedPayload {
    LinkList(ParseLinksResult),
    SingboxBundle(Vec<ClassifiedChild>),
    XrayBundle(Vec<ClassifiedChild>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectedEngine {
    Singbox,
    Xray,
}

pub fn classify_payload(bytes: &[u8], content_type: Option<&str>) -> AppResult<ClassifiedPayload> {
    let first = bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace());
    let declares_json = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/json")
                || value.to_ascii_lowercase().ends_with("+json")
        });

    if declares_json || matches!(first, Some(b'{') | Some(b'[')) {
        return classify_json(bytes);
    }

    classify_links(bytes)
}

fn classify_json(bytes: &[u8]) -> AppResult<ClassifiedPayload> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::AmbiguousConfig("provider JSON is malformed".into()))?;
    let values = match value {
        Value::Object(_) => vec![value],
        Value::Array(values) if !values.is_empty() => values,
        Value::Array(_) => {
            return Err(AppError::AmbiguousConfig(
                "provider JSON array is empty".into(),
            ))
        }
        _ => {
            return Err(AppError::AmbiguousConfig(
                "provider JSON is not a config object or array".into(),
            ))
        }
    };

    let mut engine = None;
    for value in &values {
        if !value.is_object() {
            return Err(AppError::AmbiguousConfig(
                "provider JSON array contains a non-object".into(),
            ));
        }
        let detected = detect_engine(value)?;
        if engine.is_some_and(|current| current != detected) {
            return Err(AppError::AmbiguousConfig(
                "provider JSON mixes engine configuration types".into(),
            ));
        }
        engine = Some(detected);
    }

    let children = classified_children(values);
    match engine.expect("non-empty config values always produce an engine") {
        DetectedEngine::Singbox => Ok(ClassifiedPayload::SingboxBundle(children)),
        DetectedEngine::Xray => Ok(ClassifiedPayload::XrayBundle(children)),
    }
}

fn classify_links(bytes: &[u8]) -> AppResult<ClassifiedPayload> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::Subscription("subscription link payload is not UTF-8".into()))?;
    let lines = split_link_lines(text)?;
    let mut outbounds = Vec::new();
    let mut failures = Vec::new();
    for (index, line) in lines.into_iter().enumerate() {
        match parser::parse_link(&line) {
            Ok(outbound) => outbounds.push(outbound),
            Err(_) => failures.push(ParseFailure {
                line: format!("item-{index}"),
                error: parser::ParseError::InvalidValue(
                    "subscription item".into(),
                    "invalid".into(),
                ),
            }),
        }
    }
    if outbounds.is_empty() {
        return Err(AppError::Subscription(
            "subscription link payload has no usable links".into(),
        ));
    }
    Ok(ClassifiedPayload::LinkList(ParseLinksResult {
        outbounds,
        failures,
    }))
}

fn split_link_lines(text: &str) -> AppResult<Vec<String>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let direct = nonempty_link_lines(text);
    if direct.iter().any(|line| line.contains("://")) {
        return Ok(direct);
    }

    let cleaned: String = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let padded = || {
        let mut value = cleaned.clone();
        while value.len() % 4 != 0 {
            value.push('=');
        }
        value
    };
    let decoded = URL_SAFE_NO_PAD
        .decode(&cleaned)
        .or_else(|_| URL_SAFE_NO_PAD.decode(padded()))
        .or_else(|_| STANDARD.decode(&cleaned))
        .or_else(|_| STANDARD.decode(padded()))
        .map_err(|_| AppError::Subscription("subscription link payload is invalid".into()))?;
    let decoded = std::str::from_utf8(&decoded).map_err(|_| {
        AppError::Subscription("decoded subscription link payload is not UTF-8".into())
    })?;
    Ok(nonempty_link_lines(decoded))
}

fn nonempty_link_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn detect_engine(value: &Value) -> AppResult<DetectedEngine> {
    let singbox = has_outbound_key(value, "type") || value.get("route").is_some();
    let xray = has_outbound_key(value, "protocol")
        || pointer_exists(value, "/routing/domainStrategy")
        || pointer_exists(value, "/routing/balancers")
        || value.get("observatory").is_some();

    match (singbox, xray) {
        (true, false) => Ok(DetectedEngine::Singbox),
        (false, true) => Ok(DetectedEngine::Xray),
        (true, true) => Err(AppError::AmbiguousConfig(
            "provider config contains markers for multiple engines".into(),
        )),
        (false, false) => Err(AppError::AmbiguousConfig(
            "provider config has no recognized engine markers".into(),
        )),
    }
}

fn has_outbound_key(value: &Value, key: &str) -> bool {
    value
        .get("outbounds")
        .and_then(Value::as_array)
        .is_some_and(|outbounds| {
            outbounds.iter().any(|outbound| {
                outbound
                    .as_object()
                    .is_some_and(|object| object.contains_key(key))
            })
        })
}

fn pointer_exists(value: &Value, pointer: &str) -> bool {
    value.pointer(pointer).is_some()
}

fn classified_children(values: Vec<Value>) -> Vec<ClassifiedChild> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, config)| {
            let key = stable_child_key(&config, index, 0);
            let name = child_name(&config).unwrap_or_else(|| format!("Profile {}", index + 1));
            ClassifiedChild { key, name, config }
        })
        .collect()
}

pub fn stable_child_key(_value: &Value, index: usize, duplicate_ordinal: usize) -> String {
    if duplicate_ordinal == 0 {
        format!("index-{index}")
    } else {
        format!("index-{index}-{duplicate_ordinal}")
    }
}

fn child_name(value: &Value) -> Option<String> {
    ["remarks", "name"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(normalize_whitespace)
        .filter(|value| !value.is_empty())
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{classify_payload, stable_child_key, ClassifiedPayload};
    use crate::error::AppError;
    use serde_json::json;

    const LINK: &str =
        "vless://11111111-1111-4111-8111-111111111111@example.com:443?security=tls#Demo";

    #[test]
    fn classifies_plain_links() {
        let classified = classify_payload(LINK.as_bytes(), Some("text/plain")).unwrap();
        assert!(matches!(classified, ClassifiedPayload::LinkList(_)));
    }

    #[test]
    fn classifies_base64_links() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(LINK);
        let classified = classify_payload(encoded.as_bytes(), Some("text/plain")).unwrap();
        assert!(matches!(classified, ClassifiedPayload::LinkList(_)));
    }

    #[test]
    fn classifies_singbox_object_from_outbound_type() {
        let payload = br#"{"outbounds":[{"type":"direct"}]}"#;
        let classified = classify_payload(payload, Some("application/json")).unwrap();
        assert!(matches!(classified, ClassifiedPayload::SingboxBundle(_)));
    }

    #[test]
    fn classifies_xray_array_from_protocol_and_routing_markers() {
        let payload = br#"[{"remarks":"Auto","outbounds":[{"protocol":"vless"}],"routing":{"balancers":[]},"observatory":{}}]"#;
        let classified = classify_payload(payload, Some("application/json")).unwrap();
        assert!(matches!(classified, ClassifiedPayload::XrayBundle(_)));
    }

    #[test]
    fn rejects_mixed_engine_array() {
        let payload =
            br#"[{"outbounds":[{"type":"direct"}]},{"outbounds":[{"protocol":"freedom"}]}]"#;
        assert!(matches!(
            classify_payload(payload, Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_object_with_both_engine_marker_sets() {
        let payload = br#"{"outbounds":[{"type":"direct","protocol":"freedom"}]}"#;
        assert!(matches!(
            classify_payload(payload, Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_malformed_declared_json() {
        assert!(matches!(
            classify_payload(br#"{"outbounds": ["#, Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_empty_json_array() {
        assert!(matches!(
            classify_payload(b"[]", Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_scalar_json() {
        assert!(matches!(
            classify_payload(b"42", Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_json_without_engine_markers() {
        assert!(matches!(
            classify_payload(br#"{"name":"unknown"}"#, Some("application/json")),
            Err(AppError::AmbiguousConfig(_))
        ));
    }

    #[test]
    fn rejects_non_utf8_link_payload_without_lossy_conversion() {
        assert!(matches!(
            classify_payload(&[0xff, 0xfe], Some("text/plain")),
            Err(AppError::Subscription(_))
        ));
    }

    #[test]
    fn stable_key_never_uses_provider_controlled_identity_fields() {
        let labels = [
            "192.0.2.10",
            "node.provider.example",
            "dG9rZW4tY3JlZGVudGlhbC0xMjM0NTY",
            "11111111-1111-4111-8111-111111111111",
            "https://user:password@example.test/sub",
            "user:password@192.0.2.1:1080",
            "Primary Node",
        ];
        for label in labels {
            let value = json!({
                "id": label,
                "profile_id": label,
                "remarks": label,
                "name": label
            });
            let key = stable_child_key(&value, 3, 0);
            assert_eq!(key, "index-3");
            assert!(!key.contains(label));
        }
    }

    #[test]
    fn classifies_plain_links_with_partial_failures() {
        let payload = format!("{LINK}\nnot-a-supported-link");
        let ClassifiedPayload::LinkList(result) =
            classify_payload(payload.as_bytes(), Some("text/plain")).unwrap()
        else {
            panic!("expected link list");
        };
        assert_eq!(result.outbounds.len(), 1);
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn classifies_base64_links_with_partial_failures() {
        use base64::Engine;
        let decoded = format!("{LINK}\nnot-a-supported-link");
        let encoded = base64::engine::general_purpose::STANDARD.encode(decoded);
        let ClassifiedPayload::LinkList(result) =
            classify_payload(encoded.as_bytes(), Some("text/plain")).unwrap()
        else {
            panic!("expected link list");
        };
        assert_eq!(result.outbounds.len(), 1);
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn classified_failures_serialize_and_debug_without_raw_link_material() {
        let uuid = "22222222-2222-4222-8222-222222222222";
        let failed_url = format!(
            "socks://synthetic-user:synthetic-pass@192.0.2.1:1080/{uuid}?marker=raw-marker"
        );
        let payload = format!("{LINK}\n{failed_url}\nopaque-secret-raw-marker");
        let ClassifiedPayload::LinkList(result) =
            classify_payload(payload.as_bytes(), Some("text/plain")).unwrap()
        else {
            panic!("expected link list");
        };

        assert_eq!(result.outbounds.len(), 1);
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].line, "item-1");
        assert_eq!(result.failures[1].line, "item-2");

        let serialized = serde_json::to_string(&result.failures).unwrap();
        let debugged = format!("{:?}", result.failures);
        for exposed in [
            "socks",
            "synthetic-user",
            "synthetic-pass",
            "192.0.2.1",
            uuid,
            "raw-marker",
            "opaque-secret",
            &failed_url,
        ] {
            assert!(!serialized.contains(exposed));
            assert!(!debugged.contains(exposed));
        }
    }

    #[test]
    fn stable_key_uses_position_and_duplicate_ordinal() {
        let value = json!({"remarks": "Primary Node"});
        assert_eq!(stable_child_key(&value, 4, 0), "index-4");
        assert_eq!(stable_child_key(&value, 4, 2), "index-4-2");
    }

    #[test]
    fn stable_key_falls_back_to_array_position() {
        assert_eq!(stable_child_key(&json!({}), 3, 0), "index-3");
    }
}
