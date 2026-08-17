use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

use crate::error::AppError;

use super::{
    AddSubscriptionInput, HwidStore, SubscriptionHttpClient, SubscriptionService, SubscriptionStore,
};

#[derive(Debug)]
struct CapturedRequest {
    headers: HashMap<String, String>,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map(String::as_str).unwrap_or("")
    }
}

async fn spawn_server(
    response: Vec<u8>,
    delay: Duration,
) -> (Url, tokio::task::JoinHandle<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 16 * 1024];
        let count = socket.read(&mut buffer).await.unwrap();
        let request = String::from_utf8(buffer[..count].to_vec()).unwrap();
        let mut headers = HashMap::new();
        for line in request.lines().skip(1) {
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        tokio::time::sleep(delay).await;
        let _ = socket.write_all(&response).await;
        CapturedRequest { headers }
    });
    (
        Url::parse(&format!("http://{address}/subscription")).unwrap(),
        task,
    )
}

#[tokio::test]
async fn opaque_link_refs_resolve_and_reject_invalid_selection() {
    use crate::parser::Outbound;
    use crate::subscriptions::{SubscriptionKind, SubscriptionLinkRef, SubscriptionRecord};

    let directory = tempfile::tempdir().unwrap();
    let link = crate::parser::parse_link(
        "vless://11111111-1111-4111-8111-111111111111@server-address.example.test:443?security=tls#ProviderLabel",
    )
    .unwrap();
    let record = SubscriptionRecord {
        id: "sub-opaque".into(), name: "Provider".into(), url: "https://example.test".into(),
        kind: SubscriptionKind::LinkList, engine: Some(super::EngineKind::Singbox), interval_minutes: 60,
        active_child_key: None, children: Vec::new(), link_outbounds: vec![link], bundle_digest: None,
        metadata: Default::default(), last_success_at: None, last_http_status: Some(200), last_error: None,
    };
    super::SubscriptionStore::new(directory.path().join("subscriptions.json"))
        .replace_all(&[record]).unwrap();
    let service = SubscriptionService::new(
        SubscriptionStore::new(directory.path().join("subscriptions.json")),
        HwidStore::new(directory.path().join("device-id")), SubscriptionHttpClient::new().unwrap(), "1.3.0".into(),
    );
    let resolved = service.resolve_link_refs(&[SubscriptionLinkRef { subscription_id: "sub-opaque".into(), link_key: "index-0".into() }]).await.unwrap();
    assert!(matches!(resolved.first(), Some(Outbound::Vless(_))));
    let stale = service.resolve_link_refs(&[SubscriptionLinkRef { subscription_id: "sub-opaque".into(), link_key: "index-9".into() }]).await;
    assert!(matches!(stale, Err(AppError::Validation(_))));
    for invalid_key in ["index-00", "index-01", "index-+1", "index--1", "index-"] {
        let invalid = service.resolve_link_refs(&[SubscriptionLinkRef { subscription_id: "sub-opaque".into(), link_key: invalid_key.into() }]).await;
        assert!(matches!(invalid, Err(AppError::Validation(_))), "{invalid_key} must be rejected");
    }
    let duplicate = service.resolve_link_refs(&[SubscriptionLinkRef { subscription_id: "sub-opaque".into(), link_key: "index-0".into() }; 2]).await;
    assert!(matches!(duplicate, Err(AppError::Validation(_))));
}

