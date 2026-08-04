//! Tauri-команды — публичный API для frontend.

use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::pipeline;
use crate::state::AppState;
use crate::types::{FileInfo, Job, JobId, JobSource, SidecarStatus};
use std::path::{Path, PathBuf};

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
pub fn clear_done_jobs(state: tauri::State<'_, AppState>) {
    state.clear_terminal_jobs();
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

const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "aac", "wma", "opus",
    "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "ts", "mts",
];

#[tauri::command]
pub async fn scan_folder(path: String) -> AppResult<Vec<FileInfo>> {
    let dir = PathBuf::from(path.trim());
    let meta = std::fs::metadata(&dir)
        .map_err(|e| AppError::Other(format!("stat {}: {e}", dir.display())))?;
    if !meta.is_dir() {
        return Err(AppError::Other(format!("not a directory: {}", dir.display())));
    }
    // Sync I/O в blocking pool: на больших папках быстрее чем тысячи
    // `tokio::fs::metadata` с yield'ами между ними.
    tokio::task::spawn_blocking(move || -> AppResult<Vec<FileInfo>> {
        let mut files = Vec::new();
        scan_dir_recursive(&dir, &mut files)?;
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    })
    .await
    .map_err(|e| AppError::Other(format!("scan join: {e}")))?
}

fn scan_dir_recursive(dir: &std::path::Path, out: &mut Vec<FileInfo>) -> AppResult<()> {
    let rd = std::fs::read_dir(dir)
        .map_err(|e| AppError::Other(format!("read dir {}: {e}", dir.display())))?;
    for entry in rd.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            scan_dir_recursive(&path, out)?;
        } else if file_type.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if MEDIA_EXTENSIONS.contains(&ext_lower.as_str()) {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
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
pub fn diagnose(_state: tauri::State<'_, AppState>) -> SidecarStatus {
    SidecarStatus {
        ytdlp: Some(crate::sidecar::yt_dlp_path()).filter(|p| p.is_file()),
        ffmpeg: Some(crate::sidecar::ffmpeg_path()).filter(|p| p.is_file()),
        app_data: crate::paths::data_root(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        hardware: crate::hardware::detect().clone(),
    }
}

// ===== Job folder: reveal / save =====

/// Вернуть путь к папке конкретной задачи (`<data_root>/data/jobs/<job_id>/`).
/// Используется UI чтобы показать "открыть папку" / "размер".
#[tauri::command]
pub fn get_job_workdir(job_id: String) -> AppResult<String> {
    let dir = crate::paths::job_workdir(&job_id);
    if !dir.is_dir() {
        return Err(AppError::Other(format!(
            "job folder not found: {}",
            dir.display()
        )));
    }
    Ok(dir.to_string_lossy().to_string())
}

/// Скопировать всю папку задачи (видео + аудио + txt/srt/json транскрипты)
/// в `<dest_dir>/<job_id>/`. Возвращает путь к созданной папке.
#[tauri::command]
pub async fn save_job(job_id: String, dest_dir: String) -> AppResult<String> {
    let src = crate::paths::job_workdir(&job_id);
    if !src.is_dir() {
        return Err(AppError::Other(format!(
            "job folder not found: {}",
            src.display()
        )));
    }
    let dest = PathBuf::from(dest_dir.trim());
    if !dest.is_dir() {
        return Err(AppError::Other(format!(
            "destination is not a directory: {}",
            dest.display()
        )));
    }
    let target = dest.join(&job_id);
    if target.exists() {
        return Err(AppError::Other(format!(
            "destination already exists: {} (удалить вручную или переименовать)",
            target.display()
        )));
    }
    // Sync I/O в blocking pool: на больших видеофайлах копирование может
    // занимать минуты — нельзя блокировать tokio worker thread.
    let src_clone = src.clone();
    let target_clone = target.clone();
    tokio::task::spawn_blocking(move || copy_dir_recursive(&src_clone, &target_clone))
        .await
        .map_err(|e| AppError::Other(format!("copy join: {e}")))?
        .map_err(|e| AppError::Other(format!("copy {} → {}: {e}", src.display(), target.display())))?;
    Ok(target.to_string_lossy().to_string())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
