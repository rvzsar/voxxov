//! ASR-движок. Sherpa-onnx (static C library, native Rust bindings).
//!
//! Если `model_dir` начинается с `cmd:` — fallback на внешний CLI
//! (см. `cmd_fallback`). Иначе — прямой вызов sherpa-onnx.
//!
//! `engine::transcribe` — async orchestrator; тяжёлая работа (чтение WAV,
//! создание recognizer, decode) унесена в один `tokio::task::spawn_blocking`.

pub mod cmd_fallback;
pub mod engine;
pub mod grouping;
pub mod segmentation;

pub use engine::transcribe;

use serde::{Deserialize, Serialize};

/// Таймстампнутый фрагмент распознанной речи.
#[derive(Debug, Clone)]
pub struct TimedSegment {
    pub start_sec: f32,
    pub end_sec: f32,
    pub text: String,
}

/// Результат ASR: полный текст + сегменты с таймкодами (для SRT).
#[derive(Debug, Clone)]
pub struct Transcription {
    pub text: String,
    pub segments: Vec<TimedSegment>,
}

/// Per-stage timings для завершённой задачи. Пишется в `<jobdir>/bench.json`
/// и используется для форматирования `progress.label` после done.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageTimings {
    /// yt-dlp --dump-json (только для URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_sec: Option<f32>,
    /// yt-dlp скачивание (только для URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_sec: Option<f32>,
    /// ffmpeg → 16 kHz mono WAV.
    pub extract_sec: f32,
    /// sherpa-onnx ASR decode.
    pub transcribe_sec: f32,
    /// Полное время от enqueue до done.
    pub total_sec: f32,
    /// ASR Real-Time Factor: `transcribe_sec / audio_duration_sec`.
    /// < 1.0 = быстрее realtime, > 1.0 = медленнее.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asr_rtf: Option<f32>,
    /// ASR throughput в samples/sec (Capacity-planning метрика).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asr_throughput: Option<f32>,
    /// Audio extract: MB входного файла / extract_sec.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_mb_per_sec: Option<f32>,
}

impl StageTimings {
    /// "Готово · 2:30 (скачивание 1:10, аудио 0:08, ASR 1:12, RTF 0.42x)".
    /// Локальные файлы (без metadata/download) — короче.
    pub fn summary_ru(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(t) = self.metadata_sec {
            parts.push(format!("метаданные {}", fmt_mmss(t)));
        }
        if let Some(t) = self.download_sec {
            parts.push(format!("скачивание {}", fmt_mmss(t)));
        }
        let mut audio = format!("аудио {}", fmt_mmss(self.extract_sec));
        if let Some(mbps) = self.extract_mb_per_sec {
            audio.push_str(&format!(" · {:.1} MB/s", mbps));
        }
        parts.push(audio);
        let mut asr = format!("ASR {}", fmt_mmss(self.transcribe_sec));
        if let Some(rtf) = self.asr_rtf {
            asr.push_str(&format!(" · RTF {:.2}x", rtf));
        }
        parts.push(asr);
        format!(
            "Готово · {} ({})",
            fmt_mmss(self.total_sec),
            parts.join(", ")
        )
    }
}

pub(crate) fn fmt_mmss(sec: f32) -> String {
    if sec < 60.0 {
        format!("{:.0}с", sec)
    } else {
        let m = (sec / 60.0) as u32;
        let s = (sec - m as f32 * 60.0) as u32;
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_mmss_under_minute() {
        assert_eq!(fmt_mmss(0.4), "0с");
        assert_eq!(fmt_mmss(8.1), "8с");
        assert_eq!(fmt_mmss(59.6), "60с"); // округление вверх — ок для UI
    }

    #[test]
    fn fmt_mmss_over_minute() {
        assert_eq!(fmt_mmss(70.3), "1:10");
        assert_eq!(fmt_mmss(125.0), "2:05");
    }

    #[test]
    fn summary_local_file() {
        let t = StageTimings {
            extract_sec: 8.0,
            transcribe_sec: 72.0,
            total_sec: 92.0,
            ..Default::default()
        };
        let s = t.summary_ru();
        assert!(s.contains("Готово"));
        assert!(s.contains("1:32"));
        assert!(s.contains("аудио 8с"));
        assert!(s.contains("ASR 1:12"));
        assert!(
            !s.contains("скачивание"),
            "local file should not mention download"
        );
    }

    #[test]
    fn summary_url_full() {
        let t = StageTimings {
            metadata_sec: Some(1.0),
            download_sec: Some(70.0),
            extract_sec: 8.0,
            transcribe_sec: 72.0,
            total_sec: 152.0,
        };
        let s = t.summary_ru();
        assert!(s.contains("метаданные"));
        assert!(s.contains("скачивание 1:10"));
    }
}
