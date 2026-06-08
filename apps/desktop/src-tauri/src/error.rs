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

impl AppError {
    /// Краткий стабильный код ошибки для UI.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::TomlDe(_) | AppError::TomlSer(_) => "toml",
            AppError::Json(_) => "json",
            AppError::YtDlp(_) => "ytdlp",
            AppError::Ffmpeg(_) => "ffmpeg",
            AppError::Sidecar(_) => "sidecar",
            AppError::Proxy(_) => "proxy",
            AppError::Job(_) => "job",
            AppError::Asr(_) => "asr",
            AppError::Cancelled => "cancelled",
            AppError::InvalidUrl(_) => "invalid_url",
            AppError::Other(_) => "other",
        }
    }
}

/// Tauri ожидает, что `Result<T, E>` сериализуется в JSON.
/// Отдаём строку-сообщение + стабильный код в объекте.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(format!("{e:#}"))
    }
}
