//! Запуск yt-dlp: метаданные, скачивание, парсинг прогресса.

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::proxy::build as build_proxy;
use crate::sidecar;
use crate::state::AppState;
use crate::types::{MediaInfo, Progress};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub struct YtDlpRunner {
    pub bin: PathBuf,
}

impl YtDlpRunner {
    pub fn resolve(cfg: &AppConfig) -> AppResult<Self> {
        let bin = sidecar::find_ytdlp(cfg.download.custom_ytdlp_path.as_deref())
            .ok_or_else(|| AppError::Sidecar("yt-dlp not found".into()))?;
        Ok(Self { bin })
    }

    pub async fn fetch_metadata(&self, url: &str) -> AppResult<MediaInfo> {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("--dump-single-json")
            .arg("--no-warnings")
            .arg("--no-playlist")
            .arg(url);
        let out = cmd.output().await.map_err(|e| AppError::YtDlp(format!("spawn: {e}")))?;
        if !out.status.success() {
            return Err(AppError::YtDlp(String::from_utf8_lossy(&out.stderr).to_string()));
        }
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        Ok(MediaInfo {
            id: v.get("id").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            url: url.to_string(),
            title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            uploader: v.get("uploader").and_then(|x| x.as_str())
                .or_else(|| v.get("channel").and_then(|x| x.as_str()))
                .map(|s| s.to_string()),
            duration_sec: v.get("duration").and_then(|x| x.as_f64()).map(|f| f as u64),
            thumbnail: v.get("thumbnail").and_then(|x| x.as_str()).map(|s| s.to_string()),
        })
    }

    /// Скачать видео в `out_dir` (формат задаётся `output_template`).
    pub async fn download(
        &self,
        state: &AppState,
        job_id: &str,
        url: &str,
        out_dir: &Path,
        template: &str,
        cfg: &AppConfig,
        cancel: CancellationToken,
    ) -> AppResult<PathBuf> {
        std::fs::create_dir_all(out_dir)?;

        let proxy = build_proxy(&cfg.proxy);
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(out_dir)
            .arg("--newline")
            .arg("--no-playlist")
            .arg("--no-part")
            .arg("--no-mtime")
            .arg("--no-warnings")
            .arg("--progress")
            .arg("--progress-template")
            .arg("download:PROGRESS:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(progress._downloaded_bytes_str)s|%(progress._total_bytes_str)s");

        for a in &proxy.args { cmd.arg(a); }
        if cfg.download.audio_only {
            cmd.arg("-x").arg("--audio-format").arg("wav");
        } else {
            cmd.arg("-f").arg(&cfg.download.format);
            if cfg.download.max_height > 0 {
                cmd.arg("-S").arg(format!("res:{}", cfg.download.max_height));
            }
            if cfg.download.embed_subs {
                cmd.arg("--embed-subs");
            }
        }
        if let Some(ua) = &cfg.download.user_agent {
            if !ua.is_empty() { cmd.arg("--user-agent").arg(ua); }
        }
        if let Some(cookies) = &cfg.download.cookie_file {
            if !cookies.is_empty() { cmd.arg("--cookies").arg(cookies); }
        }
        cmd.arg("-o").arg(template);
        cmd.arg("--retries").arg(cfg.download.retries.to_string());
        cmd.arg("--concurrent-fragments").arg(cfg.download.concurrent_fragments.to_string());
        if cfg.download.overwrite { cmd.arg("--force-overwrites"); } else { cmd.arg("--no-overwrites"); }
        cmd.arg(url);

        for (k, v) in &proxy.env { cmd.env(k, v); }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| AppError::YtDlp(format!("spawn: {e}")))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError::YtDlp("no stdout".into()))?;
        let stderr = child.stderr.take().ok_or_else(|| AppError::YtDlp("no stderr".into()))?;

        let state_a = state.clone();
        let jid = job_id.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(progress) = parse_progress_line(&line) {
                    let _ = state_a.events.send(crate::types::BackendEvent::DownloadProgress {
                        id: jid.clone(),
                        pct: progress.pct,
                        label: "Загрузка".to_string(),
                        speed: progress.speed,
                        eta: progress.eta,
                    });
                }
            }
        });

        let state_b = state.clone();
        let jid_b = job_id.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() { state_b.log_line(&jid_b, line); }
            }
        });

        // Параллельно ждём cancel и завершения процесса.
        let status = tokio::select! {
            res = child.wait() => res.map_err(|e| AppError::YtDlp(format!("wait: {e}")))?,
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(AppError::Cancelled);
            }
        };

        if !status.success() {
            return Err(AppError::YtDlp(format!("exit code: {:?}", status.code())));
        }
        // Найти файл по template
        find_downloaded(out_dir, template, url).ok_or_else(|| {
            AppError::YtDlp("downloaded file not found by template".into())
        })
    }
}

fn parse_progress_line(line: &str) -> Option<Progress> {
    if let Some(rest) = line.strip_prefix("PROGRESS:") {
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.is_empty() { return None; }
        let pct_str = parts.first().copied().unwrap_or("0").trim().trim_end_matches('%');
        let pct: f32 = pct_str.parse().unwrap_or(0.0);
        let speed = parts.get(1).copied().map(|s| s.trim().to_string());
        let eta = parts.get(2).copied().map(|s| s.trim().to_string());
        return Some(Progress { pct, label: "download".to_string(), speed, eta });
    }
    None
}

fn find_downloaded(dir: &Path, template: &str, url: &str) -> Option<PathBuf> {
    let ext_hint = Path::new(template)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut best: Option<PathBuf> = None;
    let mut best_mtime = std::time::SystemTime::UNIX_EPOCH;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() { continue; }
            let m = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if !ext_hint.is_empty() {
                if p.extension().and_then(|x| x.to_str()) != Some(ext_hint) { continue; }
            }
            if m > best_mtime {
                best_mtime = m;
                best = Some(p);
            }
        }
    }
    if best.is_some() { return best; }
    // fallback: попробуем взять id из url
    let id = url.rsplit('/').find(|s| !s.is_empty()).unwrap_or("video");
    let p = dir.join(format!("{id}.{ext_hint}"));
    if p.is_file() { Some(p) } else { None }
}
