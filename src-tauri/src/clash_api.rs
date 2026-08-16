//! Clash API client (Этап 4).
//!
//! sing-box exposes a Clash-compatible REST surface for switching
//! proxies, measuring latency, and streaming traffic. The functions
//! here all take an explicit `base_url` (the `external_controller`
//! value we put in the config — defaults to `http://127.0.0.1:9090`)
//! so the rest of the app doesn't need to plumb it through.
//!
//! Reference: <https://sing-box.sagernet.org/configuration/experimental/clash-api/>
//!
//! Endpoints used:
//!   GET  /proxies                       — list all groups + members
//!   GET  /proxies/{name}                — single proxy
//!   PUT  /proxies/{name}                — change selection (Selector only)
//!   GET  /proxies/{name}/delay?timeout  — measure latency in ms

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// Thin reqwest::Client wrapper. Cheap to clone.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client should build");
        Self { http }
    }

    /// Fetch all proxies from `base_url/proxies`.
    pub async fn list_proxies(&self, base_url: &str) -> AppResult<ProxiesResponse> {
        let url = format!("{}/proxies", base_url.trim_end_matches('/'));
        let resp = self.http.get(&url).send().await.map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Clash(format!("GET /proxies returned {}", status)));
        }
        let body: ProxiesResponse = resp.json().await.map_err(http_err)?;
        Ok(body)
    }

    /// PUT /proxies/{name}  with body {"name": <member>}.
    ///
    /// sing-box's clash API differs from upstream clash: there is
    /// **no** `/select` suffix — the PUT goes directly at the
    /// proxy resource, and the body picks the active member of the
    /// selector. The original clash endpoint `/proxies/{name}/select`
    /// returns 404 in sing-box and silently no-ops if the caller
    /// ignores the status — which is exactly the bug that made
    /// our `proxy` selector stay pinned to `auto` while urltest
    /// kept switching traffic to whichever server was fastest.
    pub async fn select_proxy(&self, base_url: &str, group: &str, member: &str) -> AppResult<()> {
        let url = format!(
            "{}/proxies/{}",
            base_url.trim_end_matches('/'),
            urlencode(group)
        );
        let resp = self
            .http
            .put(&url)
            .json(&serde_json::json!({ "name": member }))
            .send()
            .await
            .map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Clash(format!(
                "select {group} -> {member} failed: {status} {text}"
            )));
        }
        Ok(())
    }

    /// GET /proxies/{name}/delay?timeout=<ms>
    /// Returns the measured delay in milliseconds, or `None` if the
    /// proxy doesn't support delay testing.
    pub async fn test_delay(
        &self,
        base_url: &str,
        name: &str,
        timeout_ms: u32,
    ) -> AppResult<Option<u32>> {
        let url = format!(
            "{}/proxies/{}/delay?timeout={}",
            base_url.trim_end_matches('/'),
            urlencode(name),
            timeout_ms
        );
        let resp = self.http.get(&url).send().await.map_err(http_err)?;
        let status = resp.status();
        if status.as_u16() == 408 || status.as_u16() == 504 {
            // Timeout / no upstream reachable.
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AppError::Clash(format!("delay({name}) returned {status}")));
        }
        // sing-box returns {"delay": 123} or {"message": "..."} on failure.
        let value: Value = resp.json().await.map_err(http_err)?;
        if let Some(n) = value.get("delay").and_then(|v| v.as_u64()) {
            return Ok(Some(n as u32));
        }
        Ok(None)
    }
}

fn http_err(e: reqwest::Error) -> AppError {
    AppError::Clash(format!("http error: {e}"))
}

fn urlencode(s: &str) -> String {
    // Minimal URL-encoding for proxy names. sing-box names are typically
    // ASCII but tag values can be anything (emoji, non-ASCII).
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{:02X}", c));
        }
    }
    out
}

// --- response types -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxiesResponse {
    pub proxies: std::collections::BTreeMap<String, ProxyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyInfo {
    /// Clash type string: "Selector", "URLTest", "Direct", "Block",
    /// "VLESS", "Shadowsocks", "Hysteria2", etc.
    pub r#type: String,
    /// All available members (only for Selector / URLTest).
    #[serde(default)]
    pub all: Vec<String>,
    /// Currently selected member (only for Selector / URLTest).
    #[serde(default)]
    pub now: Option<String>,
    /// Recent delay samples in ms, oldest first (only for URLTest).
    #[serde(default)]
    pub history: Vec<DelayRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayRecord {
    pub time: chrono::DateTime<chrono::Utc>,
    pub delay: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_handles_unicode() {
        assert_eq!(urlencode("DE-1"), "DE-1");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("🇩🇪 DE"), "%F0%9F%87%A9%F0%9F%87%AA%20DE");
    }
}
