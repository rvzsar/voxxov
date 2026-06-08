//! Direct subprocess wrapper for `yt-dlp.exe`.
//!
//! No Rust wrapper crate — we shell out and parse JSON ourselves with
//! `#[serde(default)]` on every field, so upstream schema changes don't
//! break us. Same approach the Python yt-dlp ecosystem has used forever.
//!
//! First run: `yt-dlp.exe` and `ffmpeg.exe` are downloaded to
//! `<data_root>/bin/` from official sources:
//! - yt-dlp: <https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe>
//! - ffmpeg: BtbN's `ffmpeg-master-latest-win64-gpl.zip` (GPL, no extra deps)

use crate::config::{AppConfig, DownloadConfig};
use crate::error::{AppError, AppResult};
use crate::proxy;
use crate::state::AppState;
use crate::types::{BackendEvent, MediaInfo};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

const YT_DLP_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
const HTTP_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const METADATA_TIMEOUT: Duration = Duration::from_secs(60);

pub struct YtDlpRunner;

impl YtDlpRunner {
    /// Ensure `yt-dlp.exe` + `ffmpeg.exe` are present (downloads if missing).
    /// Called eagerly at startup so the first user action doesn't wait
    /// for a 20MB download.
    pub async fn preflight() -> AppResult<()> {
        ensure_ytdlp().await?;
        ensure_ffmpeg().await?;
        Ok(())
    }

    /// Fetch video metadata via `yt-dlp --dump-json`. One-shot, no progress.
    pub async fn fetch_metadata(url: &str) -> AppResult<MediaInfo> {
        ensure_ytdlp().await?;

        let mut cmd = Command::new(crate::sidecar::yt_dlp_path());
        cmd.arg("--dump-json")
            .arg("--no-warnings")
            .arg("--no-playlist")
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(METADATA_TIMEOUT, cmd.output())
            .await
            .map_err(|_| AppError::YtDlp("metadata fetch timeout (60s)".into()))?
            .map_err(|e| AppError::YtDlp(format!("spawn yt-dlp: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::YtDlp(format!(
                "yt-dlp exited {}: {stderr}",
                output.status
            )));
        }

        // All fields #[serde(default)] on the struct → missing field is
        // never a hard error, even on a brand-new yt-dlp version.
        let video: VideoJson = serde_json::from_slice(&output.stdout)
            .map_err(|e| AppError::YtDlp(format!("parse json: {e}")))?;

        Ok(MediaInfo {
            id: video.id.unwrap_or_default(),
            url: video
                .webpage_url
                .or(video.url)
                .unwrap_or_else(|| url.to_string()),
            title: video.title.unwrap_or_else(|| "Unknown".into()),
            uploader: video.uploader.or(video.channel).or(video.uploader_id),
            duration_sec: video.duration.map(|d| d as u64),
            thumbnail: video.thumbnail,
        })
    }

    /// Download video to `out_dir/source.<ext>`. Streams progress events.
    /// Cancellation aborts the subprocess.
    pub async fn download(
        state: &AppState,
        job_id: &str,
        url: &str,
        out_dir: &Path,
        cfg: &AppConfig,
        cancel: CancellationToken,
    ) -> AppResult<PathBuf> {
        ensure_ytdlp().await?;
        std::fs::create_dir_all(out_dir).map_err(AppError::Io)?;

        let output_template = out_dir.join("source.%(ext)s");

        let mut cmd = Command::new(crate::sidecar::yt_dlp_path());
        cmd.arg("--no-mtime")
            .arg("--no-warnings")
            .arg("--newline")
            .arg("--retries")
            .arg(cfg.download.retries.to_string())
            .arg("--concurrent-fragments")
            .arg(cfg.download.concurrent_fragments.to_string());

        for arg in build_args(&cfg.download) {
            cmd.arg(arg);
        }
        for arg in proxy::to_args(&cfg.proxy) {
            cmd.arg(arg);
        }

        cmd.arg("-o")
            .arg(&output_template)
            .arg("--print")
            .arg("after_move:filepath")
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::YtDlp(format!("spawn yt-dlp: {e}")))?;

        // Stream stderr for progress. stdout is read after the child exits
        // (--print after_move:filepath outputs just one line at the end,
        // so the pipe buffer never fills).
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::YtDlp("no stderr".into()))?;

        let state_for_events = state.clone();
        let job_id_owned = job_id.to_string();
        let events_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                forward_progress(&state_for_events, &job_id_owned, &line);
            }
        });

        let status = tokio::select! {
            res = child.wait() => res.map_err(|e| AppError::YtDlp(format!("wait yt-dlp: {e}"))),
            _ = cancel.cancelled() => {
                let _ = child.start_kill();
                events_task.abort();
                return Err(AppError::Cancelled);
            }
        };
        events_task.abort();
        let status = status?;

        if !status.success() {
            return Err(AppError::YtDlp(format!("yt-dlp exited {status}")));
        }

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::YtDlp("no stdout".into()))?;
        let mut buf = String::new();
        stdout
            .read_to_string(&mut buf)
            .await
            .map_err(|e| AppError::YtDlp(format!("read stdout: {e}")))?;
        let filepath = buf
            .lines()
            .last()
            .ok_or_else(|| AppError::YtDlp("no filepath in stdout".into()))?;
        let path = PathBuf::from(filepath);
        if !path.is_file() {
            return Err(AppError::YtDlp(format!(
                "downloaded file not found at {}",
                path.display()
            )));
        }
        Ok(path)
    }
}

