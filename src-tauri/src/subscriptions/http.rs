use std::net::IpAddr;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use reqwest::{redirect, Client, StatusCode};
use url::{Host, Url};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

use super::metadata::parse_metadata;
use super::model::ProviderMetadata;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_REDIRECTS: usize = 10;

#[derive(Clone)]
pub struct FetchedPayload {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
    pub metadata: ProviderMetadata,
    pub status: u16,
}

#[derive(Debug, Clone)]
pub struct SubscriptionHttpClient {
    client: Client,
}

impl SubscriptionHttpClient {
    pub fn new() -> AppResult<Self> {
        Self::build(REQUEST_TIMEOUT)
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(timeout: Duration) -> AppResult<Self> {
        Self::build(timeout)
    }

    fn build(timeout: Duration) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("redirect limit exceeded");
                }
                if redirect_is_allowed(attempt.previous().last(), attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("redirect blocked")
                }
            }))
            .build()
            .map_err(|_| AppError::Subscription("subscription HTTP client setup failed".into()))?;
        Ok(Self { client })
    }

    pub async fn fetch(
        &self,
        url: &Url,
        hwid: Uuid,
        app_version: &str,
        platform: &str,
    ) -> AppResult<FetchedPayload> {
        validate_initial_url(url)?;
        let user_agent = format!("Cloakwire/{app_version} ({platform})");
        let response = self
            .client
            .get(url.clone())
            .header(USER_AGENT, user_agent)
            .header(ACCEPT, "application/json, text/plain")
            .header("X-HWID", hwid.to_string())
            .header("X-Device-OS", "Windows")
            .header("X-Device-Model", "Cloakwire Desktop")
            .send()
            .await
            .map_err(map_request_error)?;

        let status = response.status();
        validate_status(status)?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BODY_BYTES as u64)
        {
            return Err(AppError::PayloadTooLarge);
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let metadata = parse_metadata(response.headers())?;
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| {
                AppError::Subscription("subscription response body read failed".into())
            })?;
            if bytes.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                return Err(AppError::PayloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }

        Ok(FetchedPayload {
            bytes,
            content_type,
            metadata,
            status: status.as_u16(),
        })
    }
}

fn validate_initial_url(url: &Url) -> AppResult<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_local_url(url) => Ok(()),
        "http" => Err(AppError::UnsafeRedirect(
            "non-local subscription HTTP is blocked".into(),
        )),
        _ => Err(AppError::UnsafeRedirect(
            "subscription URL scheme is blocked".into(),
        )),
    }
}

fn redirect_is_allowed(previous: Option<&Url>, next: &Url) -> bool {
    if !matches!(next.scheme(), "http" | "https") {
        return false;
    }
    if next.scheme() == "http" && !is_local_url(next) {
        return false;
    }
    if previous.is_some_and(|url| url.scheme() == "https") && next.scheme() == "http" {
        return previous.is_some_and(is_local_url) && is_local_url(next);
    }
    true
}

fn is_local_url(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

fn map_request_error(error: reqwest::Error) -> AppError {
    if error.is_redirect() {
        AppError::UnsafeRedirect("subscription redirect was blocked".into())
    } else if error.is_timeout() {
        AppError::Subscription("subscription request timed out".into())
    } else {
        AppError::Subscription("subscription request failed".into())
    }
}

fn validate_status(status: StatusCode) -> AppResult<()> {
    match status {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AppError::SubscriptionAuth(
            format!("provider returned HTTP status {}", status.as_u16()),
        )),
        StatusCode::GONE => Err(AppError::SubscriptionExpired(format!(
            "provider returned HTTP status {}",
            status.as_u16()
        ))),
        StatusCode::PAYLOAD_TOO_LARGE => Err(AppError::PayloadTooLarge),
        status => Err(AppError::Subscription(format!(
            "provider returned HTTP status {}",
            status.as_u16()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::redirect_is_allowed;
    use url::Url;

    #[test]
    fn rejects_https_to_non_local_http_downgrade() {
        let previous = Url::parse("https://example.test/subscription").unwrap();
        let next = Url::parse("http://example.test/subscription").unwrap();
        assert!(!redirect_is_allowed(Some(&previous), &next));
    }

    #[test]
    fn permits_localhost_https_to_http_development_downgrade() {
        let previous = Url::parse("https://localhost/subscription").unwrap();
        let next = Url::parse("http://127.0.0.1/subscription").unwrap();
        assert!(redirect_is_allowed(Some(&previous), &next));
    }
}
