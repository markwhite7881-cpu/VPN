//! Traffic WebSocket stream (Этап 5).
//!
//! Connects to `ws://<controller>/traffic` and forwards each JSON
//! frame as a Tauri event so the UI can render a live speed chart.
//!
//! sing-box sends roughly one frame per second with cumulative byte
//! counters — we turn them into deltas (bytes per second) on the
//! Rust side and emit `{ up_bps, down_bps, up_total, down_total, ts }`.
//!
//! Reference: <https://sing-box.sagernet.org/configuration/experimental/clash-api/>

use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AppError, AppResult};

/// A traffic sample emitted to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct TrafficSample {
    pub up_bps: u64,
    pub down_bps: u64,
    pub up_total: u64,
    pub down_total: u64,
    pub ts_ms: i64,
}

impl TrafficSample {
    pub const EVENT: &'static str = "traffic";
}

/// Converts cumulative traffic counters into rate samples.
#[derive(Default)]
pub(crate) struct CounterSampler {
    last_up: u64,
    last_down: u64,
    last_at: Option<Instant>,
}

impl CounterSampler {
    pub(crate) fn sample_at(
        &mut self,
        up_total: u64,
        down_total: u64,
        at: Instant,
        ts_ms: i64,
    ) -> TrafficSample {
        let (up_bps, down_bps) = self
            .last_at
            .map(|last_at| {
                let elapsed = if at >= last_at {
                    at.duration_since(last_at)
                } else {
                    Duration::ZERO
                }
                .as_secs_f64()
                .max(0.001);
                (
                    (up_total.saturating_sub(self.last_up) as f64 / elapsed) as u64,
                    (down_total.saturating_sub(self.last_down) as f64 / elapsed) as u64,
                )
            })
            .unwrap_or((0, 0));

        self.last_up = up_total;
        self.last_down = down_total;
        self.last_at = Some(at);

        TrafficSample {
            up_bps,
            down_bps,
            up_total,
            down_total,
            ts_ms,
        }
    }
}

/// Owns at most one WS connection at a time. Calling `start` while
/// already running replaces the previous connection.
pub struct TrafficStream {
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Default for TrafficStream {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
            cancel: Mutex::new(None),
        }
    }
}

impl TrafficStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start streaming traffic. If a previous stream is still running,
    /// it is cancelled first.
    pub async fn start(
        self: &std::sync::Arc<Self>,
        app: AppHandle,
        base_url: &str,
    ) -> AppResult<()> {
        // Cancel any existing stream.
        self.stop().await;

        let ws_url = http_to_ws(base_url);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn the WS reader task. The handle is stored so we can
        // join on it later (best-effort; the task normally exits on
        // its own when the socket drops).
        let app_for_task = app.clone();
        let handle = tokio::spawn(async move {
            let mut sampler = CounterSampler::default();
            let mut backoff = Duration::from_millis(500);

            loop {
                // Connect.
                let connect = tokio_tungstenite::connect_async(ws_url.as_str());

                let mut ws = match connect.await {
                    Ok((ws, _resp)) => ws,
                    Err(e) => {
                        log::warn!("traffic ws connect failed: {e}");
                        // Apply backoff, but abort if cancelled.
                        tokio::select! {
                            _ = &mut cancel_rx => return,
                            _ = tokio::time::sleep(backoff) => {}
                        }
                        backoff = (backoff * 2).min(Duration::from_secs(15));
                        continue;
                    }
                };
                backoff = Duration::from_millis(500);
                log::info!("traffic ws connected: {ws_url}");

                loop {
                    let msg = tokio::select! {
                        _ = &mut cancel_rx => {
                            log::info!("traffic ws: cancel received");
                            let _ = ws.close(None).await;
                            return;
                        }
                        m = ws.next() => m,
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            log::warn!("traffic ws error: {e}; reconnecting");
                            break;
                        }
                        None => {
                            log::info!("traffic ws: stream ended");
                            return;
                        }
                    };

                    if let Message::Text(text) = msg {
                        match parse_traffic_frame(&text, &mut sampler) {
                            Ok(sample) => {
                                if let Err(e) = app_for_task.emit(TrafficSample::EVENT, sample) {
                                    log::warn!("traffic emit failed: {e}");
                                }
                            }
                            Err(e) => log::debug!("traffic parse skipped: {e}"),
                        }
                    }
                }
            }
        });

        *self.handle.lock().await = Some(handle);
        *self.cancel.lock().await = Some(cancel_tx);
        Ok(())
    }

    /// Cancel the running stream (if any).
    pub async fn stop(&self) {
        if let Some(tx) = self.cancel.lock().await.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.lock().await.take() {
            // We don't await — the task should exit promptly via the
            // oneshot, and we don't want to block the caller.
            handle.abort();
        }
    }
}

