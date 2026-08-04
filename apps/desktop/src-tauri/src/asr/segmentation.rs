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
//! ## Стриминг
//!
//! Пайплайн — один проход: `VadSegmenter::feed()` кормит VAD кусками ~64 мс
//! и отдаёт завершённые сегменты; `ChunkAssembler` склеивает их в чанки
//! 15-22 сек и закрывает готовые. В памяти держится только текущий чанк
//! (≤ ~52 сек сэмплов), а не весь файл.
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
/// Жёсткий лимит чанка — длиннее режем (использует и engine для split_chunk).
pub const MERGE_STRICT_LIMIT_SEC: f32 = 30.0;
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
pub const VAD_FEED_SAMPLES: usize = 1024;

/// Стриминговый VAD: кормим куски ~64 мс, получаем завершённые сегменты
/// (глобальные sample-индексы). `None` — модель SileroVad недоступна;
/// движок в этом случае вернёт пустой транскрипт.
pub struct VadSegmenter {
    vad: VoiceActivityDetector,
}

impl VadSegmenter {
    pub fn new(sample_rate: u32) -> Option<Self> {
        // GigaAM/SileroVad работают только на 16 кГц; C-код sherpa при
        // другом sample_rate вызывает exit() процесса — защищаемся до
        // создания детектора.
        if sample_rate != 16000 {
            tracing::warn!("ASR: unsupported sample rate {sample_rate} (need 16000)");
            return None;
        }
        let model_path = vad_model_path()?;
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
                return None;
            }
        };
        Some(Self { vad })
    }

    /// Скормить кусок сэмплов; вернуть завершённые сегменты.
    pub fn feed(&mut self, piece: &[f32], stream_end: usize) -> Vec<(usize, usize)> {
        self.vad.accept_waveform(piece);
        let mut out = Vec::new();
        drain_segments(&self.vad, stream_end, &mut out);
        out
    }

    /// Flush на конце файла; вернуть последние сегменты.
    pub fn finish(&mut self, stream_end: usize) -> Vec<(usize, usize)> {
        self.vad.flush();
        let mut out = Vec::new();
        drain_segments(&self.vad, stream_end, &mut out);
        out
    }
}

/// Инкрементальная склейка VAD-сегментов в чанки 15-22 сек.
/// Тот же алгоритм, что `segment_audio_file` в GigaAM, но по одному
/// сегменту за раз: `feed` возвращает закрытый чанк (не больше одного),
/// `finish` — финальный частичный чанк.
pub struct ChunkAssembler {
    max_samples: usize,
    min_samples: usize,
    threshold_samples: usize,
    curr_start: usize,
    curr_end: usize,
    curr_duration: usize,
}

impl ChunkAssembler {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        Self {
            max_samples: (MERGE_MAX_SEC * sr) as usize,
            min_samples: (MERGE_MIN_SEC * sr) as usize,
            threshold_samples: (MERGE_THRESHOLD_SEC * sr) as usize,
            curr_start: 0,
            curr_end: 0,
            curr_duration: 0,
        }
    }

    /// Текущее начало чанка — относительно него движок держит буфер сэмплов.
    pub fn curr_start(&self) -> usize {
        self.curr_start
    }

    /// Закрытые чанки (абсолютные `(start, end)`); максимум один за вызов.
    pub fn feed(&mut self, seg: (usize, usize)) -> Vec<(usize, usize)> {
        let (seg_start, seg_end) = seg;
        if self.curr_duration == 0 {
            self.curr_start = seg_start;
            self.curr_end = seg_end;
            self.curr_duration = seg_end - seg_start;
            return Vec::new();
        }
        let seg_len = seg_end - seg_start;
        // Закрыть текущий чанк если:
        // - добавление сегмента превысит max_duration
        // - текущий чанк уже > min_duration
        if self.curr_duration > self.threshold_samples
            && (self.curr_duration + seg_len > self.max_samples
                || self.curr_duration > self.min_samples)
        {
            let closed = vec![(self.curr_start, self.curr_end)];
            self.curr_start = seg_start;
            self.curr_end = seg_end;
            self.curr_duration = seg_len;
            closed
        } else {
            self.curr_end = seg_end;
            self.curr_duration = self.curr_end - self.curr_start;
            Vec::new()
        }
    }

    /// Финальный частичный чанк на конце файла.
    pub fn finish(&mut self) -> Vec<(usize, usize)> {
        if self.curr_duration > self.threshold_samples {
            let closed = vec![(self.curr_start, self.curr_end)];
            self.curr_start = 0;
            self.curr_end = 0;
            self.curr_duration = 0;
            closed
        } else {
            Vec::new()
        }
    }
}

