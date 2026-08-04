//! Глобальное состояние приложения.
//!
//! Хранит карту задач, токены отмены, конфиг, broadcast-канал событий и
//! лениво-инициализированный `yt-dlp` downloader.

use crate::config::AppConfig;
use crate::types::{BackendEvent, Job, JobId, JobStage, JobUpdate, Progress};
use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Semaphore};
use tokio_util::sync::CancellationToken;

const CHANNEL_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    pub cancel_tokens: Arc<RwLock<HashMap<JobId, CancellationToken>>>,
    pub config: Arc<RwLock<AppConfig>>,
    pub events: broadcast::Sender<BackendEvent>,
    /// Глобальный permit-pool для ASR-стадии. При приоритете CPU — только
    /// один ASR-декод за раз, иначе over-subscription и cache thrashing.
    pub asr_permits: Arc<Semaphore>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        let jobs = Self::load_jobs().unwrap_or_default();
        Self {
            jobs: Arc::new(RwLock::new(jobs)),
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            events: tx,
            asr_permits: Arc::new(Semaphore::new(1)),
        }
    }

    // ---------------- jobs ----------------

    pub fn register_token(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancel_tokens.write().insert(id.to_string(), token.clone());
        token
    }

    pub fn insert_job(&self, job: Job) {
        let id = job.id.clone();
        self.jobs.write().insert(id, job.clone());
        let _ = self
            .events
            .send(BackendEvent::JobCreated { job: Box::new(job) });
        self.save_jobs();
    }

    /// Применить `update` и разослать событие. Нет задачи — warning.
    /// Сохраняет в `data/jobs.json` только при переходе в терминальное
    /// состояние (Done/Failed/Cancelled) — иначе слишком много I/O на
    /// progress-апдейтах.
    pub fn update_job(&self, id: &str, update: JobUpdate) {
        let needs_save;
        {
            let mut jobs = self.jobs.write();
            if let Some(j) = jobs.get_mut(id) {
                let was_terminal = is_terminal(j.stage);
                apply_update(j, &update);
                let now_terminal = is_terminal(j.stage);
                let _ = self
                    .events
                    .send(BackendEvent::JobUpdated { id: id.to_string(), update });
                needs_save = !was_terminal && now_terminal;
            } else {
                tracing::warn!("update_job: job {id} not found");
                return;
            }
        }
        if needs_save {
            self.save_jobs();
        }
    }

    /// Установить стадию + label. **Сбрасывает** `progress.pct` в 0 при
    /// старте новой нетерминальной стадии, чтобы per-stage прогресс
    /// (download %, ffmpeg `time=` , ASR сегмент) был виден с нуля,
    /// а не упирался в "never go back" guard из предыдущей стадии.
    pub fn set_stage(&self, id: &str, stage: JobStage, label: impl Into<String>) {
        let label = label.into();
        let pct = match stage {
            JobStage::Done | JobStage::Failed | JobStage::Cancelled => 1.0,
            _ => 0.0,
        };
        self.update_job(
            id,
            JobUpdate {
                stage: Some(stage),
                progress: Some(Progress {
                    pct,
                    label,
                    speed: None,
                    eta: None,
                }),
                ..Default::default()
            },
        );
    }

    /// Отправить низкоуровневый лог в frontend (и в tracing).
    pub fn log_line(&self, id: &str, line: impl Into<String>) {
        let line = line.into();
        tracing::info!(job = %id, "{line}");
        let _ = self
            .events
            .send(BackendEvent::JobLog { id: id.to_string(), line });
    }

    pub fn get_job(&self, id: &str) -> Option<Job> {
        self.jobs.read().get(id).cloned()
    }

    pub fn list_jobs(&self) -> Vec<Job> {
        let mut v: Vec<Job> = self.jobs.read().values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    /// Запросить отмену. Возвращает `true`, если такой токен был.
    pub fn cancel(&self, id: &str) -> bool {
        let token = self.cancel_tokens.write().remove(id);
        if let Some(t) = token {
            t.cancel();
            self.update_job(
                id,
                JobUpdate {
                    stage: Some(JobStage::Cancelled),
                    finished_at: Some(chrono::Utc::now().to_rfc3339()),
                    error: Some("cancelled by user".into()),
                    ..Default::default()
                },
            );
            true
        } else {
            false
        }
    }

    /// Удалить все терминальные задачи (done/failed/cancelled) из памяти и
    /// history-файла. Активные (queued/transcribing/etc.) сохраняются.
    pub fn clear_terminal_jobs(&self) {
        self.jobs.write().retain(|_, j| !is_terminal(j.stage));
        self.save_jobs();
    }

    // ---------------- persistence ----------------

    /// Загрузить все jobs из `data/jobs.json`. Отсутствие файла = пустая карта.
    /// In-progress задачи, оставшиеся от предыдущего запуска (краш/закрытие),
    /// помечаются как `Failed` — иначе UI зависнет на "transcribing" навсегда.
    fn load_jobs() -> Option<HashMap<JobId, Job>> {
        let path = jobs_file_path();
        let bytes = std::fs::read(&path).ok()?;
        let mut jobs: Vec<Job> = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("jobs.json: parse failed, starting empty: {e}");
                return None;
            }
        };
        for j in &mut jobs {
            if !is_terminal(j.stage) {
                j.stage = JobStage::Failed;
                j.error = Some("interrupted by app restart".into());
                j.finished_at = Some(Utc::now().to_rfc3339());
            }
        }
        Some(jobs.into_iter().map(|j| (j.id.clone(), j)).collect())
    }

    /// Сохранить все jobs в `data/jobs.json` (atomic write через tmp+rename).
    /// Best-effort: ошибка I/O не должна ломать runtime.
    fn save_jobs(&self) {
        let path = jobs_file_path();
        let jobs: Vec<Job> = self.jobs.read().values().cloned().collect();
        let json = match serde_json::to_vec_pretty(&jobs) {
            Ok(j) => j,
            Err(e) => { tracing::warn!("jobs.json: serialize failed: {e}"); return; }
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_err() { return; }
        let _ = std::fs::rename(&tmp, &path);
    }

    // ---------------- config ----------------

    pub fn config(&self) -> AppConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, cfg: AppConfig) {
        *self.config.write() = cfg;
    }
}

