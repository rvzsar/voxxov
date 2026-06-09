//! Единый тип ошибки приложения, сериализуемый в строку для Tauri-команд.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml decode: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml encode: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("yt-dlp: {0}")]
    YtDlp(String),

    #[error("ffmpeg: {0}")]
    Ffmpeg(String),

    #[error("sidecar not found: {0}")]
    Sidecar(String),

    #[error("proxy: {0}")]
    Proxy(String),

    #[error("job: {0}")]
    Job(String),

    #[error("asr: {0}")]
    Asr(String),

    #[error("cancelled by user")]
    Cancelled,

    #[error("invalid url: {0}")]
    InvalidUrl(String),

    #[error("{0}")]
    Other(String),
}

/// Tauri сериализует `Result<T, E>` в JSON. Отдаём строку — проще и
/// фронту достаточно (он использует только `.message` через `Error.message`).
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(format!("{e:#}"))
    }
}
