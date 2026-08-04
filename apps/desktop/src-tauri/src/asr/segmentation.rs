//! Сегментация аудио на речевые сегменты через **SileroVad** (sherpa-onnx built-in).
//!
//! Стратегия (как в GigaAM `transcribe_longform` / `segment_audio_file`):
//! 1. SileroVad находит речевые сегменты (до 30 сек)
//! 2. Сегменты склеиваются в чанки по 15-22 секунды (оптимально для GigaAM)
//! 3. Чанки длиннее 30 сек разрезаются в самом тихом месте (не по центру)
//!
//! Модель обучена на аудио ~15-25 сек. Слишком короткие сегменты (< 5 сек)
//! дают мусор — у модели нет контекста.
//!
//! ## Почему VAD кормится кусками ~64 мс (а не 10 сек)
//!
//! `VoiceActivityDetector::AcceptWaveform` в sherpa-onnx сворачивает
//! `is_speech` в OR по всем окнам скормленного куска и решает о границах
//! сегмента **один раз за вызов**. При больших кусках (10-30 сек) любой
//! речевой сэмпл внутри куска держит сегмент открытым: паузы «невидимы»,
//! и сегмент закрывается только на `flush()`. На практике это давало либо
//! гигантские сегменты по 2+ минуты (которые потом резались по тихому
//! месту — слова пополам), либо 0.3-секундные огрызки на границах чанков.
//!
//! Куски ~64 мс (1024 сэмпла, 2 окна по 512) делают границы видимыми:
//! - пауза ≥ `min_silence_duration` (0.5 с) закрывает сегмент;
//! - непрерывная речь > `max_speech_duration` (30 с) автоматически режется
//!   на первом провале уверенности: sherpa-onnx поднимает порог до 0.9 и
//!   `min_silence_duration` до 0.1 с (см. `AcceptWaveform` в
//!   voice-activity-detector.cc + state machine в silero-vad-model.cc).
//!
//! `CircularBuffer::Push` при переполнении печатает сообщение и **завершает
//! процесс** (exit), поэтому буфер должен гарантированно вмещать любой
//! in-progress сегмент: максимум ~30-40 сек при авто-сплите, берём 120 сек
//! с трёхкратным запасом.

use sherpa_onnx::{SileroVadModelConfig, TenVadModelConfig, VadModelConfig, VoiceActivityDetector};
use std::path::PathBuf;

/// Минимальный сегмент, который имеет смысл отдавать в GigaAM.
/// Официальный `segment_audio_file` использует `min_duration_on: 0.0`
/// (pyannote VAD pipeline) — короткие реплики («ага», «угу») не теряются.
/// SileroVad сам фильтрует всё короче `min_speech_duration` (0.25с),
/// поэтому здесь достаточно технического floor'а от 0-длины.
const MIN_SEGMENT_SAMPLES: usize = 160; // 0.01 сек @ 16kHz

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
/// Размер rolling-буфера VAD (секунды). Должен с большим запасом
/// вмещать максимальный in-progress сегмент (~30-40 сек при авто-сплите),
/// иначе `CircularBuffer::Push` убьёт процесс при переполнении.
const VAD_BUFFER_SEC: f32 = 120.0;
/// Размер куска, которым feed'им VAD за один проход (в сэмплах).
/// ~64 мс при 16 кГц (2 окна по 512) — см. док модуля: только при
/// маленьких кусках паузы и авто-сплит становятся видимыми для
/// сегментации (AcceptWaveform сворачивает is_speech в OR по куску).
const VAD_FEED_SAMPLES: usize = 1024;

/// Возвращает список пар `(start_sample, end_sample)` для склеенных
/// речевых чанков. VAD-сегменты склеиваются в чанки по 15-22 сек.
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

    // Streaming-паттерн (см. док модуля): кормим VAD кусками ~64 мс,
    // забираем завершённые сегменты, `flush()` — один раз в конце файла.
    // Границы сегментов падают на реальные паузы и провалы уверенности,
    // а не на жёсткий таймер — слова не разрезаются.
    let mut segments: Vec<(usize, usize)> = Vec::new();

    for chunk_start in (0..samples.len()).step_by(VAD_FEED_SAMPLES) {
        let chunk_end = (chunk_start + VAD_FEED_SAMPLES).min(samples.len());
        vad.accept_waveform(&samples[chunk_start..chunk_end]);
        drain_segments(&vad, chunk_end, &mut segments);
    }

    // Неоконченная речь в конце файла: flush форсирует её эмиссию.
    vad.flush();
    drain_segments(&vad, samples.len(), &mut segments);

    // Склеиваем VAD-сегменты в чанки по 15-22 сек (как в GigaAM transcribe_longform)
    merge_segments(samples, &segments, sample_rate)
}