fn http_to_ws(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{}/traffic", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{}/traffic", rest)
    } else {
        format!("ws://{}/traffic", trimmed)
    }
}

fn parse_traffic_frame(text: &str, sampler: &mut CounterSampler) -> AppResult<TrafficSample> {
    // sing-box sends: {"up": <cumulative bytes>, "down": <cumulative bytes>}
    // It can also occasionally send a heartbeat text frame, which we
    // treat as `up=0, down=0` so the chart keeps a flat line.
    let v: serde_json::Value = serde_json::from_str(text).map_err(AppError::Serde)?;
    let up = v.get("up").and_then(|n| n.as_u64()).unwrap_or(0);
    let down = v.get("down").and_then(|n| n.as_u64()).unwrap_or(0);
    Ok(sampler.sample_at(
        up,
        down,
        Instant::now(),
        chrono::Utc::now().timestamp_millis(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_has_zero_rate_and_preserves_totals() {
        let start = Instant::now();
        let sample = CounterSampler::default().sample_at(1024, 2048, start, 123);

        assert_eq!(sample.up_bps, 0);
        assert_eq!(sample.down_bps, 0);
        assert_eq!(sample.up_total, 1024);
        assert_eq!(sample.down_total, 2048);
        assert_eq!(sample.ts_ms, 123);
    }

    #[test]
    fn second_sample_uses_elapsed_time_for_rates() {
        let start = Instant::now();
        let mut sampler = CounterSampler::default();
        let _ = sampler.sample_at(100, 200, start, 123);
        let sample = sampler.sample_at(1_100, 2_200, start + Duration::from_secs(2), 124);

        assert_eq!(sample.up_bps, 500);
        assert_eq!(sample.down_bps, 1_000);
    }

    #[test]
    fn counter_decrease_is_treated_as_reset() {
        let start = Instant::now();
        let mut sampler = CounterSampler::default();
        let _ = sampler.sample_at(1_000, 2_000, start, 123);
        let sample = sampler.sample_at(100, 200, start + Duration::from_secs(1), 124);

        assert_eq!(sample.up_bps, 0);
        assert_eq!(sample.down_bps, 0);
        assert_eq!(sample.up_total, 100);
        assert_eq!(sample.down_total, 200);
    }

    #[test]
    fn http_to_ws_handles_both_schemes() {
        assert_eq!(
            http_to_ws("http://127.0.0.1:9090"),
            "ws://127.0.0.1:9090/traffic"
        );
        assert_eq!(
            http_to_ws("https://controller.example.com/"),
            "wss://controller.example.com/traffic"
        );
    }

    #[test]
    fn parse_first_frame_has_zero_rate() {
        let mut sampler = CounterSampler::default();
        let s = parse_traffic_frame(r#"{"up":1024,"down":2048}"#, &mut sampler).unwrap();
        assert_eq!(s.up_bps, 0);
        assert_eq!(s.down_bps, 0);
        assert_eq!(s.up_total, 1024);
        assert_eq!(s.down_total, 2048);
        assert!(sampler.last_at.is_some());
    }

    #[test]
    fn parse_second_frame_computes_rate() {
        let mut sampler = CounterSampler::default();
        let _ = parse_traffic_frame(r#"{"up":0,"down":0}"#, &mut sampler);
        std::thread::sleep(Duration::from_millis(1100));
        let s = parse_traffic_frame(r#"{"up":1500,"down":6000}"#, &mut sampler).unwrap();
        // ~1.1s elapsed, 1500 bytes up → ~1300 B/s
        assert!(s.up_bps > 800 && s.up_bps < 2000, "got {}", s.up_bps);
        assert!(s.down_bps > 4000 && s.down_bps < 7000, "got {}", s.down_bps);
        assert_eq!(s.up_total, 1500);
    }

    #[test]
    fn parse_heartbeat_is_zero() {
        let mut sampler = CounterSampler::default();
        let s = parse_traffic_frame("{}", &mut sampler).unwrap();
        assert_eq!(s.up_bps, 0);
        assert_eq!(s.down_bps, 0);
    }
}
