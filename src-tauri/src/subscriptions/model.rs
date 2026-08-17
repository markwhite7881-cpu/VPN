use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKind {
    #[default]
    Auto,
    LinkList,
    SingboxBundle,
    XrayBundle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Singbox,
    Xray,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionErrorKind {
    #[default]
    Subscription,
    SubscriptionAuth,
    SubscriptionExpired,
    DeviceLimit,
    PayloadTooLarge,
    UnsafeRedirect,
    AmbiguousConfig,
    Validation,
    EngineUnavailable,
    UnsafeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionRecord {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub kind: SubscriptionKind,
    #[serde(default)]
    pub engine: Option<EngineKind>,
    pub interval_minutes: u32,
    #[serde(default)]
    pub active_child_key: Option<String>,
    #[serde(default)]
    pub children: Vec<ChildProfileRecord>,
    #[serde(default)]
    pub metadata: ProviderMetadata,
    #[serde(default)]
    pub last_success_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_http_status: Option<u16>,
    #[serde(default)]
    pub last_error: Option<SubscriptionFailure>,
}

impl SubscriptionRecord {
    pub fn to_summary(&self) -> SubscriptionSummary {
        SubscriptionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            engine: self.engine,
            interval_minutes: self.interval_minutes,
            active_child_key: self.active_child_key.clone(),
            children: self
                .children
                .iter()
                .map(ChildProfileRecord::to_summary)
                .collect(),
            metadata: self.metadata.clone(),
            last_success_at: self.last_success_at,
            last_http_status: self.last_http_status,
            last_error: self.last_error.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildProfileRecord {
    pub key: String,
    pub name: String,
    pub engine: EngineKind,
    pub config: Value,
}

impl ChildProfileRecord {
    fn to_summary(&self) -> ChildProfileSummary {
        ChildProfileSummary {
            key: self.key.clone(),
            name: self.name.clone(),
            engine: self.engine,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProviderMetadata {
    #[serde(default)]
    pub upload_bytes: Option<u64>,
    #[serde(default)]
    pub download_bytes: Option<u64>,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionFailure {
    pub kind: SubscriptionErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionSummary {
    pub id: String,
    pub name: String,
    pub kind: SubscriptionKind,
    pub engine: Option<EngineKind>,
    pub interval_minutes: u32,
    pub active_child_key: Option<String>,
    pub children: Vec<ChildProfileSummary>,
    pub metadata: ProviderMetadata,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_http_status: Option<u16>,
    pub last_error: Option<SubscriptionFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildProfileSummary {
    pub key: String,
    pub name: String,
    pub engine: EngineKind,
}

#[cfg(test)]
mod tests {
    use super::{
        ChildProfileRecord, EngineKind, ProviderMetadata, SubscriptionKind, SubscriptionRecord,
    };
    use serde_json::{json, Value};

    fn sample_record() -> SubscriptionRecord {
        SubscriptionRecord {
            id: "sub-1".into(),
            name: "Private provider".into(),
            url: "https://token@example.test/sub/secret".into(),
            kind: SubscriptionKind::SingboxBundle,
            engine: Some(EngineKind::Singbox),
            interval_minutes: 60,
            active_child_key: Some("primary".into()),
            children: vec![ChildProfileRecord {
                key: "primary".into(),
                name: "Primary".into(),
                engine: EngineKind::Singbox,
                config: json!({"outbounds": [{"server": "secret.example.test"}]}),
            }],
            metadata: ProviderMetadata::default(),
            last_success_at: None,
            last_http_status: Some(200),
            last_error: None,
        }
    }

    #[test]
    fn old_record_without_kind_defaults_to_auto() {
        let record: SubscriptionRecord = serde_json::from_value(json!({
            "id": "legacy-1",
            "name": "Legacy",
            "url": "https://example.test/sub",
            "interval_minutes": 60
        }))
        .unwrap();
        assert_eq!(record.kind, SubscriptionKind::Auto);
        assert_eq!(record.engine, None);
    }

    #[test]
    fn summary_never_serializes_secret_url_or_bundle() {
        let value = serde_json::to_value(sample_record().to_summary()).unwrap();
        assert!(value.get("url").is_none());
        assert!(value.get("bundle").is_none());
        assert!(!contains_key(&value, "config"));
        assert!(!value.to_string().contains("secret.example.test"));
        assert!(!value.to_string().contains("token@example.test"));
    }

    fn contains_key(value: &Value, key: &str) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key(key) || map.values().any(|value| contains_key(value, key))
            }
            Value::Array(values) => values.iter().any(|value| contains_key(value, key)),
            _ => false,
        }
    }
}
