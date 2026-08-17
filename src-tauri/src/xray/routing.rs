use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    config::RoutingOptions,
    error::{AppError, AppResult},
};

const BLOCK_TAG: &str = "cloakwire-block";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    ProcessMatcher,
    UnsupportedMatcher,
    UnsupportedAction,
    MissingOutboundTag,
    MissingBalancerTag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableRule {
    pub rule_id: String,
    pub label: String,
    pub reason: UnavailableReason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingApplicability {
    pub applied_rule_ids: Vec<String>,
    pub unavailable: Vec<UnavailableRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingPreparation {
    pub value: Value,
    pub applicability: RoutingApplicability,
}

pub fn merge_routing(
    mut provider: Value,
    routing: &RoutingOptions,
) -> AppResult<RoutingPreparation> {
    let root = provider
        .as_object_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray provider config must be an object".into()))?;
    let outbound_tags = provider_tags(root, "outbounds")?;
    let blackhole_tag = blackhole_tag(root)?;
    let inbound_tags = provider_tags(root, "inbounds")?;
    let balancer_tags = root
        .get("routing")
        .and_then(Value::as_object)
        .and_then(|routing| routing.get("balancers"))
        .map(tags_from_array)
        .transpose()?
        .unwrap_or_default();

    let mut translated = Vec::new();
    let mut applicability = RoutingApplicability::default();
    let mut needs_block = false;
    for rule in routing.rules.iter().filter(|rule| is_enabled(rule)) {
        let rule_id = rule_id(rule);
        let label = rule_label(rule);
        match translate_rule(
            rule,
            &outbound_tags,
            &balancer_tags,
            &inbound_tags,
            blackhole_tag.as_deref(),
        ) {
            Ok(TranslatedRule::Rule(rule)) => {
                applicability.applied_rule_ids.push(rule_id);
                translated.push(rule);
            }
            Ok(TranslatedRule::Reject(rule)) => {
                applicability.applied_rule_ids.push(rule_id);
                needs_block |= blackhole_tag.is_none();
                translated.push(rule);
            }
            Err(reason) => applicability.unavailable.push(UnavailableRule {
                rule_id,
                label,
                reason,
            }),
        }
    }

    let block_tag = blackhole_tag.unwrap_or_else(|| BLOCK_TAG.into());
    for rule in &mut translated {
        if rule.get("outboundTag") == Some(&Value::String(BLOCK_TAG.into())) {
            rule.as_object_mut()
                .expect("translated rule")
                .insert("outboundTag".into(), Value::String(block_tag.clone()));
        }
    }
    if needs_block {
        if outbound_tags.contains(BLOCK_TAG) {
            return Err(AppError::UnsafeConfig(
                "provider uses reserved Xray outbound tag".into(),
            ));
        }
        root.get_mut("outbounds")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| AppError::UnsafeConfig("Xray outbounds must be an array".into()))?
            .push(serde_json::json!({"tag": BLOCK_TAG, "protocol": "blackhole"}));
    }

    if translated.is_empty() {
        return Ok(RoutingPreparation {
            value: provider,
            applicability,
        });
    }
    let routing_value = root
        .entry("routing")
        .or_insert_with(|| Value::Object(Map::new()));
    let routing_object = routing_value
        .as_object_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray routing must be an object".into()))?;
    let provider_rules = routing_object
        .entry("rules")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray routing rules must be an array".into()))?;
    translated.append(provider_rules);
    *provider_rules = translated;

    Ok(RoutingPreparation {
        value: provider,
        applicability,
    })
}

enum TranslatedRule {
    Rule(Value),
    Reject(Value),
}

fn translate_rule(
    rule: &Value,
    outbound_tags: &HashSet<String>,
    balancer_tags: &HashSet<String>,
    inbound_tags: &HashSet<String>,
    blackhole_tag: Option<&str>,
) -> Result<TranslatedRule, UnavailableReason> {
    let object = rule
        .as_object()
        .ok_or(UnavailableReason::UnsupportedMatcher)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "id" | "label" | "enabled" | "matchers" | "action"
        )
    }) {
        return Err(UnavailableReason::UnsupportedMatcher);
    }
    let matchers = object
        .get("matchers")
        .and_then(Value::as_object)
        .ok_or(UnavailableReason::UnsupportedMatcher)?;
    let mut xray = Map::new();
    for (key, value) in matchers {
        match key.as_str() {
            "process_name" | "process_path" | "process_path_regex" => {
                return Err(UnavailableReason::ProcessMatcher)
            }
            "domain" => add_prefixed(&mut xray, "domain", value, "full:")?,
            "domain_suffix" => add_prefixed(&mut xray, "domain", value, "domain:")?,
            "domain_keyword" => add_prefixed(&mut xray, "domain", value, "keyword:")?,
            "ip_cidr" => add_ips(&mut xray, value)?,
            "port" => add_ports(&mut xray, value, false)?,
            "port_range" => add_ports(&mut xray, value, true)?,
            "network" => add_network(&mut xray, value)?,
            "protocol" => add_protocols(&mut xray, value)?,
            "inbound" => add_inbounds(&mut xray, value, inbound_tags)?,
            _ => return Err(UnavailableReason::UnsupportedMatcher),
        }
    }
    if xray.is_empty() {
        return Err(UnavailableReason::UnsupportedMatcher);
    }

    let action = object
        .get("action")
        .and_then(Value::as_object)
        .ok_or(UnavailableReason::UnsupportedAction)?;
    let kind = action
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(UnavailableReason::UnsupportedAction)?;
    match kind {
        "reject" if action.len() == 1 => {
            xray.insert(
                "outboundTag".into(),
                Value::String(blackhole_tag.unwrap_or(BLOCK_TAG).into()),
            );
            Ok(TranslatedRule::Reject(Value::Object(xray)))
        }
        "route" => {
            if action.len() != 2 {
                return Err(UnavailableReason::UnsupportedAction);
            }
            let (field, tag) = action
                .iter()
                .find(|(key, _)| key.as_str() != "kind")
                .ok_or(UnavailableReason::UnsupportedAction)?;
            let tag = tag
                .as_str()
                .filter(|tag| valid_tag(tag))
                .ok_or(UnavailableReason::UnsupportedAction)?;
            match field.as_str() {
                "outbound" if outbound_tags.contains(tag) => {
                    xray.insert("outboundTag".into(), Value::String(tag.into()));
                }
                "outbound" if balancer_tags.contains(tag) => {
                    xray.insert("balancerTag".into(), Value::String(tag.into()));
                }
                "outbound" => return Err(UnavailableReason::MissingOutboundTag),
                "balancer" | "balancerTag" if balancer_tags.contains(tag) => {
                    xray.insert("balancerTag".into(), Value::String(tag.into()));
                }
                "balancer" | "balancerTag" => return Err(UnavailableReason::MissingBalancerTag),
                _ => return Err(UnavailableReason::UnsupportedAction),
            }
            Ok(TranslatedRule::Rule(Value::Object(xray)))
        }
        _ => Err(UnavailableReason::UnsupportedAction),
    }
}

