//! ASR-orchestrator.
//!
//! `transcribe` — async entry point. Тяжёлая работа (чтение WAV, создание
//! recognizer, decode сегментов) вынесена в `tokio::task::spawn_blocking`,
//! чтобы не блокировать tokio runtime thread.
//!
//! Длинное аудио (> `max_segment_sec`) разрезается на чанки с overlap;
//! cancellation проверяется между чанками. Сам `decode` отменить нельзя
//! (sherpa-onnx не поддерживает mid-cancel C-API).

use super::segment::{read_wav_samples, split_into_segments};
use super::worker::AsrEngine;
use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Main entry point: transcribe audio file → text.
pub async fn transcribe(
    state: &AppState,
    job_id: &str,
    audio: &Path,
    cfg: &AsrConfig,
    cancel: CancellationToken,
) -> AppResult<String> {
    if !audio.is_file() {
        return Err(AppError::Asr(format!(
            "audio not found: {}",
            audio.display()
        )));
    }
    if cfg.model_path.is_empty() {
        return Err(AppError::Asr("model_path is empty".into()));
    }

    state.log_line(job_id, format!("ASR: loading model from {}", cfg.model_path));

    // cmd: prefix — fallback to external CLI
    if let Some(cmd) = cfg.model_path.strip_prefix("cmd:") {
        return super::cmd_fallback::transcribe_cmd(state, job_id, audio, cmd).await;
    }

    // К этой точке `cfg.model_path` гарантированно непустой и не `cmd:` —
    // оба случая отсечены выше. Auto-discovery оставлен как fallback.
    let model_dir = if cfg.model_path.is_empty() {
        match auto_discover_model_dir() {
            Some(d) => d,
            None => return Err(AppError::Asr("model_path is empty and no models/ directory found near the app".into())),
        }
    } else {
        Path::new(&cfg.model_path).to_path_buf()
    };

    // 1. Discover model files
    let (encoder, decoder, joiner, tokens) = discover_model_files(&model_dir)?;
    state.log_line(
        job_id,
        format!(
            "ASR: encoder={} decoder={} joiner={} tokens={}",
            encoder.display(),
            decoder.display(),
            joiner.display(),
            tokens.display()
        ),
    );

    // 2. Provider
    let provider = match cfg.device {
        crate::config::AsrDevice::Cuda => "cuda",
        crate::config::AsrDevice::Directml => "directml",
        _ => "cpu",
    };
    let num_threads = num_cpus().min(4);

    // 3. Read WAV в spawn_blocking
    let audio_path = audio.to_path_buf();
    let audio = tokio::task::spawn_blocking(move || read_wav_samples(&audio_path))
        .await
        .map_err(|e| AppError::Asr(format!("wav reader join: {e}")))??;

    state.log_line(
        job_id,
        format!(
            "ASR: {} samples @ {}Hz, {:.1}s",
            audio.samples.len(),
            audio.sample_rate,
            audio.duration_sec()
        ),
    );

    // 4. Сегментация
    let seg_sec = cfg.max_segment_sec.max(1.0);
    let overlap_sec = cfg.overlap_sec.max(0.0);
    let segments = split_into_segments(&audio.samples, audio.sample_rate, seg_sec, overlap_sec);
    if segments.is_empty() {
        return Err(AppError::Asr("no audio samples after segmentation".into()));
    }
    if segments.len() > 1 {
        state.log_line(
            job_id,
            format!(
                "ASR: split into {} segments (~{:.0}s, overlap {:.1}s)",
                segments.len(),
                seg_sec,
                overlap_sec
            ),
        );
    }

    // 5. Создать recognizer в spawn_blocking (тяжёлая загрузка ONNX)
    state.log_line(
        job_id,
        format!("ASR: creating recognizer (threads={num_threads}, provider={provider})"),
    );
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    let enc = encoder.clone();
    let dec = decoder.clone();
    let join = joiner.clone();
    let tok = tokens.clone();
    let engine = tokio::task::spawn_blocking(move || {
        AsrEngine::new(&enc, &dec, &join, &tok, num_threads, provider)
    })
    .await
    .map_err(|e| AppError::Asr(format!("recognizer join: {e}")))??;

    let engine = Arc::new(engine);
    let total = segments.len();

    // 6. Декодировать каждый сегмент
    let mut texts: Vec<String> = Vec::with_capacity(total);
    for (i, seg) in segments.into_iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        state.log_line(
            job_id,
            format!(
                "ASR: segment {}/{} ({:.1}s–{:.1}s)",
                i + 1,
                total,
                seg.offset_sec,
                seg.offset_sec + seg.samples.len() as f32 / audio.sample_rate as f32
            ),
        );

        let eng = Arc::clone(&engine);
        let text = tokio::task::spawn_blocking(move || {
            eng.decode(&seg.samples, seg.sample_rate)
        })
        .await
        .map_err(|e| AppError::Asr(format!("decode join: {e}")))??;

        if !text.is_empty() {
            // Склейка с пробелом; каждый чанк возвращает уже обрезанный текст.
            texts.push(text);
        }
    }

    let combined = texts.join(" ").trim().to_string();
    state.log_line(
        job_id,
        format!("ASR: done, {} chars ({} segments)", combined.len(), total),
    );
    Ok(combined)
}