fn is_terminal(stage: JobStage) -> bool {
    matches!(stage, JobStage::Done | JobStage::Failed | JobStage::Cancelled)
}

fn jobs_file_path() -> PathBuf {
    crate::paths::data_root().join("data").join("jobs.json")
}

fn apply_update(j: &mut Job, u: &JobUpdate) {
    if let Some(s) = u.stage {
        j.stage = s;
    }
    if let Some(progress) = &u.progress {
        // Прогресс применяется целиком. Старый guard «не откатывать pct»
        // блокировал весь update (включая speed/eta/label), когда pct шёл
        // назад — а он шёл при каждом новом потоке yt-dlp (bv*+ba качает
        // видео 0..100%, потом аудио с 0%) и при set_stage-сбросах.
        // Итог: бар застывал на «Загрузка 100%», скорость не обновлялась.
        // Теперь монотонность обеспечивает DownloadTracker (ytdlp.rs),
        // а set_stage сознательно сбрасывает pct на новой стадии.
        j.progress = progress.clone();
    }
    if let Some(v) = &u.finished_at {
        j.finished_at = Some(v.clone());
    }
    if let Some(m) = &u.media {
        j.media = Some(m.clone());
    }
    if let Some(v) = &u.transcript_path {
        j.transcript_path = Some(v.clone());
    }
    if let Some(v) = &u.transcript_preview {
        j.transcript_preview = Some(v.clone());
    }
    if let Some(v) = &u.error {
        j.error = Some(v.clone());
    }
}
