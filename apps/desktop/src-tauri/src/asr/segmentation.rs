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
/// Размер rolling-буфера VAD (секунды). Должен быть > max_speech_duration.
const VAD_BUFFER_SEC: f32 = 60.0;
/// Размер чанка, которым feed'им VAD (секунды). Должен быть < buffer_sec.
const VAD_FEED_CHUNK_SEC: usize = 30;

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

    // Stream-feed в VAD_FEED_CHUNK_SEC чанках, чтобы не превышать rolling buffer
    // (60s) и не упереться в огромный call для часового аудио.
    let feed_chunk_samples = sample_rate as usize * VAD_FEED_CHUNK_SEC;
    let mut segments: Vec<(usize, usize)> = Vec::new();

    for chunk_start in (0..samples.len()).step_by(feed_chunk_samples) {
        let chunk_end = (chunk_start + feed_chunk_samples).min(samples.len());
        vad.accept_waveform(&samples[chunk_start..chunk_end]);
        drain_segments(&vad, samples.len(), &mut segments);
    }

    // Flush: VAD удерживает последние min_silence_duration секунд в буфере
    // ожидая «может ещё речь». flush() форсирует эмиссию хвоста.
    vad.flush();
    drain_segments(&vad, samples.len(), &mut segments);

    segments
}

/// Сливает готовые VAD-сегменты в `out`, фильтруя по мин. длине.
fn drain_segments(vad: &VoiceActivityDetector, samples_len: usize, out: &mut Vec<(usize, usize)>) {
    while !vad.is_empty() {
        let seg = match vad.front() {
            Some(s) => s,
            None => break,
        };
        let seg_start = seg.start() as usize;
        let seg_n = seg.n() as usize;
        let seg_end = (seg_start + seg_n).min(samples_len);
        if seg_n >= MIN_SEGMENT_SAMPLES {
            out.push((seg_start, seg_end));
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