#[tokio::test]
async fn sends_exact_headers_and_parses_metadata() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nProfile-Title: Demo\r\nContent-Length: 33\r\nConnection: close\r\n\r\n{\"outbounds\":[{\"type\":\"direct\"}]}".to_vec();
    let (url, captured) = spawn_server(response, Duration::ZERO).await;
    let hwid = Uuid::new_v4();

    let payload = SubscriptionHttpClient::new()
        .unwrap()
        .fetch(&url, hwid, "1.3.0", "Windows")
        .await
        .unwrap();
    let captured = captured.await.unwrap();

    assert_eq!(captured.header("user-agent"), "Cloakwire/1.3.0 (Windows)");
    assert_eq!(captured.header("accept"), "application/json, text/plain");
    assert_eq!(captured.header("x-device-os"), "Windows");
    assert_eq!(captured.header("x-device-model"), "Cloakwire Desktop");
    assert!(captured.header("x-hwid").parse::<Uuid>().is_ok());
    assert_eq!(payload.status, 200);
    assert_eq!(payload.content_type.as_deref(), Some("application/json"));
    assert_eq!(payload.metadata.profile_title.as_deref(), Some("Demo"));
    assert_eq!(payload.bytes.len(), 33);
}

#[tokio::test]
async fn rejects_non_local_http_before_sending() {
    let url = Url::parse("http://example.invalid/subscription").unwrap();
    let result = SubscriptionHttpClient::new()
        .unwrap()
        .fetch(&url, Uuid::new_v4(), "1.3.0", "Windows")
        .await;
    assert!(matches!(result, Err(AppError::UnsafeRedirect(_))));
}

#[tokio::test]
async fn stops_streaming_above_ten_mibibytes() {
    let body = vec![b'x'; 10 * 1024 * 1024 + 1];
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    let (url, server) = spawn_server(response, Duration::ZERO).await;

    let result = SubscriptionHttpClient::new()
        .unwrap()
        .fetch(&url, Uuid::new_v4(), "1.3.0", "Windows")
        .await;
    assert!(matches!(result, Err(AppError::PayloadTooLarge)));
    let _ = server.await;
}

#[tokio::test]
async fn applies_request_timeout() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK".to_vec();
    let (url, server) = spawn_server(response, Duration::from_millis(250)).await;

    let result = SubscriptionHttpClient::with_timeout(Duration::from_millis(30))
        .unwrap()
        .fetch(&url, Uuid::new_v4(), "1.3.0", "Windows")
        .await;
    assert!(matches!(result, Err(AppError::Subscription(_))));
    let _ = server.await;
}

#[tokio::test]
async fn blocks_redirect_to_non_local_http() {
    let response = b"HTTP/1.1 302 Found\r\nLocation: http://example.invalid/subscription\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
    let (url, server) = spawn_server(response, Duration::ZERO).await;

    let result = SubscriptionHttpClient::new()
        .unwrap()
        .fetch(&url, Uuid::new_v4(), "1.3.0", "Windows")
        .await;
    assert!(matches!(result, Err(AppError::UnsafeRedirect(_))));
    let _ = server.await;
}

#[tokio::test]
async fn snapshot_never_serializes_or_debug_formats_link_credentials() {
    let response = http_response(
        "text/plain",
        b"vless://11111111-1111-4111-8111-111111111111@server-address.example.test:443?security=tls#ProviderLabel\ntrojan://supersecret-password@credential-server.example.test:443?sni=credential-server.example.test#AnotherLabel",
    );
    let (url, server) = spawn_sequence_server(vec![response]).await;
    let directory = tempfile::tempdir().unwrap();
    let service = SubscriptionService::new(
        SubscriptionStore::new(directory.path().join("subscriptions.json")),
        HwidStore::new(directory.path().join("device-id")),
        SubscriptionHttpClient::new().unwrap(),
        "1.3.0".into(),
    );

    service
        .add(AddSubscriptionInput {
            name: "Provider".into(),
            url: url.to_string(),
            interval_minutes: 60,
        })
        .await
        .unwrap();
    let snapshot = service.list().await.unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    let debug = format!("{snapshot:?}");

    for secret in [
        "server-address.example.test",
        "credential-server.example.test",
        "11111111-1111-4111-8111-111111111111",
        "supersecret-password",
        "vless://",
        "trojan://",
    ] {
        assert!(
            !serialized.contains(secret),
            "serialized snapshot leaked {secret}"
        );
        assert!(!debug.contains(secret), "debug snapshot leaked {secret}");
    }
    assert_eq!(snapshot.link_outbounds.len(), 1);
    assert_eq!(snapshot.link_outbounds[0].links.len(), 2);
    assert_eq!(snapshot.link_outbounds[0].links[0].key, "index-0");
    assert_eq!(snapshot.link_outbounds[0].links[0].label, "vless link 1");
    assert_eq!(snapshot.link_outbounds[0].links[0].protocol, "vless");
    assert_eq!(snapshot.link_outbounds[0].links[1].key, "index-1");
    assert_eq!(snapshot.link_outbounds[0].links[1].label, "trojan link 2");
    assert_eq!(snapshot.link_outbounds[0].links[1].protocol, "trojan");
    let _ = server.await;
}

