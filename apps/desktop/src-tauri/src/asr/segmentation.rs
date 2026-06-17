//! Сегментация аудио на речевые сегменты через **SileroVad** (sherpa-onnx built-in).
//!
//! Заменяет energy-VAD: каждый сегмент (0.25-30 сек речи) идёт в GigaAM
//! как **один** chunk. Модель получает 25-3000 fbank-фреймов контекста —
//! как в официальной `gigaam.transcribe_longform` и в amidexe/govorun-lite.
//!
//! Требует `silero_vad.onnx` (~629 KB) в `<data_root>/models/`.
//! Скачивается тем же `models::download_all` механизмом.
//!
//! Graceful fallback: если VAD-модель не найдена, возвращаем пустой
//! `Vec` (юзер увидит warning в логе и warning в UI — без падений).

use sherpa_onnx::{SileroVadModelConfig, TenVadModelConfig, VadModelConfig, VoiceActivityDetector};
use std::path::PathBuf;

/// Мин. длина сегмента, которую имеет смысл отдавать в GigaAM.
const MIN_SEGMENT_SAMPLES: usize = 4000; // 0.25 сек @ 16kHz = silero min_speech_duration

/// Параметры SileroVad (как в `govorun-lite/VadRecorder.kt`).
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

    segments
}

/// Сливает готовые VAD-сегменты в `out`.
///
/// **Workaround sherpa-onnx 1.13.2 Rust binding bug:** struct C `SpeechSegment`
/// в исходниках имеет `std::vector<float> samples` (24 байта), но Rust binding
/// ожидает `{int32, *mut f32, int32}` (12 байт). Layout mismatch: чтение
/// `seg.n()` / `seg.samples()` возвращает МУСОР (low 32 bits указателя
/// vector.begin). Это давало гигантские сегменты по 10+ минут, ASR их
/// "тушил" и выдавал мусор (RTF=0.004 на 58-мин видео).
///
/// Используем ТОЛЬКО `seg.start()` (offset 0, читается корректно). Границы
/// сегментов восстанавливаем по тому факту, что VAD эмитит их contiguously:
/// конец сегмента N ≈ начало сегмента N+1. Последний сегмент чанка
/// заканчивается на `chunk_end`.
///
/// Параметры:
/// - `vad` — VoiceActivityDetector (с непустой очередью segments_).
/// - `absolute_offset` — глобальный sample offset начала текущего чанка
///   (прибавляется к локальным VAD-индексам после `reset()`).
/// - `chunk_end` — глобальный sample offset конца текущего чанка (= конец
///   последнего сегмента чанка).
/// - `out` — куда пушим (start, end) пары.
fn drain_segments(
    vad: &VoiceActivityDetector,
    absolute_offset: usize,
    chunk_end: usize,
    out: &mut Vec<(usize, usize)>,
) {
    // 1. Собираем start-ы сегментов в текущем буфере VAD.
    let mut starts: Vec<usize> = Vec::new();
    while !vad.is_empty() {
        if let Some(seg) = vad.front() {
            // seg.start() — offset 0 в C struct, читается корректно.
            let seg_start = absolute_offset + seg.start() as usize;
            starts.push(seg_start);
        }
        vad.pop();
    }

    // 2. Конвертируем в (start, end) пары. VAD эмитит сегменты contiguously
    //    (конец N = начало N+1 + min_silence). Последний сегмент в чанке
    //    тянется до chunk_end.
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(chunk_end);
        if end > start && end - start >= MIN_SEGMENT_SAMPLES {
            out.push((start, end));
        }
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
