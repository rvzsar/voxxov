//! Корневой модуль Tauri-приложения.
//!
//! Поднимает плагины, регистрирует команды, инициализирует логгер и
//! подписывается на broadcast-канал событий для ретрансляции во frontend
//! через Tauri events.

#![allow(clippy::needless_return)]

mod asr;
mod commands;
mod config;
mod error;
mod ffmpeg;
mod logging;
mod paths;
mod pipeline;
mod proxy;
mod sidecar;
mod state;
mod types;
mod ytdlp;

pub use error::{AppError, AppResult};
pub use state::AppState;
pub use types::*;

use tauri::{Emitter, Manager};

/// Точка входа, вызывается из `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Инициализируем логгер максимально рано — чтобы ошибки старта
    // (например, проблемы с конфигом) попали и в файл, и в stderr.
    let cfg = config::load_or_default();
    logging::init(&cfg.logging);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            // Создаём структуру каталогов сразу, чтобы первый запуск
            // не падал на записи в `jobs/` или `transcripts/`.
            if let Err(e) = paths::ensure_all(&app.handle()) {
                tracing::warn!("paths::ensure_all failed: {e}");
            }

            let state = AppState::new(cfg.clone(), app.handle().clone());

            // Запускаем eager init `yt-dlp` downloader в фоне. При первом
            // `enqueue_url` он уже будет готов (или init ещё идёт — тогда
            // `YtDlpRunner::get` подождёт через OnceCell).
            let state_for_init = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::ytdlp::YtDlpRunner::get(&state_for_init).await {
                    tracing::warn!("yt-dlp eager init failed: {e}");
                }
            });

            // Подписываемся на канал событий ДО manage, чтобы не потерять
            // ничего из того, что сгенерируется во время инициализации.
            let mut events_rx = state.events.subscribe();
            app.manage(state);

            // Запускаем forwarder: всё, что летит в `state.events`,
            // пересылается во frontend под именем `job:event`.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match events_rx.recv().await {
                        Ok(ev) => {
                            if let Err(e) = handle.emit("job:event", &ev) {
                                tracing::warn!("emit job:event failed: {e}");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("events receiver lagged, dropped {n} messages");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            tracing::info!("GigaAM Desktop started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::enqueue_url,
            commands::enqueue_local,
            commands::scan_folder,
            commands::list_jobs,
            commands::cancel_job,
            commands::get_config,
            commands::save_config,
            commands::fetch_metadata,
            commands::diagnose,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
