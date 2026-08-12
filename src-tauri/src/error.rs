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
            AppError::WriteConfig(_) => ("write_config", self.to_string()),
            AppError::Parse(e) => {
                // The parser already has a structured kind+message; flatten
                // both so the frontend can show specific advice.
                return e.serialize(s);
            }
            AppError::Io(_) => ("io", self.to_string()),
            AppError::Serde(_) => ("serde", self.to_string()),
            AppError::Clash(_) => ("clash", self.to_string()),
            AppError::Tauri(_) => ("tauri", self.to_string()),
        };
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", kind)?;
        st.serialize_field("message", &message)?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
