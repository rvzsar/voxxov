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
use super::segmentation::{split_chunk, ChunkAssembler, VadSegmenter, MERGE_STRICT_LIMIT_SEC, VAD_FEED_SAMPLES};
use super::Transcription;
use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::{JobUpdate, Progress};
use parking_lot::Mutex;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};
use std::io::{Read, Seek};
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
    // int8-энкодер bandwidth-bound: 8 потоков оказались медленнее 4 на
    // том же ролике (RTF 0.089 → 0.164) — больше потоков = thrash кэша.
    // 4 — проверенный оптимум; на слабых машинах min() урежет сам.
    let num_threads = num_cpus().min(4);
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
    // `model_dir` — заимствованная ссылка; spawn_blocking требует 'static.
    let model_dir_owned = model_dir.to_string();
    // Clone для UI progress-emit'ов внутри spawn_blocking (closure).
    let state_for_progress = state.clone();
    let job_id_owned = job_id.to_string();
    let result =
        tokio::task::spawn_blocking(move || -> AppResult<sherpa_onnx::OfflineRecognizerResult> {
            // Стриминг: WAV читается кусками, VAD и декод идут одним проходом,
            // в памяти — только текущий чанк, а не весь файл.
            let mut wav = WavReader::open(&audio_path)?;
            let sample_rate = wav.sample_rate();
            let total = wav.total_samples();
            if total == 0 {
                return Err(AppError::Asr("no audio samples".into()));
            }
            tracing::info!(
                "ASR: {} samples @ {}Hz ({:.1}s) from {}",
                total,
                sample_rate,
                total as f32 / sample_rate as f32,
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
            let cache_key =
                format!("{model_dir_owned}|{provider}|{num_threads}|{beam}|{enc_size}");
            let cache = RECOGNIZER_CACHE.get_or_init(|| Mutex::new((String::new(), None)));
            let mut cache_guard = cache.lock();
            if cache_guard.0 != cache_key {
                cache_guard.1 = Some(OfflineRecognizer::create(&config).ok_or_else(|| {
                    AppError::Asr(
                        "failed to create OfflineRecognizer — check model paths and provider".into(),
                    )
                })?);
                cache_guard.0 = cache_key;
            }
            let recognizer = cache_guard.1.as_ref().unwrap();

            // Нет VAD-модели — пустой транскрипт (как раньше).
            let Some(mut segmenter) = VadSegmenter::new(sample_rate as u32) else {
                return Ok(sherpa_onnx::OfflineRecognizerResult {
                    text: String::new(),
                    tokens: Vec::new(),
                    timestamps: None,
                    durations: None,
                });
            };
            let mut assembler = ChunkAssembler::new(sample_rate as u32);
            let strict_limit = (MERGE_STRICT_LIMIT_SEC * sample_rate as f32) as usize;
            // Буфер текущего чанка: от assembler.curr_start() до stream_pos.
            let mut chunk_buf: Vec<f32> = Vec::new();
            let mut acc = DecodeAccum::default();
            let mut seg_count: usize = 0;
            let mut stream_pos: usize = 0;

            // Throttled UI progress: раз в 2 сек. RTF и ETA — по реально
            // декодированным сэмплам (VAD бежит впереди декода).
            const PROGRESS_INTERVAL_SECS: u64 = 2;
            let decode_start = Instant::now();
            let mut last_emit = Instant::now();

            // Один проход: WAV → VAD (куски 64 мс) → склейка → декод.
            while let Some(piece) = wav.next(VAD_FEED_SAMPLES)? {
                if cancel.is_cancelled() {
                    return Err(AppError::Cancelled);
                }
                stream_pos += piece.len();
                chunk_buf.extend_from_slice(&piece);

                for seg in segmenter.feed(&piece, stream_pos) {
                    seg_count += 1;
                    let before = assembler.curr_start();
                    for (s, e) in assembler.feed(seg) {
                        let chunk = &chunk_buf[..e - before];
                        decode_chunk(recognizer, sample_rate, &mut acc, chunk, s, strict_limit, &cancel,
                        )?;
                    }
                    chunk_buf.drain(..(assembler.curr_start() - before));
                }

                let now = Instant::now();
                if now.duration_since(last_emit) >= Duration::from_secs(PROGRESS_INTERVAL_SECS) {
                    let elapsed = decode_start.elapsed().as_secs_f32();
                    let done_sec = acc.decoded_until as f32 / sample_rate as f32;
                    let rtf = if done_sec > 0.0 {
                        elapsed / done_sec
                    } else {
                        0.0
                    };
                    let pct = (acc.decoded_until as f32 / total as f32).clamp(0.0, 1.0);
                    let eta_sec = if rtf > 0.0 && acc.decoded_until < total as usize {
                        (total as f32 - acc.decoded_until as f32) / sample_rate as f32 * rtf
                    } else {
                        0.0
                    };
                    state_for_progress.update_job(
                        &job_id_owned,
                        JobUpdate {
                            progress: Some(Progress {
                                pct,
                                label: format!("Распознаём {:.0}%", pct * 100.0),
                                speed: Some(format!("RTF {:.2}x", rtf)),
                                eta: Some(fmt_mmss(eta_sec)),
                            }),
                            ..Default::default()
                        },
                    );
                    last_emit = now;
                }
            }

            // Конец файла: последние сегменты VAD + финальный чанк.
            for seg in segmenter.finish(stream_pos) {
                seg_count += 1;
                let before = assembler.curr_start();
                for (s, e) in assembler.feed(seg) {
                    let chunk = &chunk_buf[..e - before];
                    decode_chunk(recognizer, sample_rate, &mut acc, chunk, s, strict_limit, &cancel,
                    )?;
                }
                chunk_buf.drain(..(assembler.curr_start() - before));
            }
            let before = assembler.curr_start();
            for (s, e) in assembler.finish() {
                let chunk = &chunk_buf[..e - before];
                decode_chunk(recognizer, sample_rate, &mut acc, chunk, s, strict_limit, &cancel,
                )?;
            }

            tracing::info!(
                "ASR VAD: {} segments from {} samples ({:.1}s)",
                seg_count,
                total,
                total as f32 / sample_rate as f32
            );
            tracing::info!(
                "ASR: {} chunks (avg {:.1}s, ≤{} samples) from {} samples",
                acc.chunks,
                if acc.chunks > 0 {
                    (total as f32 / acc.chunks as f32) / sample_rate as f32
                } else {
                    0.0
                },
                // Жёсткий лимит чанка — 30с (см. MERGE_STRICT_LIMIT_SEC в segmentation.rs).
                sample_rate as usize * 30,
                total
            );

            // Конструктор OfflineRecognizerResult публичный — собираем
            // комбинированный результат.
            Ok(sherpa_onnx::OfflineRecognizerResult {
                text: acc.text,
                tokens: acc.tokens,
                timestamps: if acc.timestamps.is_empty() {
                    None
                } else {
                    Some(acc.timestamps)
                },
                durations: if acc.durations.is_empty() {
                    None
                } else {
                    Some(acc.durations)
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

// --- streaming helpers ---

/// Стриминговый читатель WAV. Формат контролируется пайплайном (ffmpeg:
/// 16 кГц mono PCM s16le), но заголовок разбирается по-честному — как
/// wave-reader.cc в sherpa-onnx: RIFF/WAVE, обход chunk'ов до `data`,
/// поддержка PCM s16 (÷32768) и IEEE float32.
struct WavReader {
    file: std::fs::File,
    sample_rate: u32,
    bytes_per_sample: u64,
    data_start: u64,
    data_end: u64,
    pos: u64,
}

impl WavReader {
    fn open(path: &Path) -> AppResult<Self> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| AppError::Asr(format!("open {}: {e}", path.display())))?;
        let mut buf = [0u8; 4096];
        let n = file
            .read(&mut buf)
            .map_err(|e| AppError::Asr(format!("read header {}: {e}", path.display())))?;
        if n < 44 || &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
            return Err(AppError::Asr(format!("not a WAV file: {}", path.display())));
        }

        let mut sample_rate: u32 = 0;
        let mut channels: u16 = 0;
        let mut bits: u16 = 0;
        let mut audio_format: u16 = 0;
        let mut data_start: Option<usize> = None;
        let mut data_len: u32 = 0;

        // Обход chunk'ов от offset 12 до "data" (chunk'и выровнены по чётному).
        let mut off = 12usize;
        while off + 8 <= n {
            let size = u32::from_le_bytes([
                buf[off + 4],
                buf[off + 5],
                buf[off + 6],
                buf[off + 7],
            ]) as usize;
            match &buf[off..off + 4] {
                b"fmt " => {
                    if off + 8 + 16 <= n {
                        audio_format = u16::from_le_bytes([buf[off + 8], buf[off + 9]]);
                        channels = u16::from_le_bytes([buf[off + 10], buf[off + 11]]);
                        sample_rate = u32::from_le_bytes([
                            buf[off + 12],
                            buf[off + 13],
                            buf[off + 14],
                            buf[off + 15],
                        ]);
                        bits = u16::from_le_bytes([buf[off + 22], buf[off + 23]]);
                    }
                }
                b"data" => {
                    data_start = Some(off + 8);
                    data_len = size as u32;
                    break;
                }
                _ => {}
            }
            off += 8 + size + (size & 1);
        }

        let data_start = data_start.ok_or_else(|| {
            AppError::Asr(format!("wav data chunk not found: {}", path.display()))
        })?;
        if sample_rate == 0 || channels == 0 || bits == 0 {
            return Err(AppError::Asr(format!("wav fmt chunk missing: {}", path.display())));
        }
        if channels != 1 {
            return Err(AppError::Asr(format!(
                "unsupported wav channels: {channels} (expected mono)"
            )));
        }
        let bytes_per_sample = match (audio_format, bits) {
            (1, 16) => 2u64, // PCM s16
            (3, 32) => 4u64, // IEEE float32
            _ => {
                return Err(AppError::Asr(format!(
                    "unsupported wav format: audio_format={audio_format}, bits={bits}"
                )))
            }
        };

        file.seek(std::io::SeekFrom::Start(data_start as u64))
            .map_err(|e| AppError::Asr(format!("seek: {e}")))?;

        Ok(Self {
            file,
            sample_rate,
            bytes_per_sample,
            data_start: data_start as u64,
            data_end: data_start as u64 + data_len as u64,
            pos: data_start as u64,
        })
    }

    fn sample_rate(&self) -> i32 {
        self.sample_rate as i32
    }

    fn total_samples(&self) -> u64 {
        (self.data_end - self.data_start) / self.bytes_per_sample
    }

    /// Прочитать до `max_samples` сэмплов (нормализованных [-1, 1]).
    /// `None` — конец данных.
    fn next(&mut self, max_samples: usize) -> AppResult<Option<Vec<f32>>> {
        let remaining = self.data_end.saturating_sub(self.pos);
        if remaining == 0 {
            return Ok(None);
        }
        let want = (max_samples as u64 * self.bytes_per_sample).min(remaining);
        let mut bytes = vec![0u8; want as usize];
        let mut filled = 0usize;
        while filled < bytes.len() {
            let n = self
                .file
                .read(&mut bytes[filled..])
                .map_err(|e| AppError::Asr(format!("read wav: {e}")))?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        // Файл короче data-чанка: старый Wave::read тоже падал на этом —
        // не отдаём тихий частичный транскрипт.
        if filled < bytes.len() {
            return Err(AppError::Asr("wav file truncated".into()));
        }
        self.pos += filled as u64;
        let samples = if self.bytes_per_sample == 2 {
            bytes[..filled]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect()
        } else {
            bytes[..filled]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        };
        Ok(Some(samples))
    }
}

/// Аккумулятор результатов декода (общий для всех чанков одной задачи).
#[derive(Default)]
struct DecodeAccum {
    text: String,
    tokens: Vec<String>,
    timestamps: Vec<f32>,
    durations: Vec<f32>,
    /// Абсолютная позиция последнего декодированного сэмпла (для прогресса).
    decoded_until: usize,
    chunks: usize,
}

/// Декодировать один чанк (абсолютное начало `start_abs`). Чанк длиннее
/// `strict_limit` режется в самом тихом месте (split_chunk).
fn decode_chunk(
    recognizer: &OfflineRecognizer,
    sample_rate: i32,
    acc: &mut DecodeAccum,
    chunk: &[f32],
    start_abs: usize,
    strict_limit: usize,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let spans: Vec<(usize, usize)> = if chunk.len() > strict_limit {
        split_chunk(chunk, strict_limit)
    } else {
        vec![(0, chunk.len())]
    };
    for (rs, re) in spans {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let abs = start_abs + rs;
        let stream = recognizer.create_stream();
        stream.accept_waveform(sample_rate, &chunk[rs..re]);
        recognizer.decode(&stream);
        let r = stream
            .get_result()
            .ok_or_else(|| AppError::Asr("decode returned no result".into()))?;

        let offset_sec = abs as f32 / sample_rate as f32;
        let text = r.text.trim();
        if !text.is_empty() {
            if !acc.text.is_empty() {
                acc.text.push(' ');
            }
            acc.text.push_str(text);
        }
        acc.tokens.extend(r.tokens);
        if let Some(ts) = r.timestamps {
            acc.timestamps.extend(ts.iter().map(|&t| t + offset_sec));
        }
        if let Some(d) = r.durations {
            acc.durations.extend(d);
        }
        acc.decoded_until = abs + (re - rs);
        acc.chunks += 1;
    }
    Ok(())
}

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
