use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

use crate::commands::ParseLinksResult;
use crate::error::{AppError, AppResult};

use super::model::ChildProfileRecord;
use super::{
    classify_payload, ClassifiedChild, ClassifiedPayload, EngineKind, HwidStore, ProviderMetadata,
    SubscriptionHttpClient, SubscriptionKind, SubscriptionLinkSummary, SubscriptionOutbounds,
    SubscriptionRecord, SubscriptionSnapshot, SubscriptionStore, SubscriptionSummary,
};

#[derive(Debug, Clone, Deserialize)]
pub struct AddSubscriptionInput {
    pub name: String,
    pub url: String,
    #[serde(rename = "intervalMinutes")]
    pub interval_minutes: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacySubscriptionInput {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(rename = "intervalMinutes")]
    pub interval_minutes: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedChildProfile {
    pub key: String,
    pub name: String,
    pub engine: EngineKind,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RefreshSubscriptionResult {
    pub subscription: SubscriptionSummary,
    pub selection_changed: bool,
}

#[derive(Clone)]
pub struct SubscriptionService {
    store: SubscriptionStore,
    hwid: HwidStore,
    http: SubscriptionHttpClient,
    version: String,
    lock: Arc<Mutex<()>>,
}

impl SubscriptionService {
    pub fn new(
        store: SubscriptionStore,
        hwid: HwidStore,
        http: SubscriptionHttpClient,
        version: String,
    ) -> Self {
        Self {
            store,
            hwid,
            http,
            version,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn list(&self) -> AppResult<SubscriptionSnapshot> {
        let _guard = self.lock.lock().await;
        let records = self.store.load_all()?;
        Ok(snapshot(records))
    }

    pub async fn resolve_link_refs(
        &self,
        refs: &[super::SubscriptionLinkRef],
    ) -> AppResult<Vec<crate::parser::Outbound>> {
        let _guard = self.lock.lock().await;
        let records = self.store.load_all()?;
        resolve_link_refs_from_records(&records, refs)
    }

    pub async fn resolve_child_profile(
        &self,
        subscription_id: &str,
        child_key: &str,
    ) -> AppResult<ResolvedChildProfile> {
        let _guard = self.lock.lock().await;
        let records = self.store.load_all()?;
        let record = records
            .iter()
            .find(|record| record.id == subscription_id)
            .ok_or_else(not_found)?;
        let child = record
            .children
            .iter()
            .find(|child| child.key == child_key)
            .ok_or_else(not_found)?;
        Ok(ResolvedChildProfile {
            key: child.key.clone(),
            name: child.name.clone(),
            engine: child.engine,
            config: child.config.clone(),
        })
    }

    pub async fn resolve_all_links(&self) -> AppResult<Vec<crate::parser::Outbound>> {
        let _guard = self.lock.lock().await;
        let records = self.store.load_all()?;
        let refs = records
            .iter()
            .filter(|record| record.kind == SubscriptionKind::LinkList)
            .flat_map(|record| {
                record
                    .link_outbounds
                    .iter()
                    .enumerate()
                    .map(move |(index, _)| super::SubscriptionLinkRef {
                        subscription_id: record.id.clone(),
                        link_key: format!("index-{index}"),
                    })
            })
            .collect::<Vec<_>>();
        resolve_link_refs_from_records(&records, &refs)
    }

    pub async fn add(&self, input: AddSubscriptionInput) -> AppResult<RefreshSubscriptionResult> {
        validate_input(&input.name, &input.url, input.interval_minutes)?;
        let (record, records) = {
            let _guard = self.lock.lock().await;
            let records = self.store.load_all()?;
            if records.iter().any(|record| {
                normalized_url_digest(&record.url) == normalized_url_digest(&input.url)
            }) {
                return Err(AppError::Subscription("subscription already exists".into()));
            }
            let record = SubscriptionRecord {
                id: Uuid::new_v4().to_string(),
                name: input.name.trim().to_owned(),
                url: input.url.trim().to_owned(),
                kind: SubscriptionKind::Auto,
                engine: None,
                interval_minutes: input.interval_minutes,
                active_child_key: None,
                children: Vec::new(),
                link_outbounds: Vec::new(),
                bundle_digest: None,
                metadata: ProviderMetadata::default(),
                last_success_at: None,
                last_http_status: None,
                last_error: None,
            };
            (record, records)
        };
        let candidate = self.prepare_candidate(&record).await?;
        let _guard = self.lock.lock().await;
        let current = self.store.load_all()?;
        if current != records {
            return Err(concurrent_change());
        }
        let result = RefreshSubscriptionResult {
            subscription: candidate.to_summary(),
            selection_changed: false,
        };
        let mut next = records;
        next.push(candidate);
        self.store.replace_all(&next)?;
        Ok(result)
    }

    pub async fn remove(&self, id: &str) -> AppResult<()> {
        let _guard = self.lock.lock().await;
        let mut records = self.store.load_all()?;
        let count = records.len();
        records.retain(|record| record.id != id);
        if records.len() == count {
            return Err(not_found());
        }
        self.store.replace_all(&records)
    }

    pub async fn set_interval(
        &self,
        id: &str,
        interval_minutes: u32,
    ) -> AppResult<SubscriptionSummary> {
        let _guard = self.lock.lock().await;
        validate_interval(interval_minutes)?;
        let mut records = self.store.load_all()?;
        let record = record_mut(&mut records, id)?;
        record.interval_minutes = interval_minutes;
        let summary = record.to_summary();
        self.store.replace_all(&records)?;
        Ok(summary)
    }

    pub async fn select_child(&self, id: &str, child_key: &str) -> AppResult<SubscriptionSummary> {
        let _guard = self.lock.lock().await;
        let mut records = self.store.load_all()?;
        let record = record_mut(&mut records, id)?;
        if !record.children.iter().any(|child| child.key == child_key) {
            return Err(AppError::Validation(
                "subscription child was not found".into(),
            ));
        }
        record.active_child_key = Some(child_key.to_owned());
        let summary = record.to_summary();
        self.store.replace_all(&records)?;
        Ok(summary)
    }

    pub async fn refresh(&self, id: &str) -> AppResult<RefreshSubscriptionResult> {
        let original = {
            let _guard = self.lock.lock().await;
            let records = self.store.load_all()?;
            records
                .iter()
                .find(|record| record.id == id)
                .cloned()
                .ok_or_else(not_found)?
        };
        let candidate = self.prepare_candidate(&original).await?;
        let _guard = self.lock.lock().await;
        let mut records = self.store.load_all()?;
        let index = records
            .iter()
            .position(|record| record.id == id)
            .ok_or_else(not_found)?;
        if records[index] != original {
            return Err(concurrent_change());
        }
        let selection_changed = candidate.active_child_key != original.active_child_key;
        let result = RefreshSubscriptionResult {
            subscription: candidate.to_summary(),
            selection_changed,
        };
        records[index] = candidate;
        self.store.replace_all(&records)?;
        Ok(result)
    }

    pub async fn migrate_legacy(
        &self,
        inputs: Vec<LegacySubscriptionInput>,
    ) -> AppResult<SubscriptionSnapshot> {
        let (mut records, original) = {
            let _guard = self.lock.lock().await;
            let records = self.store.load_all()?;
            (records.clone(), records)
        };
        for input in inputs {
            validate_input(&input.name, &input.url, input.interval_minutes)?;
            if records.iter().any(|record| record.id == input.id)
                || records.iter().any(|record| {
                    normalized_url_digest(&record.url) == normalized_url_digest(&input.url)
                })
            {
                continue;
            }
            let record = SubscriptionRecord {
                id: input.id,
                name: input.name.trim().to_owned(),
                url: input.url.trim().to_owned(),
                kind: SubscriptionKind::Auto,
                engine: None,
                interval_minutes: input.interval_minutes,
                active_child_key: None,
                children: Vec::new(),
                link_outbounds: Vec::new(),
                bundle_digest: None,
                metadata: ProviderMetadata::default(),
                last_success_at: None,
                last_http_status: None,
                last_error: None,
            };
            records.push(self.prepare_candidate(&record).await?);
        }
        let _guard = self.lock.lock().await;
        if self.store.load_all()? != original {
            return Err(concurrent_change());
        }
        self.store.replace_all(&records)?;
        Ok(snapshot(records))
    }

    pub async fn get_hwid(&self) -> AppResult<String> {
        let _guard = self.lock.lock().await;
        Ok(self.hwid.get_or_create()?.to_string())
    }

    pub async fn reset_hwid(&self) -> AppResult<String> {
        let _guard = self.lock.lock().await;
        Ok(self.hwid.reset()?.to_string())
    }

    pub async fn fetch_legacy_links(&self, url: &str) -> AppResult<ParseLinksResult> {
        let url = parse_url(url)?;
        let payload = self
            .http
            .fetch(&url, self.hwid.get_or_create()?, &self.version, "Windows")
            .await?;
        match classify_payload(&payload.bytes, payload.content_type.as_deref())? {
            ClassifiedPayload::LinkList(result) => Ok(result),
            ClassifiedPayload::SingboxBundle(_) | ClassifiedPayload::XrayBundle(_) => {
                Err(AppError::Unsupported(
                    "full configuration subscriptions require backend storage".into(),
                ))
            }
        }
    }

    async fn prepare_candidate(
        &self,
        current: &SubscriptionRecord,
    ) -> AppResult<SubscriptionRecord> {
        let url = parse_url(&current.url)?;
        let payload = self
            .http
            .fetch(&url, self.hwid.get_or_create()?, &self.version, "Windows")
            .await?;
        let mut candidate = current.clone();
        let previous_profile_title = candidate.metadata.profile_title.clone();
        candidate.metadata = payload.metadata;
        apply_provider_title_fallback(&mut candidate, previous_profile_title.as_deref());
        candidate.last_http_status = Some(payload.status);
        candidate.last_success_at = Some(Utc::now());
        candidate.last_error = None;

        match classify_payload(&payload.bytes, payload.content_type.as_deref())? {
            ClassifiedPayload::LinkList(result) => {
                candidate.kind = SubscriptionKind::LinkList;
                candidate.engine = Some(EngineKind::Singbox);
                candidate.children.clear();
                candidate.active_child_key = None;
                candidate.link_outbounds = result.outbounds;
                candidate.bundle_digest = None;
            }
            ClassifiedPayload::SingboxBundle(children) => {
                apply_bundle(
                    &mut candidate,
                    SubscriptionKind::SingboxBundle,
                    EngineKind::Singbox,
                    children,
                )?;
            }
            ClassifiedPayload::XrayBundle(children) => {
                apply_bundle(
                    &mut candidate,
                    SubscriptionKind::XrayBundle,
                    EngineKind::Xray,
                    children,
                )?;
            }
        }
        validate_candidate(&candidate)?;
        Ok(candidate)
    }
}

fn apply_provider_title_fallback(
    record: &mut SubscriptionRecord,
    previous_profile_title: Option<&str>,
) {
    let Some(title) = record
        .metadata
        .profile_title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    else {
        return;
    };
    let Some(resolved_title) = decode_provider_title(title) else {
        return;
    };

    if record.name.trim() == "Subscription"
        || previous_profile_title
            .map(str::trim)
            .is_some_and(|previous| record.name == previous && previous == title)
    {
        record.name = resolved_title;
    }
}

fn decode_provider_title(title: &str) -> Option<String> {
    let Some(encoded) = title.strip_prefix("base64:") else {
        return Some(title.to_owned());
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    String::from_utf8(bytes)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn apply_bundle(
    record: &mut SubscriptionRecord,
    kind: SubscriptionKind,
    engine: EngineKind,
    children: Vec<ClassifiedChild>,
) -> AppResult<()> {
    let children = children
        .into_iter()
        .map(|child| {
            Ok(ChildProfileRecord {
                key: child.key,
                name: child.name,
                digest: value_digest(&child.config)?,
                engine,
                config: child.config,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    record.kind = kind;
    record.engine = Some(engine);
    record.bundle_digest = Some(bundle_digest(&children));
    record.link_outbounds.clear();
    record.active_child_key = stable_selection(record.active_child_key.as_deref(), &children);
    record.children = children;
    Ok(())
}

fn validate_candidate(record: &SubscriptionRecord) -> AppResult<()> {
    if matches!(
        record.kind,
        SubscriptionKind::SingboxBundle | SubscriptionKind::XrayBundle
    ) && (record.children.is_empty()
        || record
            .children
            .iter()
            .any(|child| !child.config.is_object() || child.digest.is_empty()))
    {
        return Err(AppError::Validation(
            "subscription bundle has an invalid child".into(),
        ));
    }
    Ok(())
}

fn stable_selection(previous: Option<&str>, children: &[ChildProfileRecord]) -> Option<String> {
    previous
        .filter(|key| children.iter().any(|child| child.key == *key))
        .map(str::to_owned)
        .or_else(|| children.first().map(|child| child.key.clone()))
}

fn snapshot(records: Vec<SubscriptionRecord>) -> SubscriptionSnapshot {
    let link_outbounds = records
        .iter()
        .filter(|record| record.kind == SubscriptionKind::LinkList)
        .map(|record| SubscriptionOutbounds {
            subscription_id: record.id.clone(),
            links: record
                .link_outbounds
                .iter()
                .enumerate()
                .map(|(index, outbound)| {
                    let protocol = outbound.protocol().to_owned();
                    SubscriptionLinkSummary {
                        key: format!("index-{index}"),
                        label: format!("{protocol} link {}", index + 1),
                        protocol,
                    }
                })
                .collect(),
        })
        .collect();
    let subscriptions = records.iter().map(SubscriptionRecord::to_summary).collect();
    SubscriptionSnapshot {
        subscriptions,
        link_outbounds,
    }
}

fn resolve_link_refs_from_records(
    records: &[SubscriptionRecord],
    refs: &[super::SubscriptionLinkRef],
) -> AppResult<Vec<crate::parser::Outbound>> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut outbounds = Vec::with_capacity(refs.len());
    for reference in refs {
        if !seen.insert((
            reference.subscription_id.as_str(),
            reference.link_key.as_str(),
        )) {
            return Err(AppError::Validation(
                "duplicate subscription link selection".into(),
            ));
        }
        let record = records
            .iter()
            .find(|record| record.id == reference.subscription_id)
            .ok_or_else(|| AppError::Subscription("subscription was not found".into()))?;
        if record.kind != SubscriptionKind::LinkList {
            return Err(AppError::Validation(
                "subscription does not contain links".into(),
            ));
        }
        let index = reference
            .link_key
            .strip_prefix("index-")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|index| reference.link_key == format!("index-{index}"))
            .ok_or_else(|| AppError::Validation("subscription link selection is invalid".into()))?;
        let outbound = record
            .link_outbounds
            .get(index)
            .ok_or_else(|| AppError::Validation("subscription link selection is stale".into()))?;
        outbounds.push(outbound.clone());
    }
    Ok(outbounds)
}
fn validate_input(name: &str, url: &str, interval_minutes: u32) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("subscription name is required".into()));
    }
    let _ = parse_url(url)?;
    validate_interval(interval_minutes)
}

fn validate_interval(interval_minutes: u32) -> AppResult<()> {
    if !(15..=43_200).contains(&interval_minutes) {
        return Err(AppError::Validation(
            "subscription interval is out of range".into(),
        ));
    }
    Ok(())
}

fn parse_url(value: &str) -> AppResult<Url> {
    Url::parse(value.trim())
        .map_err(|_| AppError::Subscription("subscription URL is invalid".into()))
}

fn normalized_url_digest(value: &str) -> String {
    let normalized = value.trim();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn value_digest(value: &serde_json::Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn bundle_digest(children: &[ChildProfileRecord]) -> String {
    let mut hasher = Sha256::new();
    for child in children {
        hasher.update(child.digest.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn record_mut<'a>(
    records: &'a mut [SubscriptionRecord],
    id: &str,
) -> AppResult<&'a mut SubscriptionRecord> {
    records
        .iter_mut()
        .find(|record| record.id == id)
        .ok_or_else(not_found)
}

fn concurrent_change() -> AppError {
    AppError::Subscription("subscription state changed during refresh".into())
}

fn not_found() -> AppError {
    AppError::Subscription("subscription was not found".into())
}