fn provider_tags(root: &Map<String, Value>, key: &str) -> AppResult<HashSet<String>> {
    root.get(key)
        .map(tags_from_array)
        .transpose()
        .map(|tags| tags.unwrap_or_default())
}

fn tags_from_array(value: &Value) -> AppResult<HashSet<String>> {
    value
        .as_array()
        .ok_or_else(|| AppError::UnsafeConfig("Xray tagged section must be an array".into()))?
        .iter()
        .map(|item| {
            item.as_object()
                .and_then(|item| item.get("tag"))
                .and_then(Value::as_str)
                .filter(|tag| valid_tag(tag))
                .map(str::to_string)
                .ok_or_else(|| AppError::UnsafeConfig("Xray tagged item has an invalid tag".into()))
        })
        .collect()
}

fn blackhole_tag(root: &Map<String, Value>) -> AppResult<Option<String>> {
    let Some(outbounds) = root.get("outbounds") else {
        return Ok(None);
    };
    let outbounds = outbounds
        .as_array()
        .ok_or_else(|| AppError::UnsafeConfig("Xray outbounds must be an array".into()))?;
    Ok(outbounds.iter().find_map(|outbound| {
        let object = outbound.as_object()?;
        (object.get("protocol").and_then(Value::as_str) == Some("blackhole"))
            .then(|| {
                object
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten()
    }))
}

fn add_prefixed(
    output: &mut Map<String, Value>,
    key: &str,
    input: &Value,
    prefix: &str,
) -> Result<(), UnavailableReason> {
    let values = strings(input)?;
    let translated: Result<Vec<_>, _> = values
        .into_iter()
        .map(|value| {
            if valid_domain(&value)
                || (prefix == "full:"
                    && value.starts_with("geosite:")
                    && valid_reference(&value, "geosite:"))
            {
                Ok(Value::String(if value.starts_with("geosite:") {
                    value
                } else {
                    format!("{prefix}{value}")
                }))
            } else {
                Err(UnavailableReason::UnsupportedMatcher)
            }
        })
        .collect();
    append_values(output, key, translated?);
    Ok(())
}

fn add_ips(output: &mut Map<String, Value>, input: &Value) -> Result<(), UnavailableReason> {
    let values: Result<Vec<_>, _> = strings(input)?
        .into_iter()
        .map(|value| {
            if valid_cidr(&value)
                || (value.starts_with("geoip:") && valid_reference(&value, "geoip:"))
            {
                Ok(Value::String(value))
            } else {
                Err(UnavailableReason::UnsupportedMatcher)
            }
        })
        .collect();
    append_values(output, "ip", values?);
    Ok(())
}

fn add_ports(
    output: &mut Map<String, Value>,
    input: &Value,
    range: bool,
) -> Result<(), UnavailableReason> {
    let values = input
        .as_array()
        .ok_or(UnavailableReason::UnsupportedMatcher)?;
    let ports: Result<Vec<_>, _> = values
        .iter()
        .map(|value| {
            let value = if range {
                value.as_str().map(str::to_string)
            } else {
                value.as_u64().map(|port| port.to_string())
            }
            .ok_or(UnavailableReason::UnsupportedMatcher)?;
            if valid_port_or_range(&value, range) {
                Ok(value)
            } else {
                Err(UnavailableReason::UnsupportedMatcher)
            }
        })
        .collect();
    let ports = ports?;
    if ports.is_empty() {
        return Err(UnavailableReason::UnsupportedMatcher);
    }
    output.insert("port".into(), Value::String(ports.join(",")));
    Ok(())
}

fn add_network(output: &mut Map<String, Value>, input: &Value) -> Result<(), UnavailableReason> {
    let values = strings(input)?;
    if values
        .iter()
        .any(|value| !matches!(value.as_str(), "tcp" | "udp"))
    {
        return Err(UnavailableReason::UnsupportedMatcher);
    }
    output.insert("network".into(), Value::String(values.join(",")));
    Ok(())
}

fn add_protocols(output: &mut Map<String, Value>, input: &Value) -> Result<(), UnavailableReason> {
    let values = strings(input)?;
    if values.iter().any(|value| {
        !matches!(
            value.as_str(),
            "http" | "tls" | "bittorrent" | "quic" | "stun"
        )
    }) {
        return Err(UnavailableReason::UnsupportedMatcher);
    }
    output.insert(
        "protocol".into(),
        Value::Array(values.into_iter().map(Value::String).collect()),
    );
    Ok(())
}

fn add_inbounds(
    output: &mut Map<String, Value>,
    input: &Value,
    known: &HashSet<String>,
) -> Result<(), UnavailableReason> {
    let values = strings(input)?;
    if values
        .iter()
        .any(|value| !valid_tag(value) || !known.contains(value))
    {
        return Err(UnavailableReason::UnsupportedMatcher);
    }
    output.insert(
        "inboundTag".into(),
        Value::Array(values.into_iter().map(Value::String).collect()),
    );
    Ok(())
}

fn strings(value: &Value) -> Result<Vec<String>, UnavailableReason> {
    let values = value
        .as_array()
        .ok_or(UnavailableReason::UnsupportedMatcher)?
        .iter()
        .map(Value::as_str)
        .map(|value| value.map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .filter(|values| !values.is_empty())
        .ok_or(UnavailableReason::UnsupportedMatcher)?;
    Ok(values)
}
fn append_values(output: &mut Map<String, Value>, key: &str, values: Vec<Value>) {
    output
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("array")
        .extend(values);
}
fn valid_domain(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
}
fn valid_reference(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|value| !value.is_empty() && !value.chars().any(char::is_whitespace))
}
fn valid_cidr(value: &str) -> bool {
    let Some((ip, prefix)) = value.split_once('/') else {
        return false;
    };
    ip.parse::<std::net::IpAddr>().is_ok()
        && prefix
            .parse::<u8>()
            .is_ok_and(|prefix| prefix <= if ip.contains(':') { 128 } else { 32 })
}
fn valid_port_or_range(value: &str, range: bool) -> bool {
    let parts: Vec<_> = value
        .split(if value.contains(':') { ':' } else { '-' })
        .collect();
    if (!range && parts.len() != 1) || (range && parts.len() != 2) {
        return false;
    }
    let parsed: Option<Vec<u16>> = parts
        .into_iter()
        .map(|part| part.parse::<u16>().ok().filter(|port| *port > 0))
        .collect();
    parsed.is_some_and(|parts| !range || parts[0] <= parts[1])
}
fn valid_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}
fn is_enabled(rule: &Value) -> bool {
    rule.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}
fn rule_id(rule: &Value) -> String {
    rule.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("unknown")
        .into()
}
fn rule_label(rule: &Value) -> String {
    rule.get("label")
        .and_then(Value::as_str)
        .filter(|label| !label.is_empty())
        .unwrap_or("Unnamed rule")
        .into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_routing;
    use crate::config::RoutingOptions;

    #[test]
    fn prepends_exact_local_rules_before_provider_rules() {
        let provider = json!({"observatory":{"subjectSelector":["proxy"]},"routing":{"domainStrategy":"IPIfNonMatch","domainMatcher":"mph","balancers":[{"tag":"leastPing","selector":["proxy"]}],"rules":[{"type":"field","balancerTag":"leastPing"}]},"outbounds":[{"tag":"proxy","protocol":"vless"}]});
        let mut routing = RoutingOptions::default();
        routing.rules = vec![
            json!({"id":"domain-rule","label":"Example","enabled":true,"matchers":{"domain":["example.com"]},"action":{"kind":"route","outbound":"proxy"}}),
        ];
        let prepared = merge_routing(provider.clone(), &routing).unwrap();
        assert_eq!(
            prepared.value["routing"]["rules"][0]["domain"][0],
            "full:example.com"
        );
        assert_eq!(
            prepared.value["routing"]["rules"][1],
            provider["routing"]["rules"][0]
        );
        assert_eq!(
            prepared.value["routing"]["domainStrategy"],
            provider["routing"]["domainStrategy"]
        );
        assert_eq!(
            prepared.value["routing"]["domainMatcher"],
            provider["routing"]["domainMatcher"]
        );
        assert_eq!(
            prepared.value["routing"]["balancers"],
            provider["routing"]["balancers"]
        );
        assert_eq!(prepared.value["observatory"], provider["observatory"]);
        assert_eq!(prepared.applicability.applied_rule_ids, vec!["domain-rule"]);
    }

    #[test]
    fn routes_to_existing_provider_balancer_via_outbound_action_target() {
        let provider = json!({
            "routing": {"balancers": [{"tag": "leastPing", "selector": ["proxy"]}]},
            "outbounds": [{"tag": "proxy", "protocol": "vless"}]
        });
        let mut routing = RoutingOptions::default();
        routing.rules = vec![json!({
            "id": "balanced",
            "enabled": true,
            "matchers": {"domain": ["example.com"]},
            "action": {"kind": "route", "outbound": "leastPing"}
        })];

        let prepared = merge_routing(provider, &routing).unwrap();

        assert_eq!(
            prepared.value["routing"]["rules"][0]["balancerTag"],
            "leastPing"
        );
    }

    #[test]
    fn translates_only_supported_matchers_and_reject_action_uses_blackhole() {
        let provider = json!({"outbounds":[{"tag":"proxy","protocol":"vless"}]});
        let mut routing = RoutingOptions::default();
        routing.rules = vec![
            json!({"id":"supported","enabled":true,"matchers":{"domain_suffix":["example.com"],"ip_cidr":["10.0.0.0/8"],"port_range":["443:444"],"network":["tcp","udp"],"protocol":["http"]},"action":{"kind":"reject"}}),
            json!({"id":"unsupported","label":"Unsafe","enabled":true,"matchers":{"source_ip_cidr":["10.0.0.0/8"]},"action":{"kind":"route","outbound":"proxy"}}),
        ];
        let prepared = merge_routing(provider, &routing).unwrap();
        assert_eq!(
            prepared.value["routing"]["rules"][0]["domain"][0],
            "domain:example.com"
        );
        assert_eq!(prepared.value["routing"]["rules"][0]["ip"][0], "10.0.0.0/8");
        assert_eq!(prepared.value["routing"]["rules"][0]["port"], "443:444");
        assert_eq!(prepared.value["routing"]["rules"][0]["network"], "tcp,udp");
        assert_eq!(
            prepared.value["routing"]["rules"][0]["outboundTag"],
            "cloakwire-block"
        );
        assert_eq!(
            prepared.value["outbounds"][1],
            json!({"tag":"cloakwire-block","protocol":"blackhole"})
        );
        assert_eq!(prepared.applicability.unavailable[0].rule_id, "unsupported");
    }
}
