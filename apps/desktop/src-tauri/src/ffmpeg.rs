//! Извлечение аудио через ffmpeg: 16kHz mono PCM s16le WAV,
//! опционально с loudnorm-нормализацией.
//!
//! Прогресс-репортинг: парсим `time=HH:MM:SS.MS` из stderr, делим на
//! `total_duration_sec` (если знаем) → шлём `FfmpegEvent::Progress(pct)`.

use crate::error::{AppError, AppResult};
use crate::sidecar;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Что ffmpeg-стадия сообщает наружу во время работы.
#[derive(Debug, Clone)]
pub enum FfmpegEvent {
    /// Одна строка stderr ffmpeg (для лога).
    Log(String),
    /// Текущий прогресс 0.0..=1.0 (вычисляется из `time=...` в stderr).
    Progress(f32),
}

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
    /// `on_event` дёргается на каждую строку stderr + на обновления прогресса.
    /// `cancel` прерывает процесс.
    pub async fn extract_audio(
        &self,
        input: &Path,
        output: &Path,
        sample_rate: u32,
        normalize: bool,
        total_duration_sec: Option<f32>,
        cancel: CancellationToken,
        mut on_event: impl FnMut(FfmpegEvent) + Send + 'static,
    ) -> AppResult<()> {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }

        info!(
            "ffmpeg: starting (input={}, output={}, sr={}, normalize={}, total={:?}s)",
            input.display(),
            output.display(),
            sample_rate,
            normalize,
            total_duration_sec
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

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Ffmpeg("no stderr".into()))?;

        // Парсим каждую строку stderr: эмитим как Log, плюс пробуем вытащить
        // `time=...` для Progress (если знаем total_duration_sec).
        // Callback consumed by the spawn task; total_duration_sec is Copy
        // so the info! above + the closure capture both work.
        let _progress_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                on_event(FfmpegEvent::Log(line.clone()));
                if let Some(total) = total_duration_sec {
                    if total > 0.0 {
                        if let Some(time) = parse_ffmpeg_time(&line) {
                            let pct = (time / total).clamp(0.0, 1.0);
                            on_event(FfmpegEvent::Progress(pct));
                        }
                    }
                }
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

/// Извлечь `time=HH:MM:SS.MS` (или `MM:SS.MS`, или `Ns`) из строки stderr.
/// Возвращает текущую позицию в **секундах**, или None если не нашли.
fn parse_ffmpeg_time(line: &str) -> Option<f32> {
    let idx = line.find("time=")?;
    let after = &line[idx + 5..];
    let time_str: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ':' || *c == '.')
        .collect();
    parse_hms_to_seconds(&time_str)
}

fn parse_hms_to_seconds(s: &str) -> Option<f32> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h: f32 = parts[0].parse().ok()?;
            let m: f32 = parts[1].parse().ok()?;
            let sec: f32 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + sec)
        }
        2 => {
            let m: f32 = parts[0].parse().ok()?;
            let sec: f32 = parts[1].parse().ok()?;
            Some(m * 60.0 + sec)
        }
        1 => parts[0].parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_hms() {
        assert_eq!(parse_ffmpeg_time("time=00:01:23.45 bitrate=300kbits"), Some(83.45));
        assert_eq!(parse_ffmpeg_time("time=01:00:00.00"), Some(3600.0));
        assert_eq!(parse_ffmpeg_time("time=00:00:00.00 size=0kB"), Some(0.0));
    }

    #[test]
    fn parse_time_ms() {
        assert_eq!(parse_ffmpeg_time("time=12.34 "), Some(12.34));
    }

    #[test]
    fn parse_time_missing() {
        assert_eq!(parse_ffmpeg_time("frame=  100 fps=56 q=28.0"), None);
    }

    #[test]
    fn parse_hms_basic() {
        assert_eq!(parse_hms_to_seconds("00:01:23.45"), Some(83.45));
        assert_eq!(parse_hms_to_seconds("01:00:00"), Some(3600.0));
        assert_eq!(parse_hms_to_seconds("12.5"), Some(12.5));
    }
}
