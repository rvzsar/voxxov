//! Конфиг приложения: TOML-файл в `directories::ProjectConfig`,
//! под-структуры сериализуются в camelCase, чтобы соответствовать TS.
//! Конфиг приложения: TOML-файл в `<data_root>/data/config.toml`.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self { kind: ProxyKind::None, host: None, port: None, username: None, password: None, no_proxy: None }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    None,
    Http,
    Https,
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadConfig {
    pub format: String,
    pub max_height: u32,
    pub audio_only: bool,
    pub embed_subs: bool,
    pub concurrent_fragments: u32,
    pub retries: u32,
    pub overwrite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie_file: Option<String>,
    pub user_agent: Option<String>,
    /// Префикс-зеркало для GitHub-загрузок (yt-dlp, ffmpeg, модели),
    /// напр. "https://mirror.example.com/" — для регионов с блокировками.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_prefix: Option<String>,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            format: "bv*+ba/b".to_string(),
            max_height: 1080,
            audio_only: false,
            embed_subs: false,
            concurrent_fragments: 4,
            retries: 3,
            overwrite: false,
            cookie_file: None,
            user_agent: None,
            mirror_prefix: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AsrConfig {
    /// Folder containing the 4 GigaAM files (encoder/decoder/joiner.onnx
    /// + tokens.txt). Empty = auto-download to `<data_root>/models`.
    /// `cmd:some-cli --args` = delegate to an external CLI (escape hatch).
    #[serde(alias = "modelPath", alias = "model_path")]
    pub model_dir: String,
    pub sample_rate: u32,
    pub language: String,
    pub device: AsrDevice,
    pub beam_size: u32,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            model_dir: String::new(),
            sample_rate: 16000,
            language: "ru".to_string(),
            device: AsrDevice::Cpu,
            beam_size: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AsrDevice {
    Cpu,
    Cuda,
    Directml,
    Openvino,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutputConfig {
    /// Куда дополнительно копировать транскрипты; пусто = только workdir.
    pub dir: String,
    pub formats: Vec<String>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
            formats: vec!["txt".to_string(), "srt".to_string(), "json".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingConfig {
    pub level: String,
    pub file: bool,
    pub max_size_mb: u32,
    pub keep_files: u32,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: true,
            max_size_mb: 5,
            keep_files: 3,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub proxy: ProxyConfig,
    pub download: DownloadConfig,
    pub asr: AsrConfig,
    pub output: OutputConfig,
    pub logging: LoggingConfig,
}

// ---------------- file IO ----------------

pub fn load_or_default() -> AppConfig {
    let path = crate::paths::config_path();
    load_from(&path).unwrap_or_default()
}

pub fn load_from(path: &Path) -> Option<AppConfig> {
    let text = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<AppConfig>(&text) {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::error!("config: failed to parse {}: {e}", path.display());
            None
        }
    }
}

pub fn save(cfg: &AppConfig) -> crate::error::AppResult<()> {
    let path = crate::paths::config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
