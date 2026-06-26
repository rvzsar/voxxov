//! Auto-download моделей GigaAM-V3 e2e_rnnt с GitHub Releases.
//!
//! При первом запуске (если `model_path` пустой) — качаем 4 файла
//! (`gigaam_v3_e2e_rnnt_{encoder_int8,decoder,joint}.onnx` + `tokens.txt`)
//! + `silero_vad.onnx` в `<exe_dir>/models/`. Если все файлы уже есть — skip.
//!
//! e2e_rnnt голова: пунктуация и регистр встроены в модель.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

/// Тег Release'а с моделями GigaAM-V3 в репо `amidexe/govorun-lite`.
/// При обновлении моделей — bump здесь и перевыпустить Release.
pub const MODEL_RELEASE_TAG: &str = "model-gigaam-v3";

/// URL до (но не включая) release tag.
const MODEL_RELEASE_BASE_URL: &str = "https://github.com/amidexe/govorun-lite/releases/download";

/// File name as it appears in the release → where it's stored locally.
/// e2e_rnnt голова: 1025-token BPE vocab, пунктуация встроенная.
const MODEL_FILES: &[&str] = &[
    "gigaam_v3_e2e_rnnt_encoder_int8.onnx",
    "gigaam_v3_e2e_rnnt_decoder.onnx",
    "gigaam_v3_e2e_rnnt_joint.onnx",
    "gigaam_v3_e2e_rnnt_tokens.txt",
    "silero_vad.onnx",
];

/// Sanity-check: файл меньше — считаем битым и перекачиваем.
const MIN_FILE_SIZES: &[(&str, u64)] = &[
    ("gigaam_v3_e2e_rnnt_encoder_int8.onnx", 100_000_000), // ~319 MB
    ("gigaam_v3_e2e_rnnt_decoder.onnx", 1_000_000),        // ~4.6 MB
    ("gigaam_v3_e2e_rnnt_joint.onnx", 500_000),            // ~2.7 MB
    ("gigaam_v3_e2e_rnnt_tokens.txt", 1_000),              // ~13 KB
    ("silero_vad.onnx", 500_000),                          // ~629 KB
];

/// SHA256 (hex) для каждого файла. Пустая строка = пропустить проверку.
const MODELS_SHA256: &[(&str, &str)] = &[
    (
        "gigaam_v3_e2e_rnnt_encoder_int8.onnx",
        "2cac62d0c270bd128f898f2be1a2d34780d524a6e9483888ebac7b00f97410f1",
    ),
    (
        "gigaam_v3_e2e_rnnt_decoder.onnx",
        "781971998e6a355d6a714f6932a30eab295e7ba0d14fd7e0f78c83b87e811860",
    ),
    (
        "gigaam_v3_e2e_rnnt_joint.onnx",
        "602ff7017a93311aad34df1437c8d7f49911353c13d6eae7a6ee7b041339465c",
    ),
    (
        "gigaam_v3_e2e_rnnt_tokens.txt",
        "7ddf22514c42c531358182c81446a8159771e9921019f09ae743ea622d40221d",
    ),
];

/// Результат проверки модели.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// Директория где модели должны лежать.
    pub model_dir: PathBuf,
    /// Найденные файлы (имя → размер в байтах).
    pub present: Vec<ModelFile>,
    /// Отсутствующие или битые.
    pub missing: Vec<ModelFile>,
    /// Суммарный размер скачанного.
    pub total_bytes: u64,
    /// URL, откуда качаем.
    pub release_url: String,
    /// Тег Release'а.
    pub release_tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub name: String,
    pub size: u64,
}

/// Проверить статус модели. Не качает — только файловая система.
/// Size-only check (без SHA256): ~0.1ms на вызов. Полная SHA256-проверка
/// делается в `download_all` сразу после скачивания — там она нужна для
/// целостности скачанного файла. Здесь верифицировать диск не нужно: после
/// успешного download файлы считаются валидными, и любая последующая
/// порча — это уже не наш сценарий.
pub fn check_status(model_dir: &Path) -> ModelStatus {
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut total = 0u64;
    for name in MODEL_FILES {
        let path = model_dir.join(name);
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let min_size = MIN_FILE_SIZES
            .iter()
            .find(|(n, _)| *n == *name)
            .map(|(_, s)| *s)
            .unwrap_or(0);
        if size >= min_size {
            present.push(ModelFile {
                name: name.to_string(),
                size,
            });
            total += size;
        } else {
            missing.push(ModelFile {
                name: name.to_string(),
                size: 0,
            });
        }
    }
    ModelStatus {
        model_dir: model_dir.to_path_buf(),
        present,
        missing,
        total_bytes: total,
        release_url: MODEL_RELEASE_BASE_URL.to_string(),
        release_tag: MODEL_RELEASE_TAG.to_string(),
    }
}

