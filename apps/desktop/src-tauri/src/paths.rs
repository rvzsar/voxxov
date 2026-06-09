//! All app paths are next to the .exe (truly portable).
//!
//! Layout relative to the .exe:
//!
//! ```text
//! voxxov.exe
//! ├── data/                  ← mutable state
//! │   ├── config.toml
//! │   ├── logs/app.log
//! │   ├── cache/
//! │   ├── downloads/         ← yt-dlp temp outputs
//! │   ├── jobs/<job_id>/
//! │   └── transcripts/<title>.{txt,srt,json}
//! ├── models/                ← ASR model files (auto-discovered)
//! └── bin/                   ← yt-dlp.exe, ffmpeg.exe (auto-downloaded)
//! ```
//!
//! No fallback to `%APPDATA%` — if the .exe lives in a read-only
//! location, `init_data_root` panics with a clear error message at startup.

use std::path::PathBuf;
use std::sync::OnceLock;

const DATA_SUBDIR: &str = "data";
const MODELS_SUBDIR: &str = "models";
const BIN_SUBDIR: &str = "bin";

static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Initialize the data root. Must be called exactly once at startup
/// (in `lib.rs::run`) before any other paths function. Panics if the
/// directory cannot be determined or created.
pub fn init_data_root() {
    let _ = DATA_ROOT.set(resolve_data_root());
}

fn resolve_data_root() -> PathBuf {
    let root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    if let Err(e) = std::fs::create_dir_all(&root) {
        panic!(
            "GigaAM: cannot create data root {}: {e}. \
             Move the .exe to a writable folder.",
            root.display()
        );
    }
    root
}

/// Override the data root (for tests only). Must be called before any
/// other paths function.
#[cfg(test)]
pub fn set_data_root_for_test(path: PathBuf) {
    let _ = DATA_ROOT.set(path);
}

pub fn data_root() -> PathBuf {
    DATA_ROOT
        .get()
        .cloned()
        .expect("paths::init_data_root() must be called before any paths function")
}

fn data() -> PathBuf {
    data_root().join(DATA_SUBDIR)
}

pub fn config_path() -> PathBuf {
    data().join("config.toml")
}
pub fn logs_dir() -> PathBuf {
    data().join("logs")
}
pub fn cache_dir() -> PathBuf {
    data().join("cache")
}
pub fn downloads_dir() -> PathBuf {
    data().join("downloads")
}
pub fn transcripts_dir() -> PathBuf {
    data().join("transcripts")
}
pub fn jobs_dir() -> PathBuf {
    data().join("jobs")
}
pub fn model_dir() -> PathBuf {
    data_root().join(MODELS_SUBDIR)
}
pub fn bin_dir() -> PathBuf {
    data_root().join(BIN_SUBDIR)
}

pub fn job_workdir(job_id: &str) -> PathBuf {
    jobs_dir().join(job_id)
}

pub fn ensure_all() -> std::io::Result<()> {
    for d in [
        data(),
        logs_dir(),
        cache_dir(),
        downloads_dir(),
        transcripts_dir(),
        jobs_dir(),
        model_dir(),
        bin_dir(),
    ] {
        std::fs::create_dir_all(d)?;
    }
    Ok(())
}
