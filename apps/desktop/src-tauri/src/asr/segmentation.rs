//! Energy-based VAD: режет long audio на речевые сегменты по паузам.
//!
//! Замена pyannote из `gigaam.transcribe_longform` для sherpa-onnx-стека.
//! Границы — на тишине (≥ 300ms), не посреди слова. Короткие речевые
//! всплески (< 500ms) фильтруются как шум.
//!
//! Параметры по умолчанию валидированы на 74-мин вебинаре (бенчмарк в
//! `bin/`): 1,476 сегментов, 78% < 1с, 21% 1-3с, 0.5% 3-5с, max=4.08с.

/// Параметры алгоритма (стартовые, тюнятся под шумность записи).
const WINDOW_MS: usize = 20;
const HOP_MS: usize = 10;
/// Нормализованный RMS выше этого порога → считается речью.
/// Тихие кабинетные записи обычно имеют RMS 0.05-0.5, шум < 0.01.
const ENERGY_THRESHOLD: f32 = 0.01;
/// Тишина короче этого (внутри-словные паузы) — схлопывается с речью.
const MIN_SILENCE_MS: u32 = 300;
/// Речь короче этого (всплески шума/эхо) — отбрасывается.
const MIN_SPEECH_MS: u32 = 500;

/// Возвращает список пар `(start_sample, end_sample)` для каждого
/// найденного речевого сегмента. Диапазоны — в исходных sample'ах.
/// Возвращает пустой `Vec` для пустого входа, нулевого `sample_rate`
/// или полностью тихой записи.
pub fn find_speech_segments(samples: &[f32], sample_rate: u32) -> Vec<(usize, usize)> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let window = (WINDOW_MS * sample_rate as usize) / 1000;
    let hop = (HOP_MS * sample_rate as usize) / 1000;
    if window == 0 || hop == 0 || window > samples.len() {
        return Vec::new();
    }
    let total_windows = (samples.len() - window) / hop + 1;
    // Ceiling division без `div_ceil` для совместимости со старым rustc.
    let min_silence_w = ((MIN_SILENCE_MS as usize * sample_rate as usize) / 1000 + hop - 1) / hop;
    let min_speech_w = ((MIN_SPEECH_MS as usize * sample_rate as usize) / 1000 + hop - 1) / hop;

    // 1. RMS per window → is_speech.
    let mut is_speech: Vec<bool> = Vec::with_capacity(total_windows);
    for w in 0..total_windows {
        let start = w * hop;
        let sum_sq: f32 = samples[start..start + window].iter().map(|s| s * s).sum();
        let rms = (sum_sq / window as f32).sqrt();
        is_speech.push(rms > ENERGY_THRESHOLD);
    }

    // 2. RLE: (is_speech, len_windows).
    let mut runs: Vec<(bool, usize)> = Vec::new();
    for &val in &is_speech {
        match runs.last_mut() {
            Some((last_val, count)) if *last_val == val => *count += 1,
            _ => runs.push((val, 1)),
        }
    }

    // 3. Pass 1: короткие silence-острова → speech (схлопываем межсловные паузы).
    for r in runs.iter_mut() {
        if !r.0 && r.1 < min_silence_w {
            r.0 = true;
        }
    }

    // 4. Pass 2: короткие speech-острова → silence (отбрасываем шумовые всплески).
    for r in runs.iter_mut() {
        if r.0 && r.1 < min_speech_w {
            r.0 = false;
        }
    }

    // 5. Merge adjacent same-value runs (после pass 1 могли появиться стыки).
    let mut merged: Vec<(bool, usize)> = Vec::with_capacity(runs.len());
    for r in runs {
        if let Some((last_val, count)) = merged.last_mut() {
            if *last_val == r.0 {
                *count += r.1;
                continue;
            }
        }
        merged.push(r);
    }

    // 6. Собрать речевые сегменты. Последнее окно «добивает» WINDOW семплов
    //    вправо — пересечение соседних сегментов на WINDOW семплов (~20ms) —
    //    это OK (модель получит короткий overlap в обе стороны, без потерь).
    let mut segments = Vec::new();
    let mut offset = 0usize;
    for (is_speech_run, len) in merged {
        if is_speech_run {
            let start_sample = offset * hop;
            let end_sample = ((offset + len) * hop + window).min(samples.len());
            segments.push((start_sample, end_sample));
        }
        offset += len;
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: u32 = 16000;
    const WIN: usize = (WINDOW_MS * SR as usize) / 1000; // 320
    const HOP: usize = (HOP_MS * SR as usize) / 1000; // 160

    fn silence(n: usize) -> Vec<f32> {
        vec![0.0; n]
    }

    /// 1 kHz sine wave с амплитудой `amp` — RMS ≈ amp/√2, чётко выше порога 0.01.
    fn sine(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * 1000.0 * i as f32 / SR as f32).sin())
            .collect()
    }

    fn ms_to_samples(ms: u32) -> usize {
        (ms as usize * SR as usize) / 1000
    }

    #[test]
    fn empty_input() {
        assert!(find_speech_segments(&[], SR).is_empty());
    }

    #[test]
    fn zero_sample_rate() {
        let s = sine(1000, 0.5);
        assert!(find_speech_segments(&s, 0).is_empty());
    }

    #[test]
    fn shorter_than_window() {
        let s = sine(100, 0.5); // < 320 samples
        assert!(find_speech_segments(&s, SR).is_empty());
    }

    #[test]
    fn all_silence() {
        let s = silence(SR * 2);
        assert!(find_speech_segments(&s, SR).is_empty());
    }

    #[test]
    fn single_burst() {
        // 1s silence + 1s speech + 1s silence → 1 сегмент.
        let mut s = silence(SR);
        s.extend(sine(SR, 0.5));
        s.extend(silence(SR));
        let segs = find_speech_segments(&s, SR);
        assert_eq!(segs.len(), 1, "1 burst → 1 segment, got {:?}", segs);
        let (start, end) = segs[0];
        // Допуск ±WIN из-за скользящего окна.
        assert!(
            start >= SR - WIN,
            "start={start} should be near {sr}",
            sr = SR
        );
        assert!(
            end <= SR * 2 + WIN,
            "end={end} should be near {v}",
            v = SR * 2
        );
    }

    #[test]
    fn two_bursts_long_gap_splits() {
        // 1s gap > MIN_SILENCE (300ms) → 2 сегмента.
        let mut s = silence(SR);
        s.extend(sine(SR, 0.5));
        s.extend(silence(SR));
        s.extend(sine(SR, 0.5));
        s.extend(silence(SR));
        let segs = find_speech_segments(&s, SR);
        assert_eq!(segs.len(), 2, "1s gap → 2 segments, got {:?}", segs);
    }

    #[test]
    fn two_bursts_short_gap_merges() {
        // 200ms gap < MIN_SILENCE (300ms) → 1 сегмент.
        let mut s = silence(SR);
        s.extend(sine(SR, 0.5));
        s.extend(silence(ms_to_samples(200)));
        s.extend(sine(SR, 0.5));
        s.extend(silence(SR));
        let segs = find_speech_segments(&s, SR);
        assert_eq!(segs.len(), 1, "200ms gap should merge, got {:?}", segs);
    }

    #[test]
    fn noise_burst_filtered() {
        // 1s speech + 500ms gap + 200ms noise + 500ms gap + 1s speech.
        // 200ms шумового «всплеска» < MIN_SPEECH (500ms) → отбрасывается,
        // итого: 1s speech + 1.2s silence + 1s speech → 2 сегмента.
        let mut s = silence(SR);
        s.extend(sine(SR, 0.5));
        s.extend(silence(ms_to_samples(500)));
        s.extend(sine(ms_to_samples(200), 0.5));
        s.extend(silence(ms_to_samples(500)));
        s.extend(sine(SR, 0.5));
        s.extend(silence(SR));
        let segs = find_speech_segments(&s, SR);
        assert_eq!(
            segs.len(),
            2,
            "filtered noise + 1.2s gap → 2 segments, got {:?}",
            segs
        );
    }

    #[test]
    fn low_amplitude_below_threshold() {
        // Очень тихий sine (amp 0.005) — RMS < 0.01 → всё тишина.
        let mut s = silence(SR);
        s.extend(sine(SR, 0.005));
        s.extend(silence(SR));
        assert!(find_speech_segments(&s, SR).is_empty());
    }
}
