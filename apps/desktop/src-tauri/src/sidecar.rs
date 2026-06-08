//! yt-dlp and ffmpeg binaries live in `<data_root>/bin/`, next to the .exe.
//!
//! The `yt-dlp` Rust crate handles the actual download on first use — we
//! just point it at our bin dir via `Libraries::new(...)` and
//! `Downloader::with_new_binaries(...)` in `ytdlp.rs`.

use crate::paths;
use std::path::PathBuf;

pub fn bin_dir() -> PathBuf {
    paths::bin_dir()
}

pub fn yt_dlp_path() -> PathBuf {
    bin_dir().join(format!("yt-dlp{}", std::env::consts::EXE_SUFFIX))
}

pub fn ffmpeg_path() -> PathBuf {
    bin_dir().join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX))
}
