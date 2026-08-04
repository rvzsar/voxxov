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
use crate::types::{JobUpdate, MediaInfo, Progress};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

const YT_DLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
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
        let mut tracker = DownloadTracker::new();
        let events_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                forward_progress(&state_for_events, &job_id_owned, &line, &mut tracker);
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
    if let Some(cf) = dl.cookie_file.as_deref() {
        if !cf.is_empty() {
            args.push("--cookies".to_string());
            args.push(cf.to_string());
        }
    }
    if let Some(ua) = dl.user_agent.as_deref() {
        if !ua.is_empty() {
            args.push("--user-agent".to_string());
            args.push(ua.to_string());
        }
    }
    args
}

// --- stderr parsing ---

/// Отслеживание прогресса много-потокового скачивания.
///
/// yt-dlp с `bv*+ba/b` качает несколько потоков подряд (видео, затем аудио),
/// каждый со своим 0..100% и размером. Общий прогресс — отношение скачанных
/// байт к сумме размеров всех потоков: он монотонный, а скорость/ETA всегда
/// актуальны (в отличие от «последней строки», где после 100% видео идёт
/// 0% аудио).
struct DownloadTracker {
    /// Байты, скачанные завершёнными потоками.
    base_bytes: f64,
    /// Размер текущего потока (0, пока yt-dlp его не сообщил).
    cur_size: f64,
    /// Прогресс текущего потока 0..1.
    cur_pct: f32,
    /// Предыдущее значение «скачано байт» + момент — для своей скорости.
    prev_done: f64,
    prev_at: Instant,
    /// Тип текущего потока — для label («видео» / «аудио»).
    current_is_audio: bool,
}

impl DownloadTracker {
    fn new() -> Self {
        Self {
            base_bytes: 0.0,
            cur_size: 0.0,
            cur_pct: 0.0,
            prev_done: 0.0,
            prev_at: Instant::now(),
            current_is_audio: false,
        }
    }

    /// `[download] Destination: <path>` — yt-dlp начал новый поток:
    /// предыдущий считается завершённым (по факту, а не по 100%).
    fn on_new_stream(&mut self, dest: &str) {
        self.base_bytes += self.cur_size * self.cur_pct as f64;
        self.cur_size = 0.0;
        self.cur_pct = 0.0;
        self.current_is_audio = is_audio_ext(dest);
    }

    fn current_stream_is_audio(&self) -> bool {
        self.current_is_audio
    }

    fn total_bytes(&self) -> f64 {
        self.base_bytes + self.cur_size
    }

    fn done_bytes(&self) -> f64 {
        self.base_bytes + self.cur_size * self.cur_pct as f64
    }

    /// Обновить состояние по строке прогресса.
    /// Возвращает (общий pct 0..1, собственная скорость B/s — если есть).
    fn on_progress(&mut self, pct: f32, size: f64, now: Instant) -> (f32, Option<f64>) {
        if size > 0.0 {
            self.cur_size = size;
        }
        self.cur_pct = pct.max(self.cur_pct);
        let done = self.done_bytes();
        let total = self.total_bytes();
        let overall = if total > 0.0 {
            (done / total).clamp(0.0, 1.0) as f32
        } else {
            self.cur_pct
        };
        // Собственная скорость: дельта байт между строками. Пригодится,
        // когда yt-dlp шлёт «Unknown B/s» (троттлинг, фрагменты).
        let mut own_speed = None;
        let dt = now.duration_since(self.prev_at).as_secs_f64();
        if dt >= 0.2 {
            let d = done - self.prev_done;
            if d > 0.0 {
                own_speed = Some(d / dt);
            }
        }
        self.prev_done = done;
        self.prev_at = now;
        (overall, own_speed)
    }
}

fn is_audio_ext(dest: &str) -> bool {
    let lower = dest.to_lowercase();
    ["m4a", "mp3", "opus", "aac", "flac", "wav", "ogg"]
        .iter()
        .any(|e| lower.ends_with(e))
}

fn fmt_speed(bps: f64) -> String {
    if bps < 1024.0 {
        format!("{bps:.0} B/s")
    } else if bps < 1024.0 * 1024.0 {
        format!("{:.1} KiB/s", bps / 1024.0)
    } else if bps < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MiB/s", bps / 1024.0 / 1024.0)
    } else {
        format!("{:.1} GiB/s", bps / 1024.0 / 1024.0 / 1024.0)
    }
}

