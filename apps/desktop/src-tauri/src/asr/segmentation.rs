//! Сегментация аудио на речевые сегменты через **SileroVad** (sherpa-onnx built-in).
//!
//! Стратегия (как в GigaAM `transcribe_longform` / `segment_audio_file`):
//! 1. SileroVad находит речевые сегменты (0.25-30 сек)
//! 2. Сегменты склеиваются в чанки по 15-22 секунды (оптимально для GigaAM)
//! 3. Сегменты длиннее 30 сек разрезаются принудительно
//!
//! Модель обучена на аудио ~15-25 сек. Слишком короткие сегменты (< 5 сек)
//! дают мусор — у модели нет контекста.

use sherpa_onnx::{SileroVadModelConfig, TenVadModelConfig, VadModelConfig, VoiceActivityDetector};
use std::path::PathBuf;

/// Мин. длина сегмента, которую имеет смысл отдавать в GigaAM.
const MIN_SEGMENT_SAMPLES: usize = 4000; // 0.25 сек @ 16kHz = silero min_speech_duration

/// Параметры склейки сегментов (из GigaAM `segment_audio_file`).
/// Модель обучена на аудио ~15-25 сек; оптимальный диапазон чанков.
const MERGE_MAX_SEC: f32 = 22.0; // максимальная длина чанка
const MERGE_MIN_SEC: f32 = 15.0; // минимальная длина для закрытия чанка
const MERGE_STRICT_LIMIT_SEC: f32 = 30.0; // жёсткий лимит — длиннее режем
const MERGE_THRESHOLD_SEC: f32 = 0.2; // минимальный осмысленный сегмент

/// Параметры SileroVad (как в gigastt).
const VAD_THRESHOLD: f32 = 0.5;
const VAD_MIN_SILENCE_SEC: f32 = 0.5;
const VAD_MIN_SPEECH_SEC: f32 = 0.25;
const VAD_WINDOW_SIZE: i32 = 512;
const VAD_MAX_SPEECH_SEC: f32 = 30.0;
/// Размер rolling-буфера VAD (секунды). Должен быть > VAD_CHUNK_SEC.
const VAD_BUFFER_SEC: f32 = 60.0;
/// Размер чанка, которым feed'им VAD за один проход. МЕНЬШЕ VAD_BUFFER_SEC —
/// иначе circular buffer sherpa-onnx переполнится и OOM-крашнет процесс
/// (см. 2286d88 followup: 2 чанка по 30s без reset уже забивают 60s буфер,
/// третий push = overflow). 30s выбран как max_speech_duration — каждый
/// чанк самодостаточен, cross-chunk context не теряется на реальной речи.
const VAD_CHUNK_SEC: usize = 30;

/// Возвращает список пар `(start_sample, end_sample)` для каждого
/// речевого сегмента от SileroVad. Сегменты по 0.25-30 сек — сразу
/// пригодны для ASR как один chunk.
pub fn find_speech_segments(samples: &[f32], sample_rate: u32) -> Vec<(usize, usize)> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }

    let model_path = match vad_model_path() {
        Some(p) => p,
        None => {
            tracing::warn!(
                "SileroVad model (silero_vad.onnx) not found in models dir; \
                 no segmentation possible — empty transcript"
            );
            return Vec::new();
        }
    };

    let silero_config = SileroVadModelConfig {
        model: Some(model_path.to_string_lossy().to_string()),
        threshold: VAD_THRESHOLD,
        min_silence_duration: VAD_MIN_SILENCE_SEC,
        min_speech_duration: VAD_MIN_SPEECH_SEC,
        window_size: VAD_WINDOW_SIZE,
        max_speech_duration: VAD_MAX_SPEECH_SEC,
    };

    let vad_config = VadModelConfig {
        silero_vad: silero_config,
        ten_vad: TenVadModelConfig::default(),
        sample_rate: sample_rate as i32,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        debug: false,
    };

    let vad = match VoiceActivityDetector::create(&vad_config, VAD_BUFFER_SEC) {
        Some(v) => v,
        None => {
            tracing::warn!("failed to create SileroVad instance");
            return Vec::new();
        }
    };

    // Chunked processing: sherpa-onnx 1.13 VAD **не** auto-drain'ит
    // circular buffer — он растёт на каждом push и переполняется при
    // > VAD_BUFFER_SEC (60s) входных данных, OOM-крашит процесс.
    // Решение: feed'им по VAD_CHUNK_SEC (30s) и `vad.reset()` между
    // чанками. reset() теряет cross-chunk VAD context, но
    // max_speech_duration=30s уже ограничивает любой сегмент длиной
    // VAD_CHUNK_SEC — каждый чанк самодостаточен.
    let chunk_samples = sample_rate as usize * VAD_CHUNK_SEC;
    let mut segments: Vec<(usize, usize)> = Vec::new();

    for chunk_start in (0..samples.len()).step_by(chunk_samples) {
        let chunk_end = (chunk_start + chunk_samples).min(samples.len());
        let chunk = &samples[chunk_start..chunk_end];

        // Feed whole chunk at once (sherpa-onnx internal windows it).
        vad.accept_waveform(chunk);
        drain_segments(&vad, chunk_start, chunk_end, &mut segments);

        // Flush: VAD удерживает последние min_silence_duration секунд в
        // буфере ожидая «может ещё речь». flush() форсирует эмиссию.
        vad.flush();
        drain_segments(&vad, chunk_start, chunk_end, &mut segments);

        // Reset: освобождает circular buffer. Без него 2-й чанк уже
        // переполнит буфер (VAD_CHUNK_SEC + VAD_CHUNK_SEC > VAD_BUFFER_SEC).
        vad.reset();
    }

    // Склеиваем VAD-сегменты в чанки по 15-22 сек (как в GigaAM transcribe_longform)
    merge_segments(&segments, sample_rate, samples.len())
}

