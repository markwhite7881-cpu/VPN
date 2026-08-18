//! Runtime-safe preparation of raw Xray provider configurations.

pub mod inbound;
pub mod routing;
pub mod stats;

use serde_json::Value;

use crate::{config::RoutingOptions, error::AppResult};

pub use routing::{RoutingApplicability, UnavailableReason, UnavailableRule};

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedXrayConfig {
    pub value: Value,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub applicability: RoutingApplicability,
    pub(crate) stats: stats::XrayStatsSpec,
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

    use super::{prepare_xray_runtime_config, UnavailableReason};
    use crate::config::RoutingOptions;

    #[test]
    fn preparation_does_not_mutate_provider_and_sanitizes_diagnostics() {
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
        assert_eq!(prepared.applicability.unavailable.len(), 1);
        assert_eq!(prepared.applicability.unavailable[0].rule_id, "rule-1");
        assert_eq!(prepared.applicability.unavailable[0].label, "Chrome only");
        assert_eq!(
            prepared.applicability.unavailable[0].reason,
            UnavailableReason::ProcessMatcher
        );
        let rendered = format!("{:?}", prepared.applicability);
        assert!(!rendered.contains("secret-uuid"));
        assert!(!rendered.contains("vnext"));
    }
}
