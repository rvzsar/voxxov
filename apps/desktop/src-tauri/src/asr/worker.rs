//! Синхронный ASR-воркер, владеет `OfflineRecognizer`.
//!
//! Используется через `tokio::task::spawn_blocking` из async-orchestrator'а
//! в `engine.rs`. Один recognizer живёт в воркере; каждый сегмент аудио
//! декодируется отдельным `OfflineStream`.

use crate::error::{AppError, AppResult};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};
use std::path::Path;

/// Сырой результат декодирования одного чанка: текст, токены, опциональные
/// per-token таймстампы и длительности (в секундах, **внутри** чанка).
pub struct DecodedChunk {
    pub text: String,
    pub tokens: Vec<String>,
    pub timestamps: Option<Vec<f32>>,
    pub durations: Option<Vec<f32>>,
}

/// Готовый к декодированию recognizer. Не Clone — живёт в одном воркере.
pub struct AsrEngine {
    recognizer: OfflineRecognizer,
}

impl AsrEngine {
    /// Сконструировать recognizer из путей к моделям + provider.
    pub fn new(
        encoder: &Path,
        decoder: &Path,
        joiner: &Path,
        tokens: &Path,
        num_threads: usize,
        provider: &str,
    ) -> AppResult<Self> {
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(encoder.to_string_lossy().into_owned()),
            decoder: Some(decoder.to_string_lossy().into_owned()),
            joiner: Some(joiner.to_string_lossy().into_owned()),
        };
        config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
        config.model_config.num_threads = num_threads as i32;
        config.model_config.provider = Some(provider.to_string());
        config.model_config.debug = false;

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            AppError::Asr(
                "failed to create OfflineRecognizer — check model paths and provider".into(),
            )
        })?;
        Ok(Self { recognizer })
    }

    /// Декодировать один моно-чанк (f32) на указанной частоте дискретизации.
    /// Синхронный вызов — должен вызываться из `spawn_blocking`.
    pub fn decode(&self, samples: &[f32], sample_rate: u32) -> AppResult<DecodedChunk> {
        let stream = self.recognizer.create_stream();
        // sherpa-onnx C API принимает sample_rate как i32
        stream.accept_waveform(sample_rate as i32, samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| AppError::Asr("decode returned no result".into()))?;
        Ok(DecodedChunk {
            text: result.text.trim().to_string(),
            tokens: result.tokens,
            timestamps: result.timestamps,
            durations: result.durations,
        })
    }
}
