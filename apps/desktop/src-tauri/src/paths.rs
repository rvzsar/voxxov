//! Пути приложения: конфиг, логи, кэш, downloads, transcripts.
//! Все дефолты указывают на `directories::ProjectConfig`.

use std::path::PathBuf;
use tauri::Manager;

const APP_DIR: &str = "GigaAM";

fn base(app: Option<&tauri::AppHandle>) -> PathBuf {
    if let Some(handle) = app {
        if let Ok(d) = handle.path().app_config_dir() {
            return d;
        }
    }
    directories::ProjectDirs::from("dev", "salute", APP_DIR)
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(".gigaam"))
}

pub fn app_root(app: Option<&tauri::AppHandle>) -> PathBuf { base(app) }

pub fn config_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app) }

pub fn logs_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app).join("logs") }

pub fn cache_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app).join("cache") }

pub fn downloads_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app).join("downloads") }

pub fn transcripts_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app).join("transcripts") }

pub fn jobs_dir(app: Option<&tauri::AppHandle>) -> PathBuf { base(app).join("jobs") }

pub fn ensure_all(app: &tauri::AppHandle) -> std::io::Result<()> {
    for d in [config_dir(Some(app)), logs_dir(Some(app)), cache_dir(Some(app)),
              downloads_dir(Some(app)), transcripts_dir(Some(app)), jobs_dir(Some(app))] {
        std::fs::create_dir_all(d)?;
    }
    Ok(())
}

pub fn job_workdir(app: &tauri::AppHandle, job_id: &str) -> PathBuf {
    jobs_dir(Some(app)).join(job_id)
}
