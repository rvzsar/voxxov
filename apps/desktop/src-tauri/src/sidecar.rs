//! yt-dlp and ffmpeg binaries live in `<data_root>/bin/`, next to the .exe.

use crate::paths;
use std::path::PathBuf;

/// Не показывать консольное окно дочернего процесса (Windows).
/// yt-dlp/ffmpeg — консольные exe: без этого флага они открывают
/// собственное окно консоли при каждом запуске.
pub(crate) fn hide_console(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

pub fn bin_dir() -> PathBuf {
    paths::bin_dir()
}

pub fn yt_dlp_path() -> PathBuf {
    bin_dir().join(format!("yt-dlp{}", std::env::consts::EXE_SUFFIX))
}

pub fn ffmpeg_path() -> PathBuf {
    bin_dir().join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX))
}