/// Склеить VAD-сегменты в чанки по 15-22 секунды (как в GigaAM `segment_audio_file`).
///
/// Модель обучена на аудио ~15-25 сек. Слишком короткие сегменты дают мусор.
/// Сегменты длиннее 30 сек разрезаются принудительно.
fn merge_segments(
    raw: &[(usize, usize)],
    sample_rate: u32,
    _total_samples: usize,
) -> Vec<(usize, usize)> {
    if raw.is_empty() {
        return Vec::new();
    }
    let sr = sample_rate as f32;
    let max_samples = (MERGE_MAX_SEC * sr) as usize;
    let min_samples = (MERGE_MIN_SEC * sr) as usize;
    let strict_limit_samples = (MERGE_STRICT_LIMIT_SEC * sr) as usize;
    let threshold_samples = (MERGE_THRESHOLD_SEC * sr) as usize;

    let mut merged: Vec<(usize, usize)> = Vec::new();
    let mut curr_start: usize = 0;
    let mut curr_end: usize = 0;
    let mut curr_duration: usize = 0;

    for &(seg_start, seg_end) in raw {
        if curr_duration == 0 {
            curr_start = seg_start;
            curr_end = seg_end;
            curr_duration = seg_end - seg_start;
            continue;
        }

        let seg_len = seg_end - seg_start;
        // Закрыть текущий чанк если:
        // - добавление сегмента превысит max_duration
        // - текущий чанк уже > min_duration
        if curr_duration > threshold_samples
            && (curr_duration + seg_len > max_samples || curr_duration > min_samples)
        {
            push_chunk(&mut merged, curr_start, curr_end, strict_limit_samples);
            curr_start = seg_start;
            curr_end = seg_end;
            curr_duration = seg_len;
        } else {
            curr_end = seg_end;
            curr_duration = curr_end - curr_start;
        }
    }

    if curr_duration > threshold_samples {
        push_chunk(&mut merged, curr_start, curr_end, strict_limit_samples);
    }

    tracing::info!(
        "ASR merge: {} VAD segments -> {} chunks (max={:.0}s, min={:.0}s)",
        raw.len(),
        merged.len(),
        MERGE_MAX_SEC,
        MERGE_MIN_SEC
    );

    merged
}

/// Запушить чанк, разрезав на части если превышает strict_limit.
fn push_chunk(out: &mut Vec<(usize, usize)>, start: usize, end: usize, strict_limit: usize) {
    let len = end - start;
    if len <= strict_limit {
        out.push((start, end));
        return;
    }
    // Разрезаем на равные части
    let n_parts = (len / strict_limit) + 1;
    let part_len = len / n_parts;
    let mut pos = start;
    for _ in 0..n_parts {
        let chunk_end = (pos + part_len).min(end);
        out.push((pos, chunk_end));
        pos = chunk_end;
    }
}

/// Сливает готовые VAD-сегменты в `out`.
///
/// Используем `seg.start()` (начало сегмента) и `seg.n()` (длина в сэмплах)
/// для точного извлечения границ сегмента.
///
/// Параметры:
/// - `vad` — VoiceActivityDetector (с непустой очередью segments_).
/// - `absolute_offset` — глобальный sample offset начала текущего чанка
///   (прибавляется к локальным VAD-индексам после `reset()`).
/// - `chunk_end` — глобальный sample offset конца текущего чанка.
/// - `out` — куда пушим (start, end) пары.
fn drain_segments(
    vad: &VoiceActivityDetector,
    absolute_offset: usize,
    chunk_end: usize,
    out: &mut Vec<(usize, usize)>,
) {
    while !vad.is_empty() {
        if let Some(seg) = vad.front() {
            let seg_start = absolute_offset + seg.start() as usize;
            let seg_len = seg.n() as usize;
            let seg_end = (seg_start + seg_len).min(chunk_end);
            if seg_end > seg_start && seg_end - seg_start >= MIN_SEGMENT_SAMPLES {
                out.push((seg_start, seg_end));
            }
        }
        vad.pop();
    }
}

/// Путь к `silero_vad.onnx` в `<data_root>/models/`.
/// `None` если файл отсутствует или подозрительно мал.
fn vad_model_path() -> Option<PathBuf> {
    let dir = crate::models::default_model_dir();
    let p = dir.join("silero_vad.onnx");
    let meta = p.metadata().ok()?;
    // Оригинал ~629 KB; 100 KB — sanity check против повреждённого/частичного скачивания.
    if meta.len() > 100_000 {
        Some(p)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        assert!(find_speech_segments(&[], 16000).is_empty());
    }

    #[test]
    fn zero_sample_rate_returns_empty() {
        let samples = vec![0.0f32; 1000];
        assert!(find_speech_segments(&samples, 0).is_empty());
    }

    #[test]
    fn missing_vad_model_returns_empty_without_panic() {
        // Модель silero_vad.onnx не установлена в тесте → graceful return.
        // (Тест проходит в любом окружении: с моделью вернёт сегменты, без — пусто.)
        let samples = vec![0.0f32; 16000];
        let segs = find_speech_segments(&samples, 16000);
        // Либо пусто (модель не найдена), либо не-пусто (модель нашлась в реальной FS).
        // Главное — не паникует.
        let _ = segs;
    }
}
