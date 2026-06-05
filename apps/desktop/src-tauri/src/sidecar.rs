//! Поиск исполняемых файлов yt-dlp и ffmpeg в системе.
//! Приоритет: custom path из конфига → $PATH → бинарь рядом с exe → known locations.

use std::path::{Path, PathBuf};
use which::which;

pub fn find_ytdlp(custom: Option<&str>) -> Option<PathBuf> {
    find(custom, "yt-dlp")
}

pub fn find_ffmpeg(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = custom {
        if let Some(found) = probe(Path::new(p)) {
            return Some(found);
        }
    }
    for name in ["ffmpeg", "ffmpeg.exe"] {
        if let Ok(p) = which(name) { return Some(p); }
    }
    known_location("ffmpeg.exe")
        .or_else(|| known_location("ffmpeg"))
}

fn find(custom: Option<&str>, basename: &str) -> Option<PathBuf> {
    if let Some(p) = custom {
        if let Some(found) = probe(Path::new(p)) {
            return Some(found);
        }
    }
    if let Ok(p) = which(basename) { return Some(p); }
    known_location(basename)
}

fn probe(p: &Path) -> Option<PathBuf> {
    if p.is_file() { Some(p.to_path_buf()) } else { None }
}

fn known_location(name: &str) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let candidates = [
            exe.parent().map(|p| p.join(name)),
            exe.parent().and_then(|p| p.parent()).map(|p| p.join("resources").join(name)),
        ];
        for c in candidates.into_iter().flatten() {
            if c.is_file() { return Some(c); }
        }
    }
    None
}
