//! Извлечение аудио через ffmpeg: 16kHz mono PCM s16le WAV,
//! опционально с loudnorm-нормализацией.

use crate::error::{AppError, AppResult};
use crate::sidecar;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub struct FfmpegRunner {
    pub bin: std::path::PathBuf,
}

impl FfmpegRunner {
    /// Сконструировать из `custom` пути (cfg.download.custom_ffmpeg_path),
    /// либо — из auto-detected пути в `sidecar::ffmpeg_path`.
    pub fn resolve(
        app: &tauri::AppHandle,
        custom: Option<&str>,
    ) -> AppResult<Self> {
        let bin = if let Some(p) = custom.filter(|s| !s.is_empty()) {
            std::path::PathBuf::from(p)
        } else {
            sidecar::ffmpeg_path(Some(app))
        };
        if !bin.is_file() {
            return Err(AppError::Sidecar(format!("ffmpeg not found at {}", bin.display())));
        }
        Ok(Self { bin })
    }

    /// Сконвертировать вход в 16kHz mono PCM s16le WAV.
    /// `on_log` вызывается для каждой строки stderr ffmpeg.
    /// `cancel` прерывает процесс при сигнале.
    pub async fn extract_audio(
        &self,
        input: &Path,
        output: &Path,
        sample_rate: u32,
        normalize: bool,
        cancel: CancellationToken,
        mut on_log: impl FnMut(String) + Send,
    ) -> AppResult<()> {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }

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
            .arg("-dn");

        if normalize {
            // loudnorm=I=-16:TP=-1.5:LRA=11 — стандарт подкаста.
            cmd.arg("-af").arg("loudnorm=I=-16:TP=-1.5:LRA=11");
        }

        cmd.arg("-ac")
            .arg("1")
            .arg("-ar")
            .arg(sample_rate.to_string())
            .arg("-f")
            .arg("wav")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg(output);

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Ffmpeg(format!("spawn: {e}")))?;
        if let Some(stderr) = child.stderr.take() {
            // `on_log` это `FnMut`, мы move'аем его в spawn.
            // Между строками concurrent не происходит (одна задача).
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    on_log(line);
                }
            });
        }

        let status = tokio::select! {
            res = child.wait() => res.map_err(|e| AppError::Ffmpeg(format!("wait: {e}")))?,
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(AppError::Cancelled);
            }
        };
        if !status.success() {
            return Err(AppError::Ffmpeg(format!("ffmpeg exit: {:?}", status.code())));
        }
        if !output.is_file() {
            return Err(AppError::Ffmpeg("output wav not created".into()));
        }
        Ok(())
    }
}
