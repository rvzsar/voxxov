//! ASR-orchestrator. Sherpa-onnx (static C library, native Rust bindings).
//!
//! `transcribe` — async entry point. Тяжёлая работа (чтение WAV, создание
//! recognizer, decode) унесена в один `tokio::task::spawn_blocking`,
//! чтобы не блокировать tokio runtime thread.
//!
//! Cancellation проверяется перед загрузкой модели. Сам `decode` отменить
//! нельзя (sherpa-onnx не поддерживает mid-cancel C-API).
//!
//! Если `model_dir` начинается с `cmd:` — fallback на внешний CLI
//! (см. `cmd_fallback`).

use super::grouping::group_into_segments;
use super::Transcription;
use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Main entry point: transcribe audio file → text + timed segments.
/// `model_dir` — директория с 4 файлами GigaAM; проверка/загрузка
/// делается в `pipeline.rs::ensure_asr_model` до этого вызова.
pub async fn transcribe(
    state: &AppState,
    job_id: &str,
    audio: &Path,
    model_dir: &str,
    cfg: &AsrConfig,
    cancel: CancellationToken,
) -> AppResult<Transcription> {
    if !audio.is_file() {
        return Err(AppError::Asr(format!(
            "audio not found: {}",
            audio.display()
        )));
    }
    if model_dir.is_empty() {
        return Err(AppError::Asr("model_dir is empty".into()));
    }

    state.log_line(job_id, format!("ASR: loading model from {model_dir}"));

    // cmd: prefix — fallback to external CLI.
    if let Some(cmd) = cfg.model_dir.strip_prefix("cmd:") {
        return super::cmd_fallback::transcribe_cmd(state, job_id, audio, cmd, cancel).await;
    }

    // Discover model files.
    let (encoder, decoder, joiner, tokens) = discover_model_files(model_dir)?;
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

    let provider = match cfg.device {
        crate::config::AsrDevice::Cuda => "cuda",
        crate::config::AsrDevice::Directml => "directml",
        // Openvino = cpu в текущей версии sherpa-onnx.
        _ => "cpu",
    };
    let num_threads = num_cpus().min(4);
    let beam_size = cfg.beam_size;

    state.log_line(
        job_id,
        format!("ASR: creating recognizer (threads={num_threads}, provider={provider}, beam={beam_size})"),
    );
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    // WAV-чтение + создание recognizer + decode — всё sync, в blocking pool.
    let audio_path = audio.to_path_buf();
    let result =
        tokio::task::spawn_blocking(move || -> AppResult<sherpa_onnx::OfflineRecognizerResult> {
            let wave = sherpa_onnx::Wave::read(&audio_path.to_string_lossy()).ok_or_else(|| {
                AppError::Asr(format!("read wav {}: failed", audio_path.display()))
            })?;
            let samples = wave.samples();
            if samples.is_empty() {
                return Err(AppError::Asr("no audio samples".into()));
            }
            let sample_rate = wave.sample_rate();
            // Диагностика: что реально получает sherpa-onnx. Если в логе видно
            // samples.len() / sample_rate ≈ длительности файла, а не ~100×короче
            // (что соответствовало бы fbank frames), значит энкодер получает raw
            // samples вместо фичей и упрётся в max_seq_len (типично 5000 для
            // Conformer positional encoding).
            tracing::info!(
                "ASR: {} samples @ {}Hz ({:.1}s) from {}",
                samples.len(),
                sample_rate,
                samples.len() as f32 / sample_rate as f32,
                audio_path.display()
            );

            let mut config = OfflineRecognizerConfig::default();
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(encoder.to_string_lossy().into_owned()),
                decoder: Some(decoder.to_string_lossy().into_owned()),
                joiner: Some(joiner.to_string_lossy().into_owned()),
            };
            config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
            config.model_config.num_threads = num_threads as i32;
            config.model_config.provider = Some(provider.to_string());
            // GigaAM-V3 — это NeMo-стиль transducer (закодирован в репо
            // amidexe/govorun-lite, который ипользует sherpa-onnx 1.13). Без
            // этого поля sherpa-onnx не знает что это NeMo и подаёт raw samples
            // в энкодер, который рассчитан на fbank-фичи → broadcasting
            // error 5000 × N. См. также `feat_config` ниже.
            config.model_config.model_type = Some("nemo_transducer".to_string());
            // GigaAM-V3 RNN-T обучен на 80-мерных fbank-фичах с 25ms окном /
            // 10ms шагом. Задаём явно на случай, если дефолты отличаются.
            config.feat_config.sample_rate = sample_rate;
            config.feat_config.feature_dim = 80;

            // beam > 1 → modified_beam_search; beam == 1 → дефолт (greedy_search) из C-стороны.
            let beam = beam_size.clamp(1, 64);
            if beam > 1 {
                config.decoding_method = Some("modified_beam_search".to_string());
                config.max_active_paths = beam as i32;
            }

            let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
                AppError::Asr(
                    "failed to create OfflineRecognizer — check model paths and provider".into(),
                )
            })?;

            let stream = recognizer.create_stream();
            stream.accept_waveform(sample_rate, samples);
            recognizer.decode(&stream);
            // recognizer и stream дропаются здесь — больше не нужны.
            stream
                .get_result()
                .ok_or_else(|| AppError::Asr("decode returned no result".into()))
        })
        .await
        .map_err(|e| AppError::Asr(format!("asr join: {e}")))??;

    let tokens = result.tokens;
    let timestamps = result.timestamps;
    let durations = result.durations;
    let duration_sec = timestamps
        .as_ref()
        .and_then(|t| t.last().copied())
        .unwrap_or(0.0);

    state.log_line(
        job_id,
        format!(
            "ASR: {} tokens, {:.1}s audio, text={} chars",
            tokens.len(),
            duration_sec,
            result.text.chars().count(),
        ),
    );

    let segments = group_into_segments(
        &tokens,
        timestamps.as_deref(),
        durations.as_deref(),
        0.0,
        duration_sec,
    );
    Ok(Transcription {
        text: result.text.trim().to_string(),
        segments,
    })
}

// --- helpers ---

/// Найти encoder/decoder/joiner/tokens в `model_dir` (должна быть директорией).
pub fn discover_model_files(model_dir: &str) -> AppResult<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let dir = Path::new(model_dir);
    if !dir.is_dir() {
        return Err(AppError::Asr(format!(
            "model_dir is not a directory: {}",
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

    let encoder =
        encoder.ok_or_else(|| AppError::Asr(format!("no *encoder*.onnx in {}", dir.display())))?;
    let decoder =
        decoder.ok_or_else(|| AppError::Asr(format!("no *decoder*.onnx in {}", dir.display())))?;
    let joiner =
        joiner.ok_or_else(|| AppError::Asr(format!("no *joiner*.onnx in {}", dir.display())))?;
    let tokens =
        tokens.ok_or_else(|| AppError::Asr(format!("no *tokens.txt in {}", dir.display())))?;

    Ok((encoder, decoder, joiner, tokens))
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}
