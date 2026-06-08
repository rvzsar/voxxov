//! Tauri-команды — публичный API для frontend.

use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::models::{self, ModelStatus};
use crate::paths;
use crate::pipeline;
use crate::state::AppState;
use crate::types::{FileInfo, Job, JobId, JobSource, MediaInfo, SidecarStatus};

#[tauri::command]
pub async fn enqueue_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> AppResult<JobId> {
    enqueue(state, JobSource::Url, url).await
}

#[tauri::command]
pub async fn enqueue_local(
    state: tauri::State<'_, AppState>,
    path: String,
) -> AppResult<JobId> {
    enqueue(state, JobSource::LocalFile, path).await
}

/// Валидация + создание Job + spawn pipeline.
async fn enqueue(
    state: tauri::State<'_, AppState>,
    source: JobSource,
    target: String,
) -> AppResult<JobId> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(AppError::InvalidUrl("empty".into()));
    }
    match source {
        JobSource::Url => {
            if !(target.starts_with("http://") || target.starts_with("https://")) {
                return Err(AppError::InvalidUrl(target));
            }
        }
        JobSource::LocalFile => {
            if !std::path::Path::new(&target).is_file() {
                return Err(AppError::InvalidUrl(format!("file not found: {target}")));
            }
        }
    }
    let job = Job::new(target, source);
    let id = job.id.clone();
    state.insert_job(job.clone());
    let cfg = state.config();
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = pipeline::run_job(st, cfg, job).await;
    });
    Ok(id)
}

#[tauri::command]
pub fn list_jobs(state: tauri::State<'_, AppState>) -> Vec<Job> {
    state.list_jobs()
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, AppState>, id: String) -> bool {
    state.cancel(&id)
}

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
pub fn save_config(
    state: tauri::State<'_, AppState>,
    cfg: AppConfig,
) -> AppResult<AppConfig> {
    config::save(&cfg)?;
    state.set_config(cfg.clone());
    Ok(cfg)
}

#[tauri::command]
pub async fn fetch_metadata(
    state: tauri::State<'_, AppState>,
    url: String,
) -> AppResult<MediaInfo> {
    crate::ytdlp::YtDlpRunner::fetch_metadata(&state, &url).await
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "aac", "wma", "opus",
    "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "ts", "mts",
];

#[tauri::command]
pub async fn scan_folder(path: String) -> AppResult<Vec<FileInfo>> {
    let dir = std::path::PathBuf::from(path.trim());
    let meta = tokio::fs::metadata(&dir)
        .await
        .map_err(|e| AppError::Other(format!("stat {}: {e}", dir.display())))?;
    if !meta.is_dir() {
        return Err(AppError::Other(format!("not a directory: {}", dir.display())));
    }
    let mut files = Vec::new();
    scan_dir_recursive(&dir, &mut files).await?;
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

async fn scan_dir_recursive(dir: &std::path::Path, out: &mut Vec<FileInfo>) -> AppResult<()> {
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| AppError::Other(format!("read dir {}: {e}", dir.display())))?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
    {
        let path = entry.path();
        let file_type = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            Box::pin(scan_dir_recursive(&path, out)).await?;
        } else if file_type.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if MEDIA_EXTENSIONS.contains(&ext_lower.as_str()) {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                    out.push(FileInfo {
                        path: path.to_string_lossy().to_string(),
                        name,
                        extension: ext_lower,
                        size_bytes: size,
                    });
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn check_model_status(model_dir: Option<String>) -> ModelStatus {
    let dir = model_dir
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models::default_model_dir);
    models::check_status(&dir)
}

#[tauri::command]
pub async fn download_model(
    state: tauri::State<'_, AppState>,
    job_id: String,
    model_dir: Option<String>,
) -> AppResult<ModelStatus> {
    let dir = model_dir
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models::default_model_dir);
    let job_id_for_log = job_id.clone();
    state.log_line(&job_id, "model: starting download");

    let result = models::download_all(&dir, |downloaded, total| {
        let pct = if total > 0 {
            (downloaded as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        tracing::info!("model: {}% ({} / {})", pct as u32, downloaded, total);
    })
    .await;

    match &result {
        Ok(_) => state.log_line(&job_id_for_log, "model: download complete"),
        Err(e) => state.log_line(&job_id_for_log, format!("model: download failed: {e}")),
    }
    result
}

#[tauri::command]
pub fn diagnose(_state: tauri::State<'_, AppState>) -> SidecarStatus {
    SidecarStatus {
        ytdlp: Some(crate::sidecar::yt_dlp_path()).filter(|p| p.is_file()),
        ffmpeg: Some(crate::sidecar::ffmpeg_path()).filter(|p| p.is_file()),
        app_data: crate::paths::data_root(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
