//! ASR-orchestrator.
//!
//! `transcribe` — async entry point. Тяжёлая работа (чтение WAV, создание
//! recognizer, decode сегментов) вынесена в `tokio::task::spawn_blocking`,
//! чтобы не блокировать tokio runtime thread.
//!
//! Длинное аудио (> `max_segment_sec`) разрезается на чанки с overlap;
//! cancellation проверяется между чанками. Сам `decode` отменить нельзя
//! (sherpa-onnx не поддерживает mid-cancel C-API).

use super::grouping::group_into_segments;
use super::segment::{read_wav_samples, split_into_segments};
use super::worker::AsrEngine;
use super::Transcription;
use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Main entry point: transcribe audio file → text + timed segments.
pub async fn transcribe(
    state: &AppState,
    job_id: &str,
    audio: &Path,
    cfg: &AsrConfig,
    cancel: CancellationToken,
) -> AppResult<Transcription> {
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

    // cmd: prefix — fallback to external CLI.
    if let Some(cmd) = cfg.model_path.strip_prefix("cmd:") {
        return super::cmd_fallback::transcribe_cmd(state, job_id, audio, cmd).await;
    }

    // 1. Discover model files.
    let (encoder, decoder, joiner, tokens) = discover_model_files(&cfg.model_path)?;
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

    // 2. Provider.
    let provider = match cfg.device {
        crate::config::AsrDevice::Cuda => "cuda",
        crate::config::AsrDevice::Directml => "directml",
        // Openvino не отличается от cpu в текущей версии sherpa-onnx.
        _ => "cpu",
    };
    let num_threads = num_cpus().min(4);

    // 3. Read WAV в spawn_blocking (hound — sync I/O).
    let audio_path = audio.to_path_buf();
    let samples = tokio::task::spawn_blocking(move || read_wav_samples(&audio_path))
        .await
        .map_err(|e| AppError::Asr(format!("wav reader join: {e}")))??;

    state.log_line(
        job_id,
        format!(
            "ASR: {} samples @ {}Hz, {:.1}s",
            samples.samples.len(),
            samples.sample_rate,
            samples.duration_sec()
        ),
    );

    // 4. Сегментация.
    let seg_sec = cfg.max_segment_sec.max(1.0);
    let overlap_sec = cfg.overlap_sec.max(0.0);
    let segments = split_into_segments(&samples.samples, samples.sample_rate, seg_sec, overlap_sec);
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

    // 5. Создать recognizer в spawn_blocking (тяжёлая загрузка ONNX).
    state.log_line(
        job_id,
        format!("ASR: creating recognizer (threads={num_threads}, provider={provider})"),
    );
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    let engine = tokio::task::spawn_blocking({
        let encoder = encoder;
        let decoder = decoder;
        let joiner = joiner;
        let tokens = tokens;
        let beam_size = cfg.beam_size;
        move || AsrEngine::new(&encoder, &decoder, &joiner, &tokens, num_threads, provider, beam_size)
    })
    .await
    .map_err(|e| AppError::Asr(format!("recognizer join: {e}")))??;
    let engine = Arc::new(engine);
    let total = segments.len();
    let chunk_dur = samples.samples.len() as f32 / samples.sample_rate as f32;

    // 6. Декодировать каждый сегмент.
    let mut texts: Vec<String> = Vec::with_capacity(total);
    let mut timed: Vec<super::TimedSegment> = Vec::new();
    for (i, seg) in segments.into_iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let seg_dur = seg.samples.len() as f32 / samples.sample_rate as f32;
        state.log_line(
            job_id,
            format!(
                "ASR: segment {}/{} ({:.1}s–{:.1}s)",
                i + 1,
                total,
                seg.offset_sec,
                seg.offset_sec + seg_dur
            ),
        );

        let eng = Arc::clone(&engine);
        let chunk = tokio::task::spawn_blocking(move || {
            eng.decode(&seg.samples, seg.sample_rate)
        })
        .await
        .map_err(|e| AppError::Asr(format!("decode join: {e}")))??;

        if !chunk.text.is_empty() {
            texts.push(chunk.text);
        }
        // Per-token timestamps + durations → сгруппировать в сегменты.
        let mut segs = group_into_segments(
            &chunk.tokens,
            chunk.timestamps.as_deref(),
            chunk.durations.as_deref(),
            seg.offset_sec,
            seg_dur,
        );
        timed.append(&mut segs);
    }

    let combined = texts.join(" ").trim().to_string();
    state.log_line(
        job_id,
        format!(
            "ASR: done, {} chars ({} segments, {} timed)",
            combined.len(),
            total,
            timed.len()
        ),
    );
    Ok(Transcription {
        text: combined,
        segments: timed,
    })
}

// --- helpers used both by orchestrator and tests ---

/// Discover encoder, decoder, joiner, и tokens файлы в директории моделей.
/// `model_path` должен быть директорией, содержащей все 4 файла.
pub fn discover_model_files(
    model_path: &str,
) -> AppResult<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)>
{
    let dir = std::path::Path::new(model_path);
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

    let encoder = encoder
        .ok_or_else(|| AppError::Asr(format!("no *encoder*.onnx in {}", dir.display())))?;
    let decoder = decoder
        .ok_or_else(|| AppError::Asr(format!("no *decoder*.onnx in {}", dir.display())))?;
    let joiner = joiner
        .ok_or_else(|| AppError::Asr(format!("no *joiner*.onnx in {}", dir.display())))?;
    let tokens = tokens
        .ok_or_else(|| AppError::Asr(format!("no *tokens.txt in {}", dir.display())))?;

    Ok((encoder, decoder, joiner, tokens))
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}
