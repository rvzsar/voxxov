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
mod models;
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
    // Инициализируем data_root раньше всего — пути нужны и для чтения
    // конфига, и для файла лога. Если .exe в read-only месте — паникуем
    // сразу с понятным сообщением (GIGAAM_DATA_DIR как escape hatch).
    crate::paths::init_data_root();

    // Каталоги создаём до первого обращения — иначе первый запуск
    // падает на записи.
    if let Err(e) = crate::paths::ensure_all() {
        tracing::warn!("paths::ensure_all failed: {e}");
    }

    // Инициализируем логгер после data_root — файл лога пишется в
    // <data_root>/data/logs/app.log.
    let cfg = config::load_or_default();
    logging::init(&cfg.logging);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let state = AppState::new(cfg.clone());

            // Eager init yt-dlp в фоне: при первом `enqueue_url` yt-dlp.exe
            // и ffmpeg.exe уже будут скачаны (или ошибка залогируется).
            // Ставим state в Tauri ДО spawn, чтобы UI мог через
            // `app.state::<AppState>()` сразу проверять состояние.
            let mut events_rx = state.events.subscribe();
            app.manage(state);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::ytdlp::YtDlpRunner::preflight().await {
                    tracing::warn!("yt-dlp preflight failed: {e}");
                }
            });

            // Forwarder: всё, что летит в state.events, эмитим как job:event.
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
            commands::diagnose,
            commands::save_job,
            commands::get_job_workdir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