fn fmt_eta(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn forward_progress(state: &AppState, job_id: &str, line: &str, tracker: &mut DownloadTracker) {
    // Новый поток: yt-dlp печатает `[download] Destination: <path>` перед
    // каждым файлом. Переключаем трекер; прогресс-строку UI увидит уже
    // с новым label и перекалиброванным общим pct.
    if let Some(dest) = line.strip_prefix("[download] Destination: ") {
        tracker.on_new_stream(dest);
        return;
    }

    // Склейка потоков ffmpeg'ом: прогресса нет, меняем только label.
    if line.starts_with("[Merger]") {
        state.update_job(
            job_id,
            JobUpdate {
                progress: Some(Progress {
                    pct: 1.0,
                    label: "Объединяем потоки…".to_string(),
                    speed: None,
                    eta: None,
                }),
                ..Default::default()
            },
        );
        return;
    }

    let Some(prog) = parse_download_progress(line) else {
        if line.contains("ERROR:") {
            state.log_line(job_id, format!("yt-dlp: {line}"));
        }
        debug!("yt-dlp stderr: {line}");
        return;
    };

    let now = Instant::now();
    let (overall, own_speed) = tracker.on_progress(prog.pct, prog.size_bytes.unwrap_or(0.0), now);

    let mut label = format!(
        "Загрузка · {}",
        if tracker.current_stream_is_audio() {
            "аудио"
        } else {
            "видео"
        }
    );
    if let Some(f) = prog.fragment.as_deref() {
        label.push_str(&format!(" · frag {f}"));
    }

    // Скорость: yt-dlp, либо наша оценка по байтам («Unknown B/s»).
    let speed = prog
        .speed
        .or_else(|| own_speed.map(fmt_speed));
    // ETA: yt-dlp (кроме "Unknown"), либо из нашей скорости по остатку.
    let eta = prog.eta.filter(|e| e != "Unknown").or_else(|| {
        let s = own_speed?;
        let remaining = tracker.total_bytes() - tracker.done_bytes();
        if remaining <= 0.0 {
            return None;
        }
        Some(fmt_eta(remaining / s))
    });

    state.update_job(
        job_id,
        JobUpdate {
            progress: Some(Progress {
                pct: overall,
                label,
                speed,
                eta,
            }),
            ..Default::default()
        },
    );
    debug!("yt-dlp stderr: {line}");
}

/// Распарсить одну stderr-строку yt-dlp. Форматы:
/// `[download]  42.3% of   100.00MiB at    5.20MiB/s ETA 00:12`
/// `[download]  42.3% of ~  100.00MiB at    5.20MiB/s ETA 00:12 (frag 5/12)`
/// `[download] 100% of   100.00MiB in 00:19`  ← без speed/ETA
/// `Unknown B/s` и `ETA Unknown` пропускаются (ещё не известно на этом проходе).
struct DownloadProgress {
    pct: f32,
    /// Размер текущего потока в байтах (`of X.XXMiB` / `of ~X.XXMiB`).
    size_bytes: Option<f64>,
    speed: Option<String>,
    eta: Option<String>,
    fragment: Option<String>,
}

fn parse_download_progress(line: &str) -> Option<DownloadProgress> {
    let idx = line.find("[download]")?;
    let after = &line[idx + 10..];

    // Percentage: число прямо перед '%'.
    let pct_end = after.find('%')?;
    let pct_str = after[..pct_end].trim().split_whitespace().next()?;
    let pct_num: f32 = pct_str.parse().ok()?;

    let mut out = DownloadProgress {
        pct: (pct_num / 100.0).clamp(0.0, 1.0),
        size_bytes: parse_total_bytes(after),
        speed: None,
        eta: None,
        fragment: None,
    };

    // Speed: "at <rate>MiB/s" (или KiB/s, B/s). Берём токен после "at ".
    // Пропускаем "Unknown" (ещё не определено yt-dlp'ом).
    if let Some(at_pos) = after.find("at ") {
        let after_at = &after[at_pos + 3..];
        let end = after_at
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(after_at.len());
        let s = after_at[..end].trim();
        if !s.is_empty() && !s.starts_with("Unknown") {
            out.speed = Some(s.to_string());
        }
    }

    // ETA: "ETA <mm:ss>" или "ETA --" (skip).
    if let Some(eta_pos) = after.find("ETA ") {
        let after_eta = &after[eta_pos + 4..];
        let end = after_eta
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(after_eta.len());
        let e = after_eta[..end].trim();
        if !e.is_empty() && e != "--" {
            out.eta = Some(e.to_string());
        }
    }

    // Fragment: "(frag N/M)" в HLS/DASH-стримах.
    if let Some(frag_pos) = after.find("(frag ") {
        let after_frag = &after[frag_pos + 6..];
        if let Some(paren_end) = after_frag.find(')') {
            let f = after_frag[..paren_end].trim();
            if !f.is_empty() {
                out.fragment = Some(f.to_string());
            }
        }
    }

    Some(out)
}

/// Размер потока из `of 100.00MiB` / `of ~ 100.00MiB` (байты).
/// `None`, если размер неизвестен (шаблон `downloaded_bytes` без total).
fn parse_total_bytes(after: &str) -> Option<f64> {
    let pos = after.find("of ")?;
    let mut s = after[pos + 3..].trim_start();
    if let Some(rest) = s.strip_prefix('~') {
        s = rest.trim_start();
    }
    let token: String = s.chars().take_while(|c| !c.is_whitespace()).collect();
    if token.is_empty() {
        return None;
    }
    let num_end = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(token.len());
    let num: f64 = token[..num_end].parse().ok()?;
    let mult = match token[num_end..].to_uppercase().as_str() {
        "B" => 1.0,
        "KIB" => 1024.0,
        "MIB" => 1024.0 * 1024.0,
        "GIB" => 1024.0 * 1024.0 * 1024.0,
        "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    Some(num * mult)
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
        return Err(AppError::YtDlp(format!(
            "GET {url}: HTTP {}",
            resp.status()
        )));
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