#[tokio::test]
async fn legacy_fetch_returns_link_list_compatibility_result() {
    let response = http_response(
        "text/plain",
        b"vless://11111111-1111-4111-8111-111111111111@server-address.example.test:443?security=tls#ProviderLabel",
    );
    let (url, server) = spawn_sequence_server(vec![response]).await;
    let directory = tempfile::tempdir().unwrap();
    let service = SubscriptionService::new(
        SubscriptionStore::new(directory.path().join("subscriptions.json")),
        HwidStore::new(directory.path().join("device-id")),
        SubscriptionHttpClient::new().unwrap(),
        "1.3.0".into(),
    );

    let result = service.fetch_legacy_links(url.as_str()).await.unwrap();

    assert_eq!(result.outbounds.len(), 1);
    assert_eq!(result.outbounds[0].protocol(), "vless");
    let _ = server.await;
}

#[tokio::test]
async fn legacy_fetch_rejects_full_configuration_bundle() {
    let (url, server) = spawn_sequence_server(vec![xray_bundle(&["Private child"])]).await;
    let directory = tempfile::tempdir().unwrap();
    let service = SubscriptionService::new(
        SubscriptionStore::new(directory.path().join("subscriptions.json")),
        HwidStore::new(directory.path().join("device-id")),
        SubscriptionHttpClient::new().unwrap(),
        "1.3.0".into(),
    );

    let result = service.fetch_legacy_links(url.as_str()).await;

    assert!(matches!(result, Err(AppError::Unsupported(_))));
    let _ = server.await;
}

#[tokio::test]
async fn failed_first_refresh_does_not_store_subscription() {
    let (url, server) = spawn_sequence_server(vec![http_response(
        "application/json",
        br#"[{"not":"classifiable"}]"#,
    )])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let service = SubscriptionService::new(
        SubscriptionStore::new(directory.path().join("subscriptions.json")),
        HwidStore::new(directory.path().join("device-id")),
        SubscriptionHttpClient::new().unwrap(),
        "1.3.0".into(),
    );

    assert!(service
        .add(AddSubscriptionInput {
            name: "Provider".into(),
            url: url.to_string(),
            interval_minutes: 60,
        })
        .await
        .is_err());
    assert!(service.list().await.unwrap().subscriptions.is_empty());
    let _ = server.await;
}

