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

use super::fmt_mmss;
use super::grouping::group_into_segments;
use super::Transcription;
use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::{JobUpdate, Progress};
use parking_lot::Mutex;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Кэш распознавателя: одна ORT-сессия на конфигурацию, а не на задачу.
/// Создание сессии грузит ~320MB модели — на очереди из нескольких задач
/// это повторялось бы для каждой. Ключ — всё, что влияет на конфигурацию.
/// Декоды сериализованы семафором `asr_permits`, поэтому лок не конкурирует.
static RECOGNIZER_CACHE: OnceLock<Mutex<(String, Option<OfflineRecognizer>)>> = OnceLock::new();

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
    // int8 энкодер масштабируется до ~8 потоков на десктопных CPU; дальше
    // упор в пропускную способность памяти, лишние потоки вредят.
    let num_threads = num_cpus().min(8);
    let beam_size = cfg.beam_size;

    state.log_line(
        job_id,
        format!("ASR: creating recognizer (threads={num_threads}, provider={provider}, beam={beam_size})"),
    );
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    // WAV-чтение + создание recognizer + decode (всё sync, в blocking pool).
    let audio_path = audio.to_path_buf();
    // Clone для UI progress-emit'ов внутри spawn_blocking (closure).
    let state_for_progress = state.clone();
    let job_id_owned = job_id.to_string();
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
            // GigaAM-V3 — NeMo-стиль transducer (так использует ekhodzitsky/gigastt
            // с sherpa-onnx 1.13). Без этого поля sherpa-onnx не знает, что это NeMo,
            // и подаёт raw samples в энкодер, рассчитанный на mel-фичи.
            config.model_config.model_type = Some("nemo_transducer".to_string());
            // GigaAM-V3 RNN-T обучен на 80-мерных fbank-фичах с 25ms окном /
            // 10ms шагом. Задаём явно.
            config.feat_config.sample_rate = sample_rate;
            config.feat_config.feature_dim = 80;

            // beam > 1 → modified_beam_search; beam == 1 → дефолт (greedy_search).
            let beam = beam_size.clamp(1, 64);
            if beam > 1 {
                config.decoding_method = Some("modified_beam_search".to_string());
                config.max_active_paths = beam as i32;
            }

            // Сессия переиспользуется между задачами (кэш выше): загрузка
            // модели — один раз на конфигурацию, а не на каждую задачу.
            // Размер энкодера в ключе — страховка от подмены файлов модели
            // на лету (устаревшая сессия).
            let enc_size = std::fs::metadata(&encoder).map(|m| m.len()).unwrap_or(0);
            let cache_key = format!("{model_dir}|{provider}|{num_threads}|{beam}|{enc_size}");
            let cache = RECOGNIZER_CACHE.get_or_init(|| Mutex::new((String::new(), None)));
            let cache_guard = cache.lock();
            if cache_guard.0 != cache_key {
                cache_guard.1 = OfflineRecognizer::create(&config).ok_or_else(|| {
                    AppError::Asr(
                        "failed to create OfflineRecognizer — check model paths and provider".into(),
                    )
                })?;
                cache_guard.0 = cache_key;
            }
            let recognizer = cache_guard.1.as_ref().unwrap();

            // Чанки определяются VAD + merge_segments в segmentation.rs
            // (15-22 сек, как в GigaAM transcribe_longform).
            let total = samples.len();
            let mut all_text = String::new();
            let mut all_tokens: Vec<String> = Vec::new();
            let mut all_timestamps: Vec<f32> = Vec::new();
            let mut all_durations: Vec<f32> = Vec::new();

            // `sample_rate` is i32 в sherpa_onnx::Wave; VAD принимает u32.
            let chunks: Vec<(usize, usize)> =
                super::segmentation::find_speech_segments(&samples, sample_rate as u32);

            tracing::info!(
                "ASR VAD: {} segments from {} samples ({:.1}s)",
                chunks.len(),
                total,
                total as f32 / sample_rate as f32
            );
            for (i, &(s, e)) in chunks.iter().enumerate().take(20) {
                tracing::debug!(
                    "ASR VAD seg[{}]: {}..{} ({:.2}s)",
                    i,
                    s,
                    e,
                    (e - s) as f32 / sample_rate as f32
                );
            }

            // Throttled UI progress: раз в 2 сек + на последнем чанке.
            // RTF и ETA обновляются на основе реально обработанных сэмплов.
            const PROGRESS_INTERVAL_SECS: u64 = 2;
            let decode_start = Instant::now();
            let mut last_emit = Instant::now();
            let total_chunks = chunks.len();
            let mut chunk_idx: usize = 0;
            let mut samples_done: usize = 0;

            for (chunk_start, chunk_end) in chunks {
                if cancel.is_cancelled() {
                    return Err(AppError::Cancelled);
                }
                let chunk = &samples[chunk_start..chunk_end];

                let stream = recognizer.create_stream();
                stream.accept_waveform(sample_rate, chunk);
                recognizer.decode(&stream);

                let r = stream
                    .get_result()
                    .ok_or_else(|| AppError::Asr("decode returned no result".into()))?;

                let chunk_offset_sec = chunk_start as f32 / sample_rate as f32;
                let chunk_text = r.text.trim();

                let text_preview: String = chunk_text.chars().take(80).collect();
                tracing::debug!(
                    "ASR chunk[{}/{}]: samples {}..{} ({:.2}s) tokens={} text={:?}",
                    chunk_idx,
                    total_chunks,
                    chunk_start,
                    chunk_end,
                    (chunk_end - chunk_start) as f32 / sample_rate as f32,
                    r.tokens.len(),
                    text_preview
                );
                if !chunk_text.is_empty() {
                    if !all_text.is_empty() {
                        all_text.push(' ');
                    }
                    all_text.push_str(chunk_text);
                }
                all_tokens.extend(r.tokens);
                if let Some(ts) = r.timestamps {
                    all_timestamps.extend(ts.iter().map(|&t| t + chunk_offset_sec));
                }
                if let Some(durs) = r.durations {
                    all_durations.extend(durs);
                }

                samples_done = chunk_end;
                chunk_idx += 1;

                // UI progress: throttled — раз в 2 сек или последний чанк.
                let now = Instant::now();
                if chunk_idx == total_chunks
                    || now.duration_since(last_emit) >= Duration::from_secs(PROGRESS_INTERVAL_SECS)
                {
                    let elapsed = decode_start.elapsed().as_secs_f32();
                    let audio_done_sec = samples_done as f32 / sample_rate as f32;
                    let rtf = if audio_done_sec > 0.0 {
                        elapsed / audio_done_sec
                    } else {
                        0.0
                    };
                    let audio_total_sec = total as f32 / sample_rate as f32;
                    let eta_sec = if rtf > 0.0 && audio_done_sec < audio_total_sec {
                        (audio_total_sec - audio_done_sec) * rtf
                    } else {
                        0.0
                    };
                    state_for_progress.update_job(
                        &job_id_owned,
                        JobUpdate {
                            progress: Some(Progress {
                                pct: chunk_idx as f32 / total_chunks as f32,
                                label: format!("Распознаём {}/{}", chunk_idx, total_chunks),
                                speed: Some(format!("RTF {:.2}x", rtf)),
                                eta: Some(fmt_mmss(eta_sec)),
                            }),
                            ..Default::default()
                        },
                    );
                    last_emit = now;
                }
            }

            tracing::info!(
                "ASR: {} chunks (avg {:.1}s, ≤{} samples) from {} samples",
                total_chunks,
                if total_chunks > 0 {
                    (total as f32 / total_chunks as f32) / sample_rate as f32
                } else {
                    0.0
                },
                // Жёсткий лимит чанка — 30с (см. MERGE_STRICT_LIMIT_SEC в segmentation.rs).
                sample_rate as usize * 30,
                total
            );

            // Конструктор OfflineRecognizerResult публичный — собираем комбинированный
            // результат и возвращаем Ok(...) чтобы удовлетворить сигнатуре closure.
            Ok(sherpa_onnx::OfflineRecognizerResult {
                text: all_text,
                tokens: all_tokens,
                timestamps: if all_timestamps.is_empty() {
                    None
                } else {
                    Some(all_timestamps)
                },
                durations: if all_durations.is_empty() {
                    None
                } else {
                    Some(all_durations)
                },
            })
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
    let tokens = tokens.ok_or_else(|| {
        AppError::Asr(format!("no *tokens.txt or *vocab.txt in {}", dir.display()))
    })?;

    Ok((encoder, decoder, joiner, tokens))
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}
