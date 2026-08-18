use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::traffic::{CounterSampler, TrafficSample};

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

/// Owns one private Xray StatsService polling task at a time.
pub(crate) struct XrayStatsStream {
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Default for XrayStatsStream {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
            cancel: Mutex::new(None),
        }
    }
}

impl XrayStatsStream {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn start(
        self: &Arc<Self>,
        app: AppHandle,
        spec: XrayStatsSpec,
        run_id: u64,
        active_run_id: Arc<AtomicU64>,
    ) -> AppResult<()> {
        self.stop().await;

        let Some(endpoint) = loopback_endpoint(&spec) else {
            log::warn!("Xray traffic stats unavailable");
            return Ok(());
        };
        let uplink_counter = spec.uplink_counter_name();
        let downlink_counter = spec.downlink_counter_name();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let app_for_task = app.clone();

        let handle = tokio::spawn(async move {
            let mut sampler = CounterSampler::default();
            let mut backoff = Duration::from_millis(500);

            loop {
                let mut client =
                    match grpc::stats_service_client::StatsServiceClient::connect(endpoint.clone())
                        .await
                    {
                        Ok(client) => client,
                        Err(_) => {
                            log::warn!("Xray traffic stats unavailable; retrying");
                            if !wait_or_cancel(&mut cancel_rx, backoff).await {
                                return;
                            }
                            backoff = (backoff * 2).min(Duration::from_secs(15));
                            continue;
                        }
                    };

                loop {
                    let counters = tokio::select! {
                        _ = &mut cancel_rx => return,
                        result = poll_counters(&mut client, &uplink_counter, &downlink_counter) => result,
                    };

                    match counters {
                        Ok(Some((up_total, down_total))) => {
                            let sample = sampler.sample_at(
                                up_total,
                                down_total,
                                tokio::time::Instant::now(),
                                chrono::Utc::now().timestamp_millis(),
                            );
                            if owns_run(run_id, active_run_id.load(Ordering::Acquire)) {
                                if app_for_task.emit(TrafficSample::EVENT, sample).is_err() {
                                    log::warn!("Xray traffic sample emit failed");
                                }
                            }
                            backoff = Duration::from_millis(500);
                        }
                        Ok(None) => {}
                        Err(()) => {
                            log::warn!("Xray traffic stats unavailable; retrying");
                            break;
                        }
                    }

                    if !wait_or_cancel(&mut cancel_rx, Duration::from_secs(1)).await {
                        return;
                    }
                }

                if !wait_or_cancel(&mut cancel_rx, backoff).await {
                    return;
                }
                backoff = (backoff * 2).min(Duration::from_secs(15));
            }
        });

        *self.handle.lock().await = Some(handle);
        *self.cancel.lock().await = Some(cancel_tx);
        Ok(())
    }

    pub(crate) async fn stop(&self) {
        if let Some(tx) = self.cancel.lock().await.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.lock().await.take() {
            handle.abort();
        }
    }
}

fn loopback_endpoint(spec: &XrayStatsSpec) -> Option<String> {
    let host = spec.api_host.parse::<IpAddr>().ok()?;
    host.is_loopback()
        .then(|| format!("http://{}:{}", spec.api_host, spec.api_port))
}

async fn wait_or_cancel(
    cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
    duration: Duration,
) -> bool {
    tokio::select! {
        _ = cancel_rx => false,
        _ = tokio::time::sleep(duration) => true,
    }
}

async fn poll_counters(
    client: &mut grpc::stats_service_client::StatsServiceClient<tonic::transport::Channel>,
    uplink_counter: &str,
    downlink_counter: &str,
) -> Result<Option<(u64, u64)>, ()> {
    let uplink = client
        .get_stats(grpc::GetStatsRequest {
            name: uplink_counter.into(),
            reset: false,
        })
        .await
        .map_err(|_| ())?;
    let downlink = client
        .get_stats(grpc::GetStatsRequest {
            name: downlink_counter.into(),
            reset: false,
        })
        .await
        .map_err(|_| ())?;

    Ok(extract_counter_value(&uplink.into_inner())
        .zip(extract_counter_value(&downlink.into_inner())))
}

fn extract_counter_value(response: &grpc::GetStatsResponse) -> Option<u64> {
    response.stat.as_ref()?.value.try_into().ok()
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

fn owns_run(run_id: u64, active_run_id: u64) -> bool {
    run_id != 0 && run_id == active_run_id
}

fn counter_name(traffic_tag: &str, direction: &str) -> String {
    format!("inbound>>>{traffic_tag}>>>traffic>>>{direction}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{extract_counter_value, grpc, merge_stats_config, owns_run};

    #[test]
    fn stale_run_cannot_own_a_traffic_sample_emit() {
        assert!(owns_run(7, 7));
        assert!(!owns_run(7, 8));
        assert!(!owns_run(7, 0));
    }

    #[test]
    fn malformed_or_missing_stat_is_reported_without_panicking() {
        assert_eq!(
            extract_counter_value(&grpc::GetStatsResponse { stat: None }),
            None
        );
        assert_eq!(
            extract_counter_value(&grpc::GetStatsResponse {
                stat: Some(grpc::Stat {
                    name: "ignored".into(),
                    value: -1,
                }),
            }),
            None
        );
    }

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
