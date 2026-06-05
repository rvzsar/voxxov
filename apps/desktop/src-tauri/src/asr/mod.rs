//! ASR-движок. По умолчанию — Python-сабпроцесс, дёргающий `gigaam`
//! (https://github.com/salute-developers/GigaAM). Можно заменить на
//! собственный CLI-командой, задав `asr.model_path` равным
//! `<command> <args-prefix>` — см. комментарии в engine.rs.

pub mod engine;

pub use engine::transcribe;
