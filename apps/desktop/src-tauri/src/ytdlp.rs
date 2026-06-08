//! Обёртка над крейтом `yt-dlp` (GPL-3.0).
//!
//! Крейт берёт на себя:
//! - Скачивание и обновление yt-dlp + ffmpeg под текущую платформу.
//! - Парсинг JSON-метаданных, прогресс-парсинг.
//! - Cookies, прокси, форматы, кодеки — через fluent API.
//!
//! На этом уровне мы только переводим `AppConfig` → args + `Video` /
//! events → наши внутренние типы.

use crate::config::{AppConfig, DownloadConfig};
use crate::error::{AppError, AppResult};
use crate::proxy;
use crate::state::AppState;
use crate::types::{BackendEvent, MediaInfo};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use yt_dlp::client::deps::Libraries;
use yt_dlp::model::Video;
use yt_dlp::Downloader;

const OUTPUT_BASENAME: &str = "source";
const DEFAULT_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 GigaAM-Desktop/0.1";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Namespace; сам `Downloader` живёт в `AppState.downloader`.
pub struct YtDlpRunner;

impl YtDlpRunner {
    /// Получить downloader из `AppState`, инициализируя при первом вызове.
    /// Крейт `yt-dlp` при init скачает yt-dlp+ffmpeg в `$APPDATA/GigaAM/bin/`.
    pub async fn get(state: &AppState) -> AppResult<Arc<Downloader>> {
        let cfg = state.config();
        let bin_dir = crate::sidecar::bin_dir(Some(&state.app));
        std::fs::create_dir_all(&bin_dir).map_err(|e| {
            AppError::Other(format!("create bin dir {}: {e}", bin_dir.display()))
        })?;

        let libraries = Libraries::new(
            crate::sidecar::yt_dlp_path(Some(&state.app)),
            crate::sidecar::ffmpeg_path(Some(&state.app)),
        );

        // OnceCell.get_or_init не возвращает Result из init — храним
        // Result внутри и пробрасываем caller'у.
        let cfg_for_init = cfg.clone();
        let result = state
            .downloader
            .get_or_init(|| async {
                Self::init(bin_dir, libraries, &cfg_for_init)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await;
        result
            .clone()
            .map_err(|msg| AppError::Other(format!("yt-dlp: {msg}")))
    }

    async fn init(
        bin_dir: PathBuf,
        libraries: Libraries,
        cfg: &AppConfig,
    ) -> AppResult<Arc<Downloader>> {
        info!("yt-dlp: initializing (bin dir: {})", bin_dir.display());

        // `output_dir` ставим в `bin_dir`, чтобы случайный `download_video(..)`
        // (без `to_path`) не записал в системный PATH. Мы всегда используем
        // `download_video_to_path`, так что это по сути no-op safety net.
        let builder = Downloader::with_new_binaries(bin_dir.clone(), bin_dir)
            .await
            .map_err(|e| AppError::Other(format!("yt-dlp init: {e}")))?;

        let mut all_args = build_args(&cfg.download);
        all_args.extend(proxy::to_args(&cfg.proxy));
        let builder = builder.with_args(all_args);

        let mut downloader = builder
            .build()
            .await
            .map_err(|e| AppError::Other(format!("yt-dlp build: {e}")))?;

        // Cookies / UA / timeout — на `&mut Downloader`. Clone расшаривает
        // внутреннее состояние через Arc, так что настройки применяются
        // ко всем future-операциям. Конфиг меняется только при рестарте.
        if let Some(cookies) = cfg.download.cookie_file.as_deref() {
            if !cookies.is_empty() {
                downloader.set_cookies(cookies);
            }
        }
        let ua = cfg
            .download
            .user_agent
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_UA);
        downloader.set_user_agent(ua);
        downloader.set_timeout(COMMAND_TIMEOUT);

        info!("yt-dlp: ready");
        Ok(Arc::new(downloader))
    }

    pub async fn fetch_metadata(state: &AppState, url: &str) -> AppResult<MediaInfo> {
        let downloader = Self::get(state).await?;
        let v = downloader
            .fetch_video_infos(url)
            .await
            .map_err(|e| AppError::YtDlp(format!("{e}")))?;
        Ok(video_to_media(&v))
    }

    /// Скачать видео в `out_dir/source.<ext>`. Возвращает путь к файлу.
    pub async fn download(
        state: &AppState,
        job_id: &str,
        url: &str,
        out_dir: &Path,
        cfg: &AppConfig,
        cancel: CancellationToken,
    ) -> AppResult<PathBuf> {
        let downloader = Self::get(state).await?;
        std::fs::create_dir_all(out_dir).map_err(AppError::Io)?;

        let video = downloader
            .fetch_video_infos(url)
            .await
            .map_err(|e| AppError::YtDlp(format!("{e}")))?;

        // events_task живёт пока не abort'нем или Receiver не вернёт Closed.
        let mut events_rx = downloader.subscribe_events();
        let state_for_events = state.clone();
        let job_id_owned = job_id.to_string();
        let events_task = tokio::spawn(async move {
            loop {
                match events_rx.recv().await {
                    Ok(event) => forward_event(&state_for_events, &job_id_owned, event),
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!("yt-dlp events lagged, dropped {n}");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });

        let target = out_dir.join(OUTPUT_BASENAME);
        let downloader_clone = (*downloader).clone();
        let download_future = async move {
            downloader_clone
                .download_video_to_path(&video, target)
                .await
                .map_err(|e| AppError::YtDlp(format!("{e}")))
        };

        // Cancel ↔ download: shutdown() прерывает ВСЕ текущие загрузки в этом
        // Downloader'е. Per-job cancel через API крейте нельзя (нужен
        // download_video_with_priority и download_id), поэтому сейчас
        // download ждётся до конца после сигнала отмены.
        let result = tokio::select! {
            r = download_future => r,
            _ = cancel.cancelled() => {
                downloader.shutdown();
                events_task.abort();
                return Err(AppError::Cancelled);
            }
        };

        events_task.abort();
        let path = result?;
        if !path.is_file() {
            return Err(AppError::YtDlp(format!(
                "downloaded file not found at {}",
                path.display()
            )));
        }
        Ok(path)
    }
}

/// CLI-args для yt-dlp из нашего `DownloadConfig`. Передаются
/// в `Downloader::append_args` ДО `build()`.
fn build_args(dl: &DownloadConfig) -> Vec<String> {
    let mut args = vec![
        "--no-mtime".to_string(),
        "--no-warnings".to_string(),
        "--newline".to_string(),
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
            // Fallback: пустой format — берём дефолт, иначе yt-dlp
            // сам выберет «лучший», что не всегда разумно.
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
    args.push(if dl.overwrite {
        "--force-overwrites"
    } else {
        "--no-overwrites"
    }.to_string());
    if let Some(ua) = dl.user_agent.as_deref() {
        if !ua.is_empty() {
            args.push("--user-agent".to_string());
            args.push(ua.to_string());
        }
    }
    args
}

fn video_to_media(v: &Video) -> MediaInfo {
    MediaInfo {
        id: v.id.clone(),
        url: v.webpage_url.clone().unwrap_or_default(),
        title: v.title.clone(),
        uploader: v
            .uploader
            .clone()
            .or_else(|| v.channel.clone())
            .or_else(|| v.uploader_id.clone()),
        duration_sec: v.duration.map(|d| d as u64),
        thumbnail: v.thumbnail.clone(),
    }
}

/// Пробрасывает `DownloadEvent` из крейта в наш `BackendEvent` broadcast.
fn forward_event(
    state: &AppState,
    job_id: &str,
    event: Arc<yt_dlp::events::DownloadEvent>,
) {
    use yt_dlp::events::DownloadEvent as E;
    match event.as_ref() {
        E::DownloadProgress {
            downloaded_bytes,
            total_bytes,
            ..
        } => {
            let pct = if *total_bytes > 0 {
                (*downloaded_bytes as f32 / *total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let _ = state.events.send(BackendEvent::DownloadProgress {
                id: job_id.to_string(),
                pct,
                label: "Загрузка".to_string(),
                speed: None,
                eta: None,
            });
        }
        E::DownloadFailed { error, .. } => {
            state.log_line(job_id, format!("yt-dlp: download failed: {error}"));
        }
        E::FormatSelected { video_id, quality, .. } => {
            debug!("yt-dlp: format selected (video={video_id}, quality={quality})");
        }
        _ => debug!("yt-dlp event: {:?}", event.event_type()),
    }
}
