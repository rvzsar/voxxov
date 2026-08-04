//! Извлечение аудио через ffmpeg: 16kHz mono PCM s16le WAV.
//!
//! stderr-строки ffmpeg (при `-loglevel error` — только ошибки) прокидываются
//! в `on_log` для отображения в UI. Параллельный парсинг `time=` для
//! прогресс-бара не делаем: при `-loglevel error` этих строк нет, а
//! переключать на `-loglevel info` ради прогресса слишком шумно для лога.

use crate::error::{AppError, AppResult};
use crate::sidecar;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct FfmpegRunner {
    pub bin: std::path::PathBuf,
}

impl FfmpegRunner {
    /// Из `custom` пути, иначе — auto-detected из `sidecar::ffmpeg_path`.
    pub fn resolve(custom: Option<&str>) -> AppResult<Self> {
        let bin = if let Some(p) = custom.filter(|s| !s.is_empty()) {
            std::path::PathBuf::from(p)
        } else {
            sidecar::ffmpeg_path()
        };
        if !bin.is_file() {
            return Err(AppError::Sidecar(format!("ffmpeg not found at {}", bin.display())));
        }
        Ok(Self { bin })
    }

    /// Сконвертировать вход → 16kHz mono PCM s16le WAV.
    /// `on_log` вызывается на каждую строку stderr ffmpeg.
    /// `cancel` прерывает процесс.
    pub async fn extract_audio(
        &self,
        input: &Path,
        output: &Path,
        sample_rate: u32,
        cancel: CancellationToken,
        mut on_log: impl FnMut(String) + Send + 'static,
    ) -> AppResult<()> {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }

        info!(
            "ffmpeg: starting (input={}, output={}, sr={})",
            input.display(),
            output.display(),
            sample_rate,
        );

        let mut cmd = Command::new(&self.bin);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-nostdin")
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-sn")
            .arg("-dn")
            .arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg(sample_rate.to_string())
            .arg("-f")
            .arg("wav")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg(output);

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());
        crate::sidecar::hide_console(&mut cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Ffmpeg(format!("spawn: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Ffmpeg("no stderr".into()))?;

        // Дренаж stderr в лог. Таска сама завершится когда child закроет pipe
        // (нормальный exit или после start_kill на cancel).
        let _log_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                on_log(line);
            }
        });

        let status = tokio::select! {
            res = child.wait() => res.map_err(|e| AppError::Ffmpeg(format!("wait: {e}"))),
            _ = cancel.cancelled() => {
                let _ = child.start_kill();
                return Err(AppError::Cancelled);
            }
        };
        let status = status?;
        if !status.success() {
            return Err(AppError::Ffmpeg(format!("ffmpeg exit: {:?}", status.code())));
        }
        if !output.is_file() {
            return Err(AppError::Ffmpeg("output wav not created".into()));
        }
        info!("ffmpeg: done → {}", output.display());
        Ok(())
    }
}
