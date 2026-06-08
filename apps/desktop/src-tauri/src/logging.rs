//! tracing-логгер: stderr + опционально файл в `logs/app.log`.

use crate::config::LoggingConfig;
use std::fs::OpenOptions;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(cfg: &LoggingConfig) {
    let level = cfg.level.clone();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("gigaam_desktop_lib={level},tauri=warn,info")));

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(false)
        .with_ansi(true);

    let file_layer = if cfg.file {
        let path = crate::paths::logs_dir().join("app.log");
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        file.map(|f| fmt::layer().with_writer(f).with_ansi(false).with_target(true).boxed())
    } else { None };

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer);

    let with_file = match file_layer {
        Some(l) => registry.with(l).try_init(),
        None => registry.try_init(),
    };

    if let Err(e) = with_file {
        eprintln!("logging: subscriber init failed: {e}");
    }
}
