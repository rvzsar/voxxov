//! Tauri-команды — публичный API для frontend.

use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::pipeline;
use crate::state::AppState;
use crate::types::{FileInfo, Job, JobId, MediaInfo, SidecarStatus};
use tauri::Manager;

#[tauri::command]
pub async fn enqueue_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> AppResult<JobId> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(AppError::InvalidUrl("empty".into()));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::InvalidUrl(url));
    }
    let job = Job::new(url.clone());
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
                if MEDIA_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let size = entry
                        .metadata()
                        .await
                        .map(|m| m.len())
                        .unwrap_or(0);
                    out.push(FileInfo {
                        path: path.to_string_lossy().to_string(),
                        name,
                        extension: ext.to_lowercase(),
                        size_bytes: size,
                    });
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn enqueue_local(
    state: tauri::State<'_, AppState>,
    path: String,
) -> AppResult<JobId> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err(AppError::InvalidUrl("empty path".into()));
    }
    if !std::path::Path::new(&path).is_file() {
        return Err(AppError::InvalidUrl(format!("file not found: {}", path)));
    }
    let job = Job::new_local(path);
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
pub fn diagnose(state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> SidecarStatus {
    let cfg = state.config();
    let bin_dir = crate::sidecar::bin_dir(Some(&app));
    SidecarStatus {
        ytdlp: Some(crate::sidecar::yt_dlp_path(Some(&app)))
            .filter(|p| p.is_file()),
        ffmpeg: Some(crate::sidecar::ffmpeg_path(Some(&app)))
            .filter(|p| p.is_file()),
        app_data: paths::config_dir(Some(&app)),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
