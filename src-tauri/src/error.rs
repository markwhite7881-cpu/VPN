//! Application-wide error type.
//!
//! Used in Tauri command results so the frontend gets a structured
//! `{ kind, message }` instead of a flat string.

use serde::Serialize;
use thiserror::Error;

use crate::parser;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("sing-box is not running")]
    NotRunning,

    #[error("sing-box is already running (pid {0})")]
    AlreadyRunning(u32),

    #[error("failed to locate sing-box binary: {0}")]
    BinaryNotFound(String),

    #[error("failed to spawn sing-box: {0}")]
    Spawn(String),

    /// The sing-box binary is missing the Linux capabilities it needs
    /// for TUN mode (`cap_net_admin` + `cap_net_raw`). The message
    /// already contains the exact `setcap` command the user can run
    /// to recover, so the frontend just needs to surface it.
    #[error("sing-box missing TUN capabilities: {0}")]
    TunCapabilities(String),

    #[error("failed to write config file: {0}")]
    WriteConfig(String),

    #[error("parse error: {0}")]
    Parse(#[from] parser::ParseError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("clash api error: {0}")]
    Clash(String),

    /// HTTP / network failure (sing-box update check, etc.).
    /// `io::Error` doesn't carry enough context for HTTP-level
    /// errors like "GitHub returned 403", so this exists as a
    /// distinct string variant.
    #[error("network error: {0}")]
    Network(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("subscription error: {0}")]
    Subscription(String),

    #[error("subscription authentication failed: {0}")]
    SubscriptionAuth(String),

    #[error("subscription expired: {0}")]
    SubscriptionExpired(String),

    #[error("subscription device limit reached: {0}")]
    DeviceLimit(String),

    #[error("subscription payload is too large")]
    PayloadTooLarge,

    #[error("unsafe subscription redirect: {0}")]
    UnsafeRedirect(String),

    #[error("ambiguous subscription config: {0}")]
    AmbiguousConfig(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("engine unavailable: {0}")]
    EngineUnavailable(String),

    #[error("unsafe config: {0}")]
    UnsafeConfig(String),

    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (kind, message) = match self {
            AppError::NotRunning => ("not_running", self.to_string()),
            AppError::AlreadyRunning(_) => ("already_running", self.to_string()),
            AppError::BinaryNotFound(_) => ("binary_not_found", self.to_string()),
            AppError::Spawn(_) => ("spawn", self.to_string()),
            AppError::TunCapabilities(_) => ("tun_capabilities", self.to_string()),
            AppError::WriteConfig(_) => ("write_config", self.to_string()),
            AppError::Parse(e) => {
                // The parser already has a structured kind+message; flatten
                // both so the frontend can show specific advice.
                return e.serialize(s);
            }
            AppError::Io(_) => ("io", self.to_string()),
            AppError::Serde(_) => ("serde", self.to_string()),
            AppError::Clash(_) => ("clash", self.to_string()),
            AppError::Network(_) => ("network", self.to_string()),
            AppError::Unsupported(_) => ("unsupported", self.to_string()),
            AppError::Subscription(_) => ("subscription", self.to_string()),
            AppError::SubscriptionAuth(_) => ("subscription_auth", self.to_string()),
            AppError::SubscriptionExpired(_) => ("subscription_expired", self.to_string()),
            AppError::DeviceLimit(_) => ("device_limit", self.to_string()),
            AppError::PayloadTooLarge => ("payload_too_large", self.to_string()),
            AppError::UnsafeRedirect(_) => ("unsafe_redirect", self.to_string()),
            AppError::AmbiguousConfig(_) => ("ambiguous_config", self.to_string()),
            AppError::Validation(_) => ("validation", self.to_string()),
            AppError::EngineUnavailable(_) => ("engine_unavailable", self.to_string()),
            AppError::UnsafeConfig(_) => ("unsafe_config", self.to_string()),
            AppError::Tauri(_) => ("tauri", self.to_string()),
        };
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", kind)?;
        st.serialize_field("message", &message)?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::AppError;
    use serde_json::json;

    #[test]
    fn serializes_unsupported_autostart_error() {
        assert_eq!(
            serde_json::to_value(AppError::Unsupported("autostart".into())).unwrap(),
            json!({
                "kind": "unsupported",
                "message": "unsupported: autostart",
            })
        );
    }

    #[test]
    fn serializes_subscription_error_kinds_exactly() {
        let cases = [
            (AppError::Subscription("failed".into()), "subscription"),
            (
                AppError::SubscriptionAuth("denied".into()),
                "subscription_auth",
            ),
            (
                AppError::SubscriptionExpired("expired".into()),
                "subscription_expired",
            ),
            (AppError::DeviceLimit("limit".into()), "device_limit"),
            (AppError::PayloadTooLarge, "payload_too_large"),
            (
                AppError::UnsafeRedirect("redirect".into()),
                "unsafe_redirect",
            ),
            (
                AppError::AmbiguousConfig("ambiguous".into()),
                "ambiguous_config",
            ),
            (AppError::Validation("invalid".into()), "validation"),
            (
                AppError::EngineUnavailable("missing".into()),
                "engine_unavailable",
            ),
            (AppError::UnsafeConfig("unsafe".into()), "unsafe_config"),
        ];

        for (error, expected_kind) in cases {
            let value = serde_json::to_value(error).unwrap();
            assert_eq!(value["kind"], expected_kind);
            assert!(value["message"].is_string());
        }
    }
}