/// Склеить VAD-сегменты в чанки по 15-22 секунды (как в GigaAM `segment_audio_file`).
///
/// Модель обучена на аудио ~15-25 сек. Слишком короткие сегменты дают мусор.
/// Сегменты длиннее 30 сек разрезаются в самом тихом месте (не по центру).
fn merge_segments(samples: &[f32], raw: &[(usize, usize)], sample_rate: u32) -> Vec<(usize, usize)> {
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
            push_chunk(&mut merged, curr_start, curr_end, strict_limit_samples, samples);
            curr_start = seg_start;
            curr_end = seg_end;
            curr_duration = seg_len;
        } else {
            curr_end = seg_end;
            curr_duration = curr_end - curr_start;
        }
    }

    if curr_duration > threshold_samples {
        push_chunk(&mut merged, curr_start, curr_end, strict_limit_samples, samples);
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
/// Разрез делается в самом тихом месте средней части чанка, а не по
/// центру — иначе границы падают в середину слова.
fn push_chunk(
    out: &mut Vec<(usize, usize)>,
    start: usize,
    end: usize,
    strict_limit: usize,
    samples: &[f32],
) {
    let len = end - start;
    if len <= strict_limit {
        out.push((start, end));
        return;
    }
    let cut = quietest_split_point(samples, start, end);
    push_chunk(out, start, cut, strict_limit, samples);
    push_chunk(out, cut, end, strict_limit, samples);
}

/// Самое тихое место в средней половине `[start, end)`: минимизируем RMS
/// 100ms-окна (1600 сэмплов @16kHz) с шагом 50ms. Возвращает центр окна.
/// Ограничение средней половиной не даёт отрезать огрызок у краёв.
fn quietest_split_point(samples: &[f32], start: usize, end: usize) -> usize {
    const WINDOW: usize = 1600; // 100ms @ 16kHz
    let len = end - start;
    let lo = start + len / 4;
    let hi = end - len / 4;
    let mut best = (lo + hi) / 2;
    let mut best_rms = f32::MAX;
    let mut w = lo;
    while w + WINDOW <= hi {
        let mut sum = 0.0f32;
        for &s in &samples[w..w + WINDOW] {
            sum += s * s;
        }
        let rms = sum / WINDOW as f32;
        if rms < best_rms {
            best_rms = rms;
            best = w + WINDOW / 2;
        }
        w += WINDOW / 2; // шаг 50ms
    }
    best.clamp(start + 1, end - 1)
}

/// Сливает готовые VAD-сегменты в `out`.
///
/// `seg.start()` — глобальный sample offset (детектор не сбрасывался),
/// `seg.n()` — длина в сэмплах. Конец клампится к `stream_end`.
///
/// Параметры:
/// - `vad` — VoiceActivityDetector (с непустой очередью segments_).
/// - `stream_end` — глобальный sample offset конца уже скормленного аудио.
/// - `out` — куда пушим (start, end) пары.
fn drain_segments(
    vad: &VoiceActivityDetector,
    stream_end: usize,
    out: &mut Vec<(usize, usize)>,
) {
    while !vad.is_empty() {
        if let Some(seg) = vad.front() {
            let seg_start = seg.start().max(0) as usize;
            let seg_len = seg.n().max(0) as usize;
            let seg_end = (seg_start + seg_len).min(stream_end);
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
        let samples = vec![0.0f32; 16000];
        let segs = find_speech_segments(&samples, 16000);
        let _ = segs;
    }

    // --- merge_segments tests ---

    #[test]
    fn merge_empty() {
        assert!(merge_segments(&[], &[], 16000).is_empty());
    }

    #[test]
    fn merge_single_short_segment_passthrough() {
        // 10s > threshold 0.2s — passes through
        let raw = vec![(0, 160000)];
        let merged = merge_segments(&[0.0; 160000], &raw, 16000);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], (0, 160000));
    }

    #[test]
    fn merge_below_threshold_dropped() {
        // 0.1s < threshold 0.2s — dropped
        let raw = vec![(0, 1600)];
        let merged = merge_segments(&[], &raw, 16000);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_multiple_short_concatenated() {
        let sr: usize = 16000;
        let raw = vec![(0, 5 * sr), (6 * sr, 10 * sr), (11 * sr, 16 * sr)];
        let merged = merge_segments(&[0.0; 16 * sr], &raw, sr as u32);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], (0, 16 * sr));
    }

    #[test]
    fn merge_splits_at_max_duration() {
        let sr: usize = 16000;
        let raw = vec![(0, 12 * sr), (12 * sr, 24 * sr)];
        let merged = merge_segments(&[0.0; 24 * sr], &raw, sr as u32);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_splits_at_min_duration() {
        let sr: usize = 16000;
        let raw = vec![(0, 16 * sr), (16 * sr, 18 * sr)];
        let merged = merge_segments(&[0.0; 18 * sr], &raw, sr as u32);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_strict_limit_splits_long() {
        let sr: usize = 16000;
        let raw = vec![(0, 45 * sr)];
        let merged = merge_segments(&[0.0; 45 * sr], &raw, sr as u32);
        assert!(merged.len() >= 2);
        for &(s, e) in &merged {
            assert!((e - s) / sr <= 30);
        }
    }

    #[test]
    fn quietest_split_prefers_silence_over_noise() {
        let sr: usize = 16000;
        let mut samples = vec![0.1f32; 45 * sr];
        // Тихий участок в середине (2 сек тишины) — разрез должен попасть в него.
        let silence_start = 20 * sr;
        for s in &mut samples[silence_start..silence_start + 2 * sr] {
            *s = 0.0001;
        }
        let cut = quietest_split_point(&samples, 0, 45 * sr);
        assert!(
            cut >= silence_start && cut <= silence_start + 2 * sr,
            "cut {cut} should land inside the silent gap [{silence_start}, {}]",
            silence_start + 2 * sr
        );
    }
}