// --- helpers used both by orchestrator and tests ---

/// Discover encoder, decoder, joiner, and tokens files in a model directory.
pub fn discover_model_files(
    dir: &Path,
) -> AppResult<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)>
{
    if !dir.is_dir() {
        return Err(AppError::Asr(format!(
            "model_path is not a directory: {}",
            dir.display()
        )));
    }

    let mut encoder = None;
    let mut decoder = None;
    let mut joiner = None;
    let mut tokens = None;

    for entry in std::fs::read_dir(dir)
        .map_err(|e| AppError::Asr(format!("read dir {}: {e}", dir.display())))?
        .flatten()
    {
        let name_str = entry.file_name().to_string_lossy().to_lowercase();
        if name_str.contains("encoder") && name_str.ends_with(".onnx") {
            encoder = Some(entry.path());
        } else if name_str.contains("decoder") && name_str.ends_with(".onnx") {
            decoder = Some(entry.path());
        } else if (name_str.contains("joiner") || name_str.contains("joint"))
            && name_str.ends_with(".onnx")
        {
            joiner = Some(entry.path());
        } else if name_str.ends_with("tokens.txt") {
            tokens = Some(entry.path());
        }
    }

    let encoder = encoder.ok_or_else(|| {
        AppError::Asr(format!("no *encoder*.onnx in {}", dir.display()))
    })?;
    let decoder = decoder.ok_or_else(|| {
        AppError::Asr(format!("no *decoder*.onnx in {}", dir.display()))
    })?;
    let joiner = joiner.ok_or_else(|| {
        AppError::Asr(format!("no *joiner*.onnx in {}", dir.display()))
    })?;
    let tokens = tokens.ok_or_else(|| {
        AppError::Asr(format!("no *tokens.txt in {}", dir.display()))
    })?;

    Ok((encoder, decoder, joiner, tokens))
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}

fn auto_discover_model_dir() -> Option<std::path::PathBuf> {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("models"))),
        std::env::current_exe().ok().and_then(|e| {
            e.parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("models"))
        }),
        std::env::current_dir().ok().map(|p| p.join("models")),
    ];
    for c in candidates.into_iter().flatten() {
        if c.is_dir() {
            let has_encoder = std::fs::read_dir(&c)
                .ok()
                .map(|entries| {
                    entries.flatten().any(|e| {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        n.contains("encoder") && n.ends_with(".onnx")
                    })
                })
                .unwrap_or(false);
            if has_encoder {
                return Some(c);
            }
        }
    }
    None
}
