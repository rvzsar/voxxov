//! Извлечение аудио через ffmpeg: 16kHz mono WAV, нормализация loudness.

use crate::error::{AppError, AppResult};
use crate::sidecar;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub struct FfmpegRunner {
    pub bin: PathBuf,
}

impl FfmpegRunner {
    pub fn resolve(custom: Option<&str>) -> AppResult<Self> {
        let bin = sidecar::find_ffmpeg(custom)
            .ok_or_else(|| AppError::Sidecar("ffmpeg not found".into()))?;
        Ok(Self { bin })
    }

    /// Сконвертировать любой вход в 16kHz mono PCM s16le WAV с опциональной
    /// нормализацией loudness. Прогресс в stderr ffmpeg логируется.
    pub async fn extract_audio(
        &self,
        input: &Path,
        output: &Path,
        sample_rate: u32,
        normalize: bool,
        cancel: CancellationToken,
        mut on_log: impl FnMut(String) + Send,
    ) -> AppResult<()> {
        if let Some(parent) = output.parent() { std::fs::create_dir_all(parent)?; }

        let mut cmd = Command::new(&self.bin);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel").arg("error")
            .arg("-nostdin")
            .arg("-i").arg(input)
            .arg("-vn")
            .arg("-sn")
            .arg("-dn");

        if normalize {
            // af=loudnorm=I=-16:TP=-1.5:LRA=11 — стандарт подкаста
            cmd.arg("-af").arg("loudnorm=I=-16:TP=-1.5:LRA=11");
        }

        cmd.arg("-ac").arg("1")
            .arg("-ar").arg(sample_rate.to_string())
            .arg("-f").arg("wav")
            .arg("-acodec").arg("pcm_s16le")
            .arg(output);

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| AppError::Ffmpeg(format!("spawn: {e}")))?;
        if let Some(stderr) = child.stderr.take() {
            let mut lines = BufReader::new(stderr).lines();
            tokio::spawn(async move {
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

pub fn infer_extension_from_url(url: &str) -> &'static str {
    if url.contains("youtu") || url.contains("youtu.be") { "mkv" }
    else if url.ends_with(".mp4") { "mp4" }
    else if url.ends_with(".webm") { "webm" }
    else { "mkv" }
}
