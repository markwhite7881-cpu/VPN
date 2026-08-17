use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use url::Url;

use crate::error::AppResult;

use super::model::{ProviderMetadata, SubscriptionUserinfo};

const MIN_UPDATE_HOURS: u32 = 1;
const MAX_UPDATE_HOURS: u32 = 30 * 24;

pub fn parse_metadata(headers: &HeaderMap) -> AppResult<ProviderMetadata> {
    let profile_title = header_text(headers, "profile-title")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let update_interval_hours = header_text(headers, "profile-update-interval")
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|hours| hours.clamp(MIN_UPDATE_HOURS, MAX_UPDATE_HOURS));
    let profile_web_page_url = header_text(headers, "profile-web-page-url").and_then(parse_web_url);
    let support_url = header_text(headers, "support-url").and_then(parse_web_url);
    let userinfo = header_text(headers, "subscription-userinfo").and_then(parse_userinfo);

    Ok(ProviderMetadata {
        profile_title,
        update_interval_hours,
        profile_web_page_url,
        support_url,
        upload_bytes: userinfo.as_ref().and_then(|value| value.upload),
        download_bytes: userinfo.as_ref().and_then(|value| value.download),
        total_bytes: userinfo.as_ref().and_then(|value| value.total),
        expires_at: userinfo.as_ref().and_then(|value| value.expire),
        userinfo,
    })
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn parse_web_url(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.into())
}

fn parse_userinfo(value: &str) -> Option<SubscriptionUserinfo> {
    let mut userinfo = SubscriptionUserinfo::default();
    for part in value.split(';') {
        let Some((key, raw_value)) = part.trim().split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "upload" => userinfo.upload = raw_value.trim().parse().ok(),
            "download" => userinfo.download = raw_value.trim().parse().ok(),
            "total" => userinfo.total = raw_value.trim().parse().ok(),
            "expire" => {
                userinfo.expire = raw_value
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .and_then(DateTime::<Utc>::from_timestamp_secs)
            }
            _ => {}
        }
    }

    (userinfo != SubscriptionUserinfo::default()).then_some(userinfo)
}

#[cfg(test)]
mod tests {
    use super::parse_metadata;
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

    fn headers(values: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in values {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn parses_allowlisted_headers_and_userinfo() {
        let metadata = parse_metadata(&headers(&[
            ("Profile-Title", "Cloakwire Demo"),
            ("Profile-Update-Interval", "6"),
            ("Profile-Web-Page-Url", "https://example.test/profile"),
            ("Support-Url", "http://localhost:3000/support"),
            (
                "Subscription-Userinfo",
                "upload=1; download=2; total=100; expire=2000000000",
            ),
            ("Set-Cookie", "secret=never-copy"),
        ]))
        .unwrap();

        assert_eq!(metadata.profile_title.as_deref(), Some("Cloakwire Demo"));
        assert_eq!(metadata.update_interval_hours, Some(6));
        assert_eq!(
            metadata.profile_web_page_url.as_deref(),
            Some("https://example.test/profile")
        );
        assert_eq!(
            metadata.support_url.as_deref(),
            Some("http://localhost:3000/support")
        );
        let userinfo = metadata.userinfo.unwrap();
        assert_eq!(userinfo.upload, Some(1));
        assert_eq!(userinfo.download, Some(2));
        assert_eq!(userinfo.total, Some(100));
        assert_eq!(userinfo.expire.unwrap().timestamp(), 2_000_000_000);
    }

    #[test]
    fn clamps_update_interval_to_supported_range() {
        let too_short = parse_metadata(&headers(&[("Profile-Update-Interval", "0")])).unwrap();
        let too_long = parse_metadata(&headers(&[("Profile-Update-Interval", "10000")])).unwrap();
        assert_eq!(too_short.update_interval_hours, Some(1));
        assert_eq!(too_long.update_interval_hours, Some(720));
    }

    #[test]
    fn ignores_non_http_metadata_urls_and_credentials() {
        let metadata = parse_metadata(&headers(&[
            ("Profile-Web-Page-Url", "file:///private/provider.json"),
            ("Support-Url", "https://user:password@example.test/support"),
        ]))
        .unwrap();
        assert_eq!(metadata.profile_web_page_url, None);
        assert_eq!(metadata.support_url, None);
    }

    #[test]
    fn ignores_unknown_and_malformed_userinfo_fields() {
        let metadata = parse_metadata(&headers(&[(
            "Subscription-Userinfo",
            "upload=bad; total=100; token=never-copy",
        )]))
        .unwrap();
        let userinfo = metadata.userinfo.unwrap();
        assert_eq!(userinfo.upload, None);
        assert_eq!(userinfo.total, Some(100));
    }
}
