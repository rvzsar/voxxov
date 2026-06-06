//! Обёртка над крейтом `yt-dlp` (GPL-3.0).
//!
//! Крейт берёт на себя:
//! - Скачивание и обновление yt-dlp + ffmpeg под текущую платформу.
//! - Парсинг JSON-метаданных, прогресс-парсинг.
//! - Cookies, прокси, форматы, кодеки — через fluent API.
//!
//! На этом уровне мы только переводим `AppConfig` → `DownloaderBuilder`
//! и `Video` / events → наши внутренние типы.

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::proxy::to_ytdlp_proxy;
use crate::state::AppState;
use crate::types::MediaInfo;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::info;
use yt_dlp::client::deps::Libraries;
use yt_dlp::model::Video;
use yt_dlp::Downloader;

const OUTPUT_BASENAME: &str = "source";

pub struct YtDlpRunner;

impl YtDlpRunner {
    /// Получить downloader из `AppState`, инициализируя при первом вызове.
    /// При первом вызове крейт `yt-dlp` скачает yt-dlp+ffmpeg в
    /// `$APPDATA/GigaAM/bin/` (может занять несколько секунд).
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

        // Запускаем init через `get_or_init`. OnceCell не умеет
        // Result-возврат из init closure, поэтому храним Result внутри.
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

        // output_dir в `with_new_binaries` — это default; мы всегда
        // используем `download_video_to_path`, так что он не критичен.
        // Ставим `bin_dir`, чтобы случайный `download_video(..)` не
        // записал в системный PATH.
        let mut builder = Downloader::with_new_binaries(bin_dir.clone(), bin_dir)
            .await
            .map_err(|e| AppError::Other(format!("yt-dlp init: {e}")))?;

        // Args — ДО `build()`, на builder (мутабельный, не shared).
        let mut args: Vec<String> = vec![
            "--no-mtime".into(),
            "--no-warnings".into(),
            "--newline".into(),
            "--retries".into(),
            cfg.download.retries.to_string(),
            "--concurrent-fragments".into(),
            cfg.download.concurrent_fragments.to_string(),
        ];
        if cfg.download.audio_only {
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push("wav".into());
        } else {
            if !cfg.download.format.is_empty() {
                args.push("-f".into());
                args.push(cfg.download.format.clone());
            }
            if cfg.download.max_height > 0 {
                args.push("-S".into());
                args.push(format!("res:{}", cfg.download.max_height));
            }
            if cfg.download.embed_subs {
                args.push("--embed-subs".into());
            }
        }
        if cfg.download.overwrite {
            args.push("--force-overwrites".into());
        } else {
            args.push("--no-overwrites".into());
        }
        if let Some(ua) = cfg.download.user_agent.as_deref() {
            if !ua.is_empty() {
                args.push("--user-agent".into());
                args.push(ua.to_string());
            }
        }
        // Прокси — через `--proxy` arg (а не `with_proxy`), т.к. нет
        // гарантии, что `DownloaderBuilder` имеет `with_proxy` в этой
        // версии крейта. URL уже percent-encoded через `ProxyConfig::to_ytdlp_arg()`.
        if let Some(p) = to_ytdlp_proxy(&cfg.proxy) {
            args.push("--proxy".into());
            args.push(p.to_ytdlp_arg());
        }
        builder.append_args(args);

        let downloader = builder
            .build()
            .await
            .map_err(|e| AppError::Other(format!("yt-dlp build: {e}")))?;

        // Cookies / proxy / UA / timeout — на `&mut Downloader`. Clone
        // расшаривает внутреннее состояние через Arc, поэтому настройки
        // применяются ко всем future-операциям. Это OK, т.к. config
        // у нас меняется только при рестарте приложения.
        let mut downloader = downloader;
        if let Some(cookies) = cfg.download.cookie_file.as_deref() {
            if !cookies.is_empty() {
                downloader.set_cookies(cookies);
            }
        }
        // Таймаут 30 минут на команду.
        downloader.set_timeout(Duration::from_secs(30 * 60));
        // UA по умолчанию (если не задан в cfg).
        if cfg.download.user_agent.as_deref().map(str::is_empty).unwrap_or(true) {
            downloader.set_user_agent(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 GigaAM-Desktop/0.1",
            );
        }

        info!("yt-dlp: ready");
        Ok(Arc::new(downloader))
    }

    /// Fetch метаданных для URL.
    pub async fn fetch_metadata(state: &AppState, url: &str) -> AppResult<MediaInfo> {
        let downloader = Self::get(state).await?;
        let v = downloader
            .fetch_video_infos(url)
            .await
            .map_err(|e| AppError::YtDlp(format!("{e}")))?;
        Ok(video_to_media(&v))
    }

    /// Скачать видео в `out_dir`. Возвращает путь к скачанному файлу.
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

        // 1) Fetch метаданных.
        let video = downloader
            .fetch_video_infos(url)
            .await
            .map_err(|e| AppError::YtDlp(format!("{e}")))?;

        // 2) Подписка на events.
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

        // 3) Запуск download. `to_path` пишет точно в `out_dir/source`,
        //    расширение выберет yt-dlp по доступным форматам.
        let target = out_dir.join(OUTPUT_BASENAME);
        let downloader_clone = (*downloader).clone();
        let download_future = async move {
            downloader_clone
                .download_video_to_path(&video, target)
                .await
                .map_err(|e| AppError::YtDlp(format!("{e}")))
        };

        // 4) Гонка cancel ↔ download.
        let result = tokio::select! {
            r = download_future => r,
            _ = cancel.cancelled() => {
                downloader.shutdown();
                events_task.abort();
                return Err(AppError::Cancelled);
            }
        };

        // 5) Завершаем event task.
        tokio::time::sleep(Duration::from_millis(50)).await;
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
            let _ = state.events.send(crate::types::BackendEvent::DownloadProgress {
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
        E::FormatSelected { format_id, .. } => {
            state.log_line(job_id, format!("yt-dlp: format selected = {format_id}"));
        }
        _ => {}
    }
}
