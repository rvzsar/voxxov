//! ASR-движок. Sherpa-onnx (static C library, native Rust bindings).
//!
//! Если `model_path` начинается с `cmd:` — запускается внешний CLI.
//! Иначе загружается sherpa-onnx модель напрямую.
//!
//! `engine::transcribe` — async orchestrator; тяжёлая работа унесена в
//! `worker` + `tokio::task::spawn_blocking`, длинное аудио режется
//! сегментами в `segment`.

pub mod cmd_fallback;
pub mod engine;
pub mod grouping;
pub mod segment;
pub mod worker;

pub use engine::transcribe;

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
