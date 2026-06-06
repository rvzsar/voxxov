//! Где хранить бинари yt-dlp и ffmpeg.
//!
//! В режиме разработки (cargo tauri dev) крейт `yt-dlp` сам скачивает
//! их при первом запуске в `$APPDATA/GigaAM/bin/`. В релизе (tauri build)
//! можно либо положить их в Tauri resources, либо положиться на
//! auto-install через крейт (что и происходит сейчас).

use std::path::PathBuf;

/// Директория для yt-dlp + ffmpeg.
/// `None` означает «использовать автоопределение» (через `tauri::path`).
pub fn bin_dir(app: Option<&tauri::AppHandle>) -> PathBuf {
    if let Some(handle) = app {
        if let Ok(d) = handle.path().app_data_dir() {
            return d.join("bin");
        }
    }
    // Fallback для dev/тестов: ~/.config/GigaAM/bin
    directories::ProjectDirs::from("dev", "salute", "GigaAM")
        .map(|p| p.config_dir().parent().unwrap_or(p.config_dir()).join("bin"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(".gigaam/bin"))
}

pub fn yt_dlp_path(app: Option<&tauri::AppHandle>) -> PathBuf {
    let name = format!("yt-dlp{}", std::env::consts::EXE_SUFFIX);
    bin_dir(app).join(name)
}

pub fn ffmpeg_path(app: Option<&tauri::AppHandle>) -> PathBuf {
    let name = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    bin_dir(app).join(name)
}
