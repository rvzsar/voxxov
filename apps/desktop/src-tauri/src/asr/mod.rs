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
pub mod segment;
pub mod worker;

pub use engine::transcribe;
