//! Tauri-команды — публичный API для frontend.

use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::pipeline;
use crate::sidecar;
use crate::state::AppState;
use crate::types::{Job, JobId, MediaInfo, SidecarStatus};
use crate::ytdlp::YtDlpRunner;
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
    let cfg = state.config();
    let ytdlp = YtDlpRunner::resolve(&cfg)?;
    ytdlp.fetch_metadata(&url).await
}

#[tauri::command]
pub fn diagnose(app: tauri::AppHandle) -> SidecarStatus {
    let cfg = crate::config::load_or_default();
    SidecarStatus {
        ytdlp: sidecar::find_ytdlp(cfg.download.custom_ytdlp_path.as_deref()),
        ffmpeg: sidecar::find_ffmpeg(cfg.download.custom_ffmpeg_path.as_deref()),
        app_data: paths::config_dir(Some(&app)),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
