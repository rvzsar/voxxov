//! yt-dlp and ffmpeg binaries live in `<data_root>/bin/`, next to the .exe.

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
