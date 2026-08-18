//! Runtime-safe preparation of raw Xray provider configurations.

pub mod inbound;
pub mod presentation;
pub mod routing;
pub mod stats;

use std::fmt;

use serde_json::Value;

use crate::{config::RoutingOptions, error::AppResult};

pub use routing::{RoutingApplicability, UnavailableReason, UnavailableRule};

#[derive(Clone, PartialEq)]
pub struct PreparedXrayConfig {
    pub value: Value,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub applicability: RoutingApplicability,
    pub(crate) stats: stats::XrayStatsSpec,
}

impl fmt::Debug for PreparedXrayConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedXrayConfig")
            .field("has_runtime_config", &true)
            .field("has_proxy", &true)
            .field(
                "applied_rule_count",
                &self.applicability.applied_rule_ids.len(),
            )
            .field(
                "unavailable_rule_count",
                &self.applicability.unavailable.len(),
            )
            .finish()
    }
}

/// Clone and prepare a provider config only for the running Xray process.
/// The caller's stored provider JSON is never mutated.
pub fn prepare_xray_runtime_config<F>(
    provider: Value,
    routing: &RoutingOptions,
    mut port_allocator: F,
) -> AppResult<PreparedXrayConfig>
where
    F: FnMut() -> AppResult<u16>,
{
    let inbound = inbound::ensure_managed_http_inbound(provider, &mut port_allocator)?;
    let routing = routing::merge_routing(inbound.value, routing)?;
    let (value, stats) =
        stats::merge_stats_config(routing.value, &inbound.traffic_tag, port_allocator)?;
    Ok(PreparedXrayConfig {
        value,
        proxy_host: inbound.proxy_host,
        proxy_port: inbound.proxy_port,
        applicability: routing.applicability,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::prepare_xray_runtime_config;
    use crate::config::RoutingOptions;

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn preparation_translates_process_rules_without_leaking_provider_secrets() {
        let provider = json!({
            "inbounds": [],
            "outbounds": [{"tag": "proxy", "protocol": "vless", "settings": {"vnext": [{"users": [{"id": "secret-uuid"}]}]}}]
        });
        let original = provider.clone();
        let mut routing = RoutingOptions::default();
        routing.rules = vec![json!({
            "id": "rule-1",
            "label": "Chrome only",
            "enabled": true,
            "matchers": {"process_name": ["chrome.exe"]},
            "action": {"kind": "route", "outbound": "proxy"}
        })];

        let prepared = prepare_xray_runtime_config(provider, &routing, || Ok(20809)).unwrap();

        assert_eq!(
            original["outbounds"][0]["settings"]["vnext"][0]["users"][0]["id"],
            "secret-uuid"
        );
        assert_eq!(
            prepared.value["routing"]["rules"][0]["process"],
            json!(["chrome.exe"])
        );
        assert_eq!(prepared.applicability.applied_rule_ids, vec!["rule-1"]);
        assert!(prepared.applicability.unavailable.is_empty());
        let rendered = format!("{:?}", prepared.applicability);
        assert!(!rendered.contains("secret-uuid"));
        assert!(!rendered.contains("vnext"));
    }

    #[test]
    fn prepared_config_debug_redacts_runtime_config_and_connection_details() {
        let prepared = prepare_xray_runtime_config(
            json!({
                "inbounds": [{
                    "tag": "sensitive-traffic-tag",
                    "listen": "127.0.0.1",
                    "port": 10809,
                    "protocol": "http"
                }],
                "api": {
                    "tag": "provider-api",
                    "listen": "127.0.0.1:9000",
                    "services": ["HandlerService"]
                },
                "outbounds": [{
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {"providerSecret": "synthetic-provider-secret"}
                }]
            }),
            &RoutingOptions::default(),
            || Ok(29001),
        )
        .unwrap();

        let rendered = format!("{prepared:?}");
        for secret_or_connection_detail in [
            "synthetic-provider-secret",
            "127.0.0.1:9000",
            "127.0.0.1:29001",
            "10809",
            "sensitive-traffic-tag",
            "provider-api",
            "HandlerService",
        ] {
            assert!(!rendered.contains(secret_or_connection_detail));
        }
    }
}