#[tokio::test]
async fn failed_refresh_keeps_last_valid_bundle_and_selection() {
    let (service, id, server) = service_with_responses(vec![
        xray_bundle(&["First", "Second"]),
        http_response("application/json", br#"[{"not":"classifiable"}]"#),
    ])
    .await;

    service.select_child(&id, "index-1").await.unwrap();
    assert!(service.refresh(&id).await.is_err());
    let after = service.list().await.unwrap().subscriptions.remove(0);

    assert_eq!(after.active_child_key.as_deref(), Some("index-1"));
    assert_eq!(after.children.len(), 2);
    assert_eq!(
        after.kind,
        crate::subscriptions::SubscriptionKind::XrayBundle
    );
    let _ = server.await;
}

#[tokio::test]
async fn refresh_keeps_positional_selection_when_credentials_change() {
    let (service, id, server) = service_with_responses(vec![
        xray_bundle(&["First", "Second"]),
        xray_bundle(&["First updated", "Second updated"]),
    ])
    .await;

    service.select_child(&id, "index-1").await.unwrap();
    let refresh = service.refresh(&id).await.unwrap();

    assert!(!refresh.selection_changed);
    assert_eq!(
        refresh.subscription.active_child_key.as_deref(),
        Some("index-1")
    );
    assert_eq!(refresh.subscription.children[1].key, "index-1");
    let _ = server.await;
}

#[tokio::test]
async fn refresh_keeps_selection_at_the_same_position_when_children_reorder() {
    let (service, id, server) = service_with_responses(vec![
        xray_bundle(&["First", "Second"]),
        xray_bundle(&["Second", "First"]),
    ])
    .await;

    service.select_child(&id, "index-1").await.unwrap();
    let refresh = service.refresh(&id).await.unwrap();

    assert!(!refresh.selection_changed);
    assert_eq!(
        refresh.subscription.active_child_key.as_deref(),
        Some("index-1")
    );
    assert_eq!(refresh.subscription.children[1].name, "First");
    let _ = server.await;
}

#[tokio::test]
async fn refresh_falls_back_to_first_child_when_selected_position_is_removed() {
    let (service, id, server) = service_with_responses(vec![
        xray_bundle(&["First", "Second"]),
        xray_bundle(&["First"]),
    ])
    .await;

    service.select_child(&id, "index-1").await.unwrap();
    let refresh = service.refresh(&id).await.unwrap();

    assert!(refresh.selection_changed);
    assert_eq!(
        refresh.subscription.active_child_key.as_deref(),
        Some("index-0")
    );
    let _ = server.await;
}

#[tokio::test]
async fn mixed_bundle_refresh_rejects_the_entire_candidate() {
    let (service, id, server) = service_with_responses(vec![
        xray_bundle(&["First", "Second"]),
        http_response(
            "application/json",
            br#"[{"outbounds":[{"protocol":"freedom"}]},{"outbounds":[{"type":"direct"}]}]"#,
        ),
    ])
    .await;

    service.select_child(&id, "index-1").await.unwrap();
    assert!(service.refresh(&id).await.is_err());
    let after = service.list().await.unwrap().subscriptions.remove(0);

    assert_eq!(after.active_child_key.as_deref(), Some("index-1"));
    assert_eq!(after.children.len(), 2);
    let _ = server.await;
}

async fn service_with_responses(
    responses: Vec<Vec<u8>>,
) -> (SubscriptionService, String, tokio::task::JoinHandle<()>) {
    let (url, server) = spawn_sequence_server(responses).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.keep();
    let service = SubscriptionService::new(
        SubscriptionStore::new(path.join("subscriptions.json")),
        HwidStore::new(path.join("device-id")),
        SubscriptionHttpClient::new().unwrap(),
        "1.3.0".into(),
    );
    let added = service
        .add(AddSubscriptionInput {
            name: "Provider".into(),
            url: url.to_string(),
            interval_minutes: 60,
        })
        .await
        .unwrap();
    (service, added.subscription.id, server)
}

async fn spawn_sequence_server(responses: Vec<Vec<u8>>) -> (Url, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0_u8; 16 * 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket.write_all(&response).await.unwrap();
        }
    });
    (
        Url::parse(&format!("http://{address}/subscription")).unwrap(),
        task,
    )
}

fn xray_bundle(names: &[&str]) -> Vec<u8> {
    let children = names
        .iter()
        .map(|name| format!(r#"{{"remarks":"{name}","outbounds":[{{"protocol":"freedom"}}]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    http_response("application/json", format!("[{children}]").as_bytes())
}

fn http_response(content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
