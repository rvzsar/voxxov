//! Зеркало типов frontend ↔ Rust. Все имена полей — camelCase,
//! чтобы они совпадали с TypeScript-интерфейсами без конверсий.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub type JobId = String;

/// Этапы жизненного цикла задачи. В JSON сериализуется в snake-case.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    Queued,
    FetchingMetadata,
    Downloading,
    ExtractingAudio,
    Transcribing,
    Done,
    Failed,
    Cancelled,
}

impl JobStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStage::Queued => "queued",
            JobStage::FetchingMetadata => "fetching_metadata",
            JobStage::Downloading => "downloading",
            JobStage::ExtractingAudio => "extracting_audio",
            JobStage::Transcribing => "transcribing",
            JobStage::Done => "done",
            JobStage::Failed => "failed",
            JobStage::Cancelled => "cancelled",
        }
    }
}

/// Источник задачи: URL или локальный файл.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobSource {
    Url,
    LocalFile,
}

impl Default for JobSource {
    fn default() -> Self { JobSource::Url }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub pct: f32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<String>,
}

impl Default for Progress {
    fn default() -> Self {
        Self { pct: 0.0, label: String::new(), speed: None, eta: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    pub url: String,
    #[serde(default)]
    pub source: JobSource,
    pub stage: JobStage,
    pub progress: Progress,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Job {
    pub fn new(url: String) -> Self {
        let now: DateTime<Utc> = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            url,
            source: JobSource::Url,
            stage: JobStage::Queued,
            progress: Progress::default(),
            created_at: now.to_rfc3339(),
            finished_at: None,
            media: None,
            transcript_path: None,
            transcript_preview: None,
            error: None,
        }
    }

    /// Создать задачу для локального файла.
    pub fn new_local(path: String) -> Self {
        let now: DateTime<Utc> = Utc::now();
        let name = std::path::Path::new(&path)
            .file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            url: path.clone(),
            source: JobSource::LocalFile,
            stage: JobStage::Queued,
            progress: Progress::default(),
            created_at: now.to_rfc3339(),
            finished_at: None,
            media: Some(MediaInfo {
                id: String::new(),
                url: path,
                title: name,
                uploader: None, duration_sec: None, thumbnail: None,
            }),
            transcript_path: None, transcript_preview: None, error: None,
        }
    }
}

/// Информация о локальном аудио/видео файле.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size_bytes: u64,
}

/// Patch-объект для инкрементальных обновлений Job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<JobStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// События, ретранслируемые во frontend как `job:event`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BackendEvent {
    #[serde(rename = "job:created")]
    JobCreated { job: Box<Job> },
    #[serde(rename = "job:updated")]
    JobUpdated { id: JobId, patch: JobPatch },
    #[serde(rename = "job:log")]
    JobLog { id: JobId, line: String },
    #[serde(rename = "download:progress")]
    DownloadProgress {
        id: JobId,
        pct: f32,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        speed: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        eta: Option<String>,
    },
    #[serde(rename = "job:done", rename_all = "camelCase")]
    JobDone {
        id: JobId,
        transcript_path: String,
        preview: String,
    },
    #[serde(rename = "job:failed", rename_all = "camelCase")]
    JobFailed { id: JobId, error: String },
}

/// Информация о доступных sidecar'ах — для команды `diagnose`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    pub ytdlp: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub app_data: PathBuf,
    pub app_version: String,
}
