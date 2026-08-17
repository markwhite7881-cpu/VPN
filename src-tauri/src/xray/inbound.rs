use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

pub const MANAGED_HTTP_TAG: &str = "cloakwire-managed-http";

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedHttpInbound {
    pub value: Value,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub injected: bool,
}

pub fn ensure_managed_http_inbound<F>(
    mut value: Value,
    port_allocator: F,
) -> AppResult<ManagedHttpInbound>
where
    F: FnOnce() -> AppResult<u16>,
{
    let root = value
        .as_object_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray provider config must be an object".into()))?;
    let inbounds = root
        .entry("inbounds")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| AppError::UnsafeConfig("Xray inbounds must be an array".into()))?;

    let mut candidates = Vec::new();
    for inbound in inbounds.iter() {
        let object = inbound
            .as_object()
            .ok_or_else(|| AppError::UnsafeConfig("Xray inbound must be an object".into()))?;
        if object.get("tag").and_then(Value::as_str) == Some(MANAGED_HTTP_TAG) {
            return Err(AppError::UnsafeConfig(
                "provider uses reserved Xray inbound tag".into(),
            ));
        }
        if object.get("protocol").and_then(Value::as_str) != Some("http") {
            continue;
        }
        let listen = object
            .get("listen")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::UnsafeConfig("Xray HTTP inbound has no listen address".into())
            })?;
        let port = valid_port(object.get("port"))?;
        if !is_loopback(listen) {
            return Err(AppError::UnsafeConfig(
                "Xray HTTP inbound must listen on loopback".into(),
            ));
        }
        candidates.push((listen.to_string(), port));
    }

    match candidates.len() {
        0 => {
            let port = port_allocator()?;
            if port == 0 {
                return Err(AppError::UnsafeConfig(
                    "Xray managed HTTP inbound has an invalid port".into(),
                ));
            }
            inbounds.push(json!({
                "tag": MANAGED_HTTP_TAG,
                "listen": "127.0.0.1",
                "port": port,
                "protocol": "http",
                "settings": {}
            }));
            Ok(ManagedHttpInbound {
                value,
                proxy_host: "127.0.0.1".into(),
                proxy_port: port,
                injected: true,
            })
        }
        1 => {
            let (proxy_host, proxy_port) = candidates.pop().expect("one candidate");
            Ok(ManagedHttpInbound {
                value,
                proxy_host,
                proxy_port,
                injected: false,
            })
        }
        _ => Err(AppError::UnsafeConfig(
            "Xray provider has ambiguous HTTP inbounds".into(),
        )),
    }
}

fn valid_port(value: Option<&Value>) -> AppResult<u16> {
    let port = value
        .and_then(Value::as_u64)
        .filter(|port| (1..=u16::MAX as u64).contains(port))
        .ok_or_else(|| AppError::UnsafeConfig("Xray HTTP inbound has an invalid port".into()))?;
    Ok(port as u16)
}

fn is_loopback(listen: &str) -> bool {
    matches!(listen, "127.0.0.1" | "::1" | "localhost")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ensure_managed_http_inbound;

    #[test]
    fn selects_unambiguous_loopback_http_inbound() {
        let config = json!({"inbounds":[{"tag":"local-http","listen":"127.0.0.1","port":10809,"protocol":"http"}]});

        let result = ensure_managed_http_inbound(config, || Ok(20809)).unwrap();

        assert_eq!(result.proxy_host, "127.0.0.1");
        assert_eq!(result.proxy_port, 10809);
        assert!(!result.injected);
    }

    #[test]
    fn injects_runtime_only_http_inbound_when_missing() {
        let result = ensure_managed_http_inbound(json!({"inbounds":[]}), || Ok(20809)).unwrap();

        assert_eq!(result.proxy_host, "127.0.0.1");
        assert_eq!(result.proxy_port, 20809);
        assert_eq!(result.value["inbounds"][0]["tag"], "cloakwire-managed-http");
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_http_inbounds_and_reserved_tags() {
        for config in [
            json!({"inbounds":[{"tag":"public","listen":"0.0.0.0","port":10809,"protocol":"http"}]}),
            json!({"inbounds":[{"tag":"a","listen":"127.0.0.1","port":10809,"protocol":"http"},{"tag":"b","listen":"::1","port":10810,"protocol":"http"}]}),
            json!({"inbounds":[{"tag":"bad","listen":"127.0.0.1","port":0,"protocol":"http"}]}),
            json!({"inbounds":[{"tag":"cloakwire-managed-http","listen":"127.0.0.1","port":10809,"protocol":"socks"}]}),
        ] {
            assert!(ensure_managed_http_inbound(config, || Ok(20809)).is_err());
        }
    }
}
