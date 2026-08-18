use std::{
    fmt,
    net::{IpAddr, SocketAddr},
};

use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};

pub(crate) mod grpc {
    tonic::include_proto!("xray.app.stats.command");
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct XrayStatsSpec {
    pub(crate) api_host: String,
    pub(crate) api_port: u16,
    pub(crate) traffic_tag: String,
}

impl fmt::Debug for XrayStatsSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("XrayStatsSpec { .. }")
    }
}

impl XrayStatsSpec {
    pub(crate) fn uplink_counter_name(&self) -> String {
        counter_name(&self.traffic_tag, "uplink")
    }

    pub(crate) fn downlink_counter_name(&self) -> String {
        counter_name(&self.traffic_tag, "downlink")
    }
}

pub(crate) fn merge_stats_config<F>(
    mut provider: Value,
    traffic_tag: &str,
    port_allocator: F,
) -> AppResult<(Value, XrayStatsSpec)>
where
    F: FnOnce() -> AppResult<u16>,
{
    let root = provider
        .as_object_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray provider config must be an object".into()))?;

    let api = object_field(root, "api", "Xray api must be an object")?;
    if let Some(listen) = api.get("listen") {
        let listen = listen
            .as_str()
            .ok_or_else(|| AppError::UnsafeConfig("Xray API listener is invalid".into()))?;
        if !is_loopback_listener(listen) {
            return Err(AppError::UnsafeConfig(
                "Xray API listener must listen on loopback".into(),
            ));
        }
    }

    let services = array_field(api, "services", "Xray API services must be an array")?;
    if !services
        .iter()
        .any(|service| service.as_str() == Some("StatsService"))
    {
        services.push(Value::String("StatsService".into()));
    }

    let port = port_allocator()?;
    if port == 0 {
        return Err(AppError::UnsafeConfig(
            "Xray stats API port is invalid".into(),
        ));
    }
    api.insert("listen".into(), Value::String(format!("127.0.0.1:{port}")));

    object_field(root, "stats", "Xray stats must be an object")?;
    let policy = object_field(root, "policy", "Xray policy must be an object")?;
    let system = object_field(policy, "system", "Xray policy system must be an object")?;
    system.insert("statsInboundUplink".into(), Value::Bool(true));
    system.insert("statsInboundDownlink".into(), Value::Bool(true));

    Ok((
        provider,
        XrayStatsSpec {
            api_host: "127.0.0.1".into(),
            api_port: port,
            traffic_tag: traffic_tag.into(),
        },
    ))
}

fn object_field<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
    error: &str,
) -> AppResult<&'a mut Map<String, Value>> {
    parent
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::UnsafeConfig(error.into()))
}

fn array_field<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
    error: &str,
) -> AppResult<&'a mut Vec<Value>> {
    parent
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::UnsafeConfig(error.into()))
}

fn is_loopback_listener(listen: &str) -> bool {
    match listen.parse::<SocketAddr>() {
        Ok(address) => address.ip().is_loopback(),
        Err(_) => {
            let Some((host, _)) = listen.rsplit_once(':') else {
                return false;
            };
            matches!(host, "localhost" | "127.0.0.1" | "::1")
                || host
                    .parse::<IpAddr>()
                    .map(|address| address.is_loopback())
                    .unwrap_or(false)
        }
    }
}

fn counter_name(traffic_tag: &str, direction: &str) -> String {
    format!("inbound>>>{traffic_tag}>>>traffic>>>{direction}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_stats_config;

    #[test]
    fn merge_adds_loopback_stats_api_and_inbound_counters() {
        let (value, spec) = merge_stats_config(
            json!({
                "inbounds": [{"tag":"provider-http","listen":"127.0.0.1","port":10809,"protocol":"http"}],
                "outbounds": [{"tag":"proxy","protocol":"freedom"}]
            }),
            "provider-http",
            || Ok(29001),
        )
        .unwrap();

        assert_eq!(value["stats"], json!({}));
        assert_eq!(value["api"]["listen"], "127.0.0.1:29001");
        assert_eq!(value["api"]["services"], json!(["StatsService"]));
        assert_eq!(value["policy"]["system"]["statsInboundUplink"], true);
        assert_eq!(value["policy"]["system"]["statsInboundDownlink"], true);
        assert_eq!(spec.traffic_tag, "provider-http");
        assert_eq!(
            spec.uplink_counter_name(),
            "inbound>>>provider-http>>>traffic>>>uplink"
        );
        assert_eq!(
            spec.downlink_counter_name(),
            "inbound>>>provider-http>>>traffic>>>downlink"
        );
    }

    #[test]
    fn merge_preserves_existing_api_services_and_policy_fields() {
        let (value, _) = merge_stats_config(
            json!({
                "api": {
                    "tag": "provider-api",
                    "listen": "127.0.0.1:9000",
                    "services": ["HandlerService", "StatsService"],
                    "customApiField": {"enabled": true}
                },
                "policy": {"levels":{"0":{"handshake":4}},"system":{"statsInboundUplink":false}},
                "stats": {"existing": true}
            }),
            "provider-http",
            || Ok(29001),
        )
        .unwrap();

        assert_eq!(value["api"]["listen"], "127.0.0.1:29001");
        assert_eq!(value["api"]["tag"], "provider-api");
        assert_eq!(
            value["api"]["services"],
            json!(["HandlerService", "StatsService"])
        );
        assert_eq!(value["api"]["customApiField"], json!({"enabled": true}));
        assert_eq!(value["policy"]["levels"]["0"]["handshake"], 4);
        assert_eq!(value["policy"]["system"]["statsInboundUplink"], true);
        assert_eq!(value["stats"]["existing"], true);
    }

    #[test]
    fn merge_rejects_non_loopback_api_listener() {
        let result = merge_stats_config(
            json!({"api": {"listen": "0.0.0.0:9000", "services": []}}),
            "provider-http",
            || Ok(29001),
        );

        assert!(result.is_err());
    }

    #[test]
    fn merge_adds_stats_service_only_once() {
        let (value, _) = merge_stats_config(
            json!({"api": {"listen": "127.0.0.1:9000", "services": ["StatsService"]}}),
            "provider-http",
            || Ok(29001),
        )
        .unwrap();

        assert_eq!(value["api"]["services"], json!(["StatsService"]));
    }
}