/// Разрезать чанк на части ≤ `strict_limit` (относительные индексы).
/// Разрез — в самом тихом месте средней части, а не по центру — иначе
/// границы падают в середину слова.
pub fn split_chunk(chunk: &[f32], strict_limit: usize) -> Vec<(usize, usize)> {
    fn split_rec(
        out: &mut Vec<(usize, usize)>,
        chunk: &[f32],
        start: usize,
        end: usize,
        strict: usize,
    ) {
        let len = end - start;
        if len <= strict {
            out.push((start, end));
            return;
        }
        let cut = quietest_split_point(chunk, start, end);
        split_rec(out, chunk, start, cut, strict);
        split_rec(out, chunk, cut, end, strict);
    }
    let mut out = Vec::new();
    split_rec(&mut out, chunk, 0, chunk.len(), strict_limit);
    out
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
fn drain_segments(vad: &VoiceActivityDetector, stream_end: usize, out: &mut Vec<(usize, usize)>) {
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

    // --- ChunkAssembler: те же сценарии, что у merge_segments в GigaAM ---

    #[test]
    fn assembler_empty() {
        let mut a = ChunkAssembler::new(16000);
        assert!(a.finish().is_empty());
    }

    #[test]
    fn assembler_single_short_segment_passthrough() {
        // 10s > threshold 0.2s — проходит целиком.
        let mut a = ChunkAssembler::new(16000);
        assert!(a.feed((0, 160000)).is_empty());
        assert_eq!(a.finish(), vec![(0, 160000)]);
    }

    #[test]
    fn assembler_below_threshold_dropped() {
        // 0.1s < threshold 0.2s — отбрасывается.
        let mut a = ChunkAssembler::new(16000);
        assert!(a.feed((0, 1600)).is_empty());
        assert!(a.finish().is_empty());
    }

    #[test]
    fn assembler_multiple_short_concatenated() {
        let sr: usize = 16000;
        let mut a = ChunkAssembler::new(sr as u32);
        assert!(a.feed((0, 5 * sr)).is_empty());
        assert!(a.feed((6 * sr, 10 * sr)).is_empty());
        assert!(a.feed((11 * sr, 16 * sr)).is_empty());
        assert_eq!(a.finish(), vec![(0, 16 * sr)]);
    }

    #[test]
    fn assembler_splits_at_max_duration() {
        let sr: usize = 16000;
        let mut a = ChunkAssembler::new(sr as u32);
        assert!(a.feed((0, 12 * sr)).is_empty());
        assert_eq!(a.feed((12 * sr, 24 * sr)), vec![(0, 12 * sr)]);
        assert_eq!(a.finish(), vec![(12 * sr, 24 * sr)]);
    }

    #[test]
    fn assembler_splits_at_min_duration() {
        let sr: usize = 16000;
        let mut a = ChunkAssembler::new(sr as u32);
        assert!(a.feed((0, 16 * sr)).is_empty());
        assert_eq!(a.feed((16 * sr, 18 * sr)), vec![(0, 16 * sr)]);
        assert_eq!(a.finish(), vec![(16 * sr, 18 * sr)]);
    }

    // --- split_chunk / quietest_split_point ---

    #[test]
    fn split_chunk_limits_long_chunk() {
        let sr: usize = 16000;
        let chunk = vec![0.0f32; 45 * sr];
        let pieces = split_chunk(&chunk, 30 * sr);
        assert!(pieces.len() >= 2);
        for &(s, e) in &pieces {
            assert!(e - s <= 30 * sr);
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