/// Скачать все отсутствующие файлы. `progress` вызывается после
/// каждого скачанного файла: (downloaded, total).
pub async fn download_all<F>(model_dir: &Path, mut progress: F) -> AppResult<ModelStatus>
where
    F: FnMut(u64, u64) + Send,
{
    std::fs::create_dir_all(model_dir)
        .map_err(|e| AppError::Other(format!("create model dir {}: {e}", model_dir.display())))?;

    let status = check_status(model_dir);
    let total_bytes: u64 = status
        .missing
        .iter()
        .map(|m| {
            MIN_FILE_SIZES
                .iter()
                .find(|(n, _)| *n == m.name)
                .map(|(_, s)| *s)
                .unwrap_or(0)
        })
        .sum();
    let mut downloaded: u64 = 0;

    for file in &status.missing {
        let url = format!(
            "{}/{}/{}",
            MODEL_RELEASE_BASE_URL, MODEL_RELEASE_TAG, file.name
        );
        let target = model_dir.join(&file.name);
        info!(
            "model: downloading {} ({} expected bytes) from {}",
            file.name,
            MIN_FILE_SIZES
                .iter()
                .find(|(n, _)| *n == file.name)
                .map(|(_, s)| *s)
                .unwrap_or(0),
            url
        );
        download_file(&url, &target).await?;
        let actual = std::fs::metadata(&target)
            .map_err(|e| AppError::Other(format!("stat {}: {e}", target.display())))?
            .len();
        let min = MIN_FILE_SIZES
            .iter()
            .find(|(n, _)| *n == file.name)
            .map(|(_, s)| *s)
            .unwrap_or(0);
        if actual < min {
            return Err(AppError::Other(format!(
                "downloaded file {} is {} bytes, expected ≥ {}",
                file.name, actual, min
            )));
        }
        let expected_sha = MODELS_SHA256
            .iter()
            .find(|(n, _)| *n == file.name)
            .map(|(_, h)| *h)
            .unwrap_or("");
        if !expected_sha.is_empty() {
            if let Err(e) = verify_sha256(&target, expected_sha) {
                // Hash mismatch — удаляем файл, иначе следующий run
                // примет его за валидный (size check не отличит).
                let _ = std::fs::remove_file(&target);
                return Err(e);
            }
        }
        downloaded += actual;
        progress(downloaded, total_bytes);
    }
    Ok(check_status(model_dir))
}

/// Скачать один файл через `reqwest`.
async fn download_file(url: &str, target: &Path) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60 * 30))
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Other(format!("http client: {e}")))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "GET {url}: HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("read body {url}: {e}")))?;
    // Пишем через tmp + rename для атомарности.
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| AppError::Other(format!("write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, target)
        .map_err(|e| AppError::Other(format!("rename {}: {e}", target.display())))?;
    Ok(())
}

/// Дефолтная директория для моделей: `<data_root>/models` (рядом с .exe).
pub fn default_model_dir() -> PathBuf {
    crate::paths::model_dir()
}

/// Streaming SHA256, чтобы не грузить encoder (~319 MB) в память.
fn verify_sha256(target: &Path, expected_hex: &str) -> AppResult<()> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(target)
        .map_err(|e| AppError::Other(format!("open {}: {e}", target.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| AppError::Other(format!("read {}: {e}", target.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let actual_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if !actual_hex.eq_ignore_ascii_case(expected_hex) {
        return Err(AppError::Other(format!(
            "sha256 mismatch for {}: expected {}, got {}",
            target.display(),
            expected_hex,
            actual_hex
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir_like() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "gigaam-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn status_reports_missing_files() {
        let tmp = tempdir_like();
        let s = check_status(&tmp);
        assert_eq!(s.present.len(), 0);
        assert_eq!(s.missing.len(), 4);
        assert_eq!(s.total_bytes, 0);
        assert!(s.release_url.contains("github.com"));
        assert!(!s.release_tag.is_empty());
    }

    #[test]
    fn status_skips_partial_files() {
        let tmp = tempdir_like();
        // 1 KB encoder — ниже порога, уходит в missing.
        std::fs::write(tmp.join("v3_rnnt_encoder_int8.onnx"), vec![0u8; 1024]).unwrap();
        let s = check_status(&tmp);
        assert_eq!(s.missing.len(), 4);
    }

    #[test]
    fn status_ignores_unknown_files() {
        let tmp = tempdir_like();
        std::fs::write(tmp.join("extra.txt"), b"junk").unwrap();
        let s = check_status(&tmp);
        assert_eq!(s.present.len(), 0);
        assert_eq!(s.missing.len(), 4);
    }

    #[test]
    fn sha256_matches_known_string() {
        // SHA256("hello")
        const EXPECTED: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let tmp = tempdir_like();
        let f = tmp.join("h.bin");
        std::fs::write(&f, b"hello").unwrap();
        verify_sha256(&f, EXPECTED).unwrap();
    }

    #[test]
    fn sha256_rejects_mismatch() {
        let tmp = tempdir_like();
        let f = tmp.join("h.bin");
        std::fs::write(&f, b"hello").unwrap();
        let wrong = "0".repeat(64);
        assert!(verify_sha256(&f, &wrong).is_err());
    }

    #[test]
    fn sha256_skips_empty_expected() {
        // Пустой expected_hex — opt-out из проверки (для override'а без хеша).
        let tmp = tempdir_like();
        let f = tmp.join("h.bin");
        std::fs::write(&f, b"hello").unwrap();
        verify_sha256(&f, "").unwrap();
    }
}
