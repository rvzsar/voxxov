//! Чтение WAV и нарезка длинного аудио на чанки для ASR.
//!
//! `ffmpeg` всегда пишет 16kHz mono PCM s16le WAV (см. `ffmpeg.rs`),
//! но `hound` корректно читает и другие форматы — на случай если
//! пользователь передаст произвольный `.wav`.

use crate::error::{AppError, AppResult};
use std::path::Path;

/// Семплы PCM f32 в моно-формате + sample rate.
pub struct AudioSamples {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioSamples {
    pub fn duration_sec(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

/// Прочитать WAV-файл и вернуть моно-f32 семплы. Стерео усредняется.
pub fn read_wav_samples(path: &Path) -> AppResult<AudioSamples> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| AppError::Asr(format!("open wav {}: {e}", path.display())))?;
    let spec = reader.spec();
    let max_val: f32 = match spec.bits_per_sample {
        8 => 128.0,
        16 => 32_768.0,
        24 => 8_388_608.0,
        32 => 2_147_483_648.0,
        _ => 1.0, // f32-фоллбэк для 64-bit и пр.
    };
    let channels = spec.channels.max(1) as usize;

    let raw: Vec<f32> = if spec.bits_per_sample == 16 {
        reader
            .into_samples::<i16>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / 32768.0)
            .collect()
    } else if spec.bits_per_sample == 32 {
        reader
            .into_samples::<i32>()
            .filter_map(|s| s.ok())
            .map(|s| s as f32 / max_val)
            .collect()
    } else {
        // fallback: f32 (8/24/64-bit не поддерживаем — будет падать)
        reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .collect()
    };

    let mut mono = Vec::with_capacity(raw.len() / channels);
    for chunk in raw.chunks(channels) {
        let sum: f32 = chunk.iter().copied().sum();
        mono.push(sum / channels as f32);
    }
    Ok(AudioSamples {
        samples: mono,
        sample_rate: spec.sample_rate,
    })
}

/// Один сегмент аудио + его смещение в исходном файле (в секундах).
pub struct Segment {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub offset_sec: f32,
}

/// Нарезать моно-f32 семплы на чанки фиксированной длины с overlap.
/// Если аудио короче `segment_sec` — возвращается один сегмент (весь файл).
pub fn split_into_segments(
    samples: &[f32],
    sample_rate: u32,
    segment_sec: f32,
    overlap_sec: f32,
) -> Vec<Segment> {
    if samples.is_empty() {
        return Vec::new();
    }
    let seg_len = ((segment_sec * sample_rate as f32).round() as usize).max(1);
    let overlap = ((overlap_sec.max(0.0) * sample_rate as f32).round() as usize).min(seg_len / 2);
    let stride = seg_len.saturating_sub(overlap).max(1);

    if samples.len() <= seg_len {
        return vec![Segment {
            samples: samples.to_vec(),
            sample_rate,
            offset_sec: 0.0,
        }];
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    while start < samples.len() {
        let end = (start + seg_len).min(samples.len());
        out.push(Segment {
            samples: samples[start..end].to_vec(),
            sample_rate,
            offset_sec: start as f32 / sample_rate as f32,
        });
        if end == samples.len() {
            break;
        }
        start += stride;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_short_returns_one() {
        let sr = 100;
        let s = vec![0.0; 50];
        let segs = split_into_segments(&s, sr, 1.0, 0.1);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].offset_sec, 0.0);
    }

    #[test]
    fn split_long_chunks_with_overlap() {
        let sr = 100;
        // 3 секунды, сегмент 1с, overlap 0.2с → stride 0.8с → ~4 сегмента
        let s = vec![0.0; 300];
        let segs = split_into_segments(&s, sr, 1.0, 0.2);
        assert!(segs.len() >= 3);
        assert_eq!(segs[0].offset_sec, 0.0);
        // последний сегмент должен доходить до конца
        let last = segs.last().unwrap();
        let last_end = (last.samples.len() as f32 / sr as f32) + last.offset_sec;
        assert!((last_end - 3.0).abs() < 0.05, "last_end={last_end}");
    }
}
