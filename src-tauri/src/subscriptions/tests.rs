use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

use crate::error::AppError;

use super::http::SubscriptionHttpClient;

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