// --- JSON model ---
// Every field is Option + #[serde(default)] on the struct → a new yt-dlp
// version can add/remove/rename fields without breaking our parser.

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct VideoJson {
    id: Option<String>,
    url: Option<String>,
    webpage_url: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    uploader_id: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
}

// --- CLI arg builder ---

fn build_args(dl: &DownloadConfig) -> Vec<String> {
    let mut args = vec![
        "--retries".to_string(),
        dl.retries.to_string(),
        "--concurrent-fragments".to_string(),
        dl.concurrent_fragments.to_string(),
    ];
    if dl.audio_only {
        args.push("-x".to_string());
        args.push("--audio-format".to_string());
        args.push("wav".to_string());
    } else {
        if !dl.format.is_empty() {
            args.push("-f".to_string());
            args.push(dl.format.clone());
        } else {
            args.push("-f".to_string());
            args.push("bv*+ba/b".to_string());
        }
        if dl.max_height > 0 {
            args.push("-S".to_string());
            args.push(format!("res:{}", dl.max_height));
        }
        if dl.embed_subs {
            args.push("--embed-subs".to_string());
        }
    }
    args.push(
        if dl.overwrite {
            "--force-overwrites"
        } else {
            "--no-overwrites"
        }
        .to_string(),
    );
    if let Some(ua) = dl.user_agent.as_deref() {
        if !ua.is_empty() {
            args.push("--user-agent".to_string());
            args.push(ua.to_string());
        }
    }
    args
}

// --- stderr parsing ---

fn forward_progress(state: &AppState, job_id: &str, line: &str) {
    if let Some(pct) = parse_download_pct(line) {
        let _ = state.events.send(BackendEvent::DownloadProgress {
            id: job_id.to_string(),
            pct,
            label: "Загрузка".to_string(),
            speed: None,
            eta: None,
        });
    }
    if line.contains("ERROR:") {
        state.log_line(job_id, format!("yt-dlp: {line}"));
    }
    debug!("yt-dlp stderr: {line}");
}

fn parse_download_pct(line: &str) -> Option<f32> {
    let idx = line.find("[download]")?;
    let after = line[idx + 10..].trim_start();
    let end = after.find('%')?;
    after[..end]
        .trim()
        .parse::<f32>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}

// --- auto-download ---

async fn ensure_ytdlp() -> AppResult<PathBuf> {
    let path = crate::sidecar::yt_dlp_path();
    if path.is_file() {
        return Ok(path);
    }
    info!("yt-dlp: downloading from {YT_DLP_URL}");
    download_to(YT_DLP_URL, &path).await?;
    info!("yt-dlp: ready at {}", path.display());
    Ok(path)
}

async fn ensure_ffmpeg() -> AppResult<PathBuf> {
    let path = crate::sidecar::ffmpeg_path();
    if path.is_file() {
        return Ok(path);
    }
    info!("ffmpeg: downloading from {FFMPEG_URL}");
    download_ffmpeg_zip().await?;
    if !path.is_file() {
        return Err(AppError::YtDlp(format!(
            "ffmpeg.exe not found in downloaded archive (expected at {})",
            path.display()
        )));
    }
    info!("ffmpeg: ready at {}", path.display());
    Ok(path)
}

async fn download_to(url: &str, target: &Path) -> AppResult<()> {
    let client = http_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::YtDlp(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::YtDlp(format!("GET {url}: HTTP {}", resp.status())));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::YtDlp(format!("read body {url}: {e}")))?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::Io)?;
    }
    std::fs::write(target, &bytes).map_err(AppError::Io)?;
    Ok(())
}

async fn download_ffmpeg_zip() -> AppResult<()> {
    let client = http_client()?;
    let resp = client
        .get(FFMPEG_URL)
        .send()
        .await
        .map_err(|e| AppError::YtDlp(format!("GET {FFMPEG_URL}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::YtDlp(format!(
            "GET {FFMPEG_URL}: HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::YtDlp(format!("read body: {e}")))?;

    let bin_dir = crate::sidecar::bin_dir();
    std::fs::create_dir_all(&bin_dir).map_err(AppError::Io)?;

    // BtbN's zip layout: ffmpeg-master-latest-win64-gpl/bin/{ffmpeg,ffprobe}.exe
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| AppError::YtDlp(format!("open zip: {e}")))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::YtDlp(format!("zip entry {i}: {e}")))?;
        let name = file.name().to_string();
        let basename = name.rsplit('/').next().unwrap_or(&name);
        if basename == "ffmpeg.exe" || basename == "ffprobe.exe" {
            let target = bin_dir.join(basename);
            let mut out = std::fs::File::create(&target)
                .map_err(|e| AppError::YtDlp(format!("create {}: {e}", target.display())))?;
            std::io::copy(&mut file, &mut out)
                .map_err(|e| AppError::YtDlp(format!("extract {}: {e}", target.display())))?;
            info!("ffmpeg: extracted {}", target.display());
        }
    }
    Ok(())
}

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::YtDlp(format!("http client: {e}")))
}
