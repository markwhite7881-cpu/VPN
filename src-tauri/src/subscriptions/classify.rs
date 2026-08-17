use std::collections::HashMap;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::Value;
use uuid::Uuid;

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
    for line in lines {
        match parser::parse_link(&line) {
            Ok(outbound) => outbounds.push(outbound),
            Err(error) => failures.push(ParseFailure { line, error }),
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
    let mut duplicate_counts: HashMap<String, usize> = HashMap::new();
    values
        .into_iter()
        .enumerate()
        .map(|(index, config)| {
            let identity = child_identity(&config).unwrap_or_else(|| format!("index-{index}"));
            let duplicate_ordinal = duplicate_counts.entry(identity).or_default();
            let key = stable_child_key(&config, index, *duplicate_ordinal);
            *duplicate_ordinal += 1;
            let name = child_name(&config).unwrap_or_else(|| format!("Profile {}", index + 1));
            ClassifiedChild { key, name, config }
        })
        .collect()
}

pub fn stable_child_key(value: &Value, index: usize, duplicate_ordinal: usize) -> String {
    let Some(identity) = child_identity(value) else {
        return format!("index-{index}");
    };
    if duplicate_ordinal == 0 {
        identity
    } else {
        format!("{identity}-{duplicate_ordinal}")
    }
}

fn child_identity(value: &Value) -> Option<String> {
    child_name(value)
        .as_deref()
        .and_then(safe_public_label)
        .and_then(normalize_key)
        .map(|name| format!("name-{name}"))
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

fn safe_public_label(value: &str) -> Option<&str> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let sensitive_word = [
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "bearer",
        "api-key",
        "apikey",
        "access-key",
        "access_key",
    ]
    .iter()
    .any(|word| lower.contains(word));
    let sensitive_delimiter = value
        .chars()
        .any(|character| matches!(character, '/' | '\\' | ':' | '@' | '=' | '?' | '#' | '&'));
    let suspicious_compact_length = !value.chars().any(char::is_whitespace) && value.len() > 32;

    if value.is_empty()
        || value.len() > 64
        || Uuid::parse_str(value).is_ok()
        || sensitive_word
        || sensitive_delimiter
        || suspicious_compact_length
    {
        return None;
    }
    Some(value)
}

fn normalize_key(value: &str) -> Option<String> {
    let mut normalized = String::new();
    let mut pending_separator = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(character);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
        if normalized.len() >= 64 {
            break;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    (!normalized.is_empty()).then_some(normalized)
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
    fn stable_key_ignores_generic_provider_ids_and_keeps_safe_public_label() {
        let first = json!({
            "id": "subscription-token-abc123",
            "profile_id": "credential-secret-one",
            "remarks": "Primary Node",
            "outbounds": [{"type": "vless", "uuid": "secret-one"}]
        });
        let rotated = json!({
            "id": "subscription-token-def456",
            "profile_id": "credential-secret-two",
            "remarks": "Primary Node",
            "outbounds": [{"type": "vless", "uuid": "secret-two"}]
        });
        assert_eq!(stable_child_key(&first, 0, 0), "name-primary-node");
        assert_eq!(
            stable_child_key(&first, 0, 0),
            stable_child_key(&rotated, 0, 0)
        );
    }

    #[test]
    fn stable_key_rejects_secret_like_public_labels() {
        let cases = [
            "11111111-1111-4111-8111-111111111111",
            "https://user:password@example.test/sub",
            "access-token-abc123",
            "user@example.test",
            "name=value",
        ];
        for label in cases {
            let value = json!({"remarks": label});
            assert_eq!(stable_child_key(&value, 3, 0), "index-3");
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
    fn stable_key_uses_normalized_name_and_duplicate_ordinal() {
        let value = json!({"remarks": "  Primary   Node  "});
        assert_eq!(stable_child_key(&value, 4, 0), "name-primary-node");
        assert_eq!(stable_child_key(&value, 4, 2), "name-primary-node-2");
    }

    #[test]
    fn stable_key_falls_back_to_array_position() {
        assert_eq!(stable_child_key(&json!({}), 3, 0), "index-3");
    }
}
