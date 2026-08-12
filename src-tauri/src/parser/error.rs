//! Parser error type. Surfaces structured failures to the frontend so the
//! UI can explain what went wrong (bad base64, missing UUID, etc.).
//!
//! Serialised as `{ kind, message }` for human consumption on the frontend.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message")]
pub enum ParseError {
    #[error("input is empty")]
    Empty,

    #[error("unknown or missing scheme in '{0}…'")]
    UnknownScheme(String),

    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),

    #[error("invalid URL: {0}")]
    Url(String),

    #[error("invalid base64 payload: {0}…")]
    Base64(String),

    #[error("invalid UTF-8 in decoded payload")]
    Utf8,

    #[error("missing required field: {0}")]
    Missing(String),

    #[error("invalid value for {0}: {1}")]
    InvalidValue(String, String),

    #[error("invalid port number: {0}")]
    Port(String),

    #[error("protocol {0} does not support field {1}")]
    UnsupportedField(String, String),
}
