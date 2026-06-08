//! Глобальное состояние приложения.
//!
//! Хранит карту задач, токены отмены, конфиг, broadcast-канал событий и
//! лениво-инициализированный `yt-dlp` downloader.

use crate::config::AppConfig;
use crate::types::{BackendEvent, Job, JobId, JobStage, JobUpdate, Progress};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

const CHANNEL_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    pub cancel_tokens: Arc<RwLock<HashMap<JobId, CancellationToken>>>,
    pub config: Arc<RwLock<AppConfig>>,
    pub events: broadcast::Sender<BackendEvent>,
    /// Lazy-initialized `yt-dlp` downloader. Result-обёртка позволяет
    /// пробрасывать init-ошибки вызывающим (OnceCell.set не умеет Result).
    pub downloader: Arc<tokio::sync::OnceCell<Result<Arc<yt_dlp::Downloader>, String>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            events: tx,
            downloader: Arc::new(tokio::sync::OnceCell::new()),
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
        self.jobs.write().insert(id.clone(), job.clone());
        let _ = self
            .events
            .send(BackendEvent::JobCreated { job: Box::new(job) });
    }

    /// Применить `update` и разослать событие. Нет задачи — warning.
    pub fn update_job(&self, id: &str, update: JobUpdate) {
        let mut jobs = self.jobs.write();
        if let Some(j) = jobs.get_mut(id) {
            apply_update(j, &update);
            let _ = self
                .events
                .send(BackendEvent::JobUpdated { id: id.to_string(), update });
        } else {
            drop(jobs);
            tracing::warn!("update_job: job {id} not found");
        }
    }

    /// Установить стадию + label. Сохраняет `progress.pct > 0`,
    /// чтобы прогресс-бар не отскакивал назад при смене стадии.
    pub fn set_stage(&self, id: &str, stage: JobStage, label: impl Into<String>) {
        let label = label.into();
        let current_pct = self
            .jobs
            .read()
            .get(id)
            .map(|j| j.progress.pct)
            .unwrap_or(0.0);
        let pct = match stage {
            JobStage::Done | JobStage::Failed | JobStage::Cancelled => 1.0,
            _ if current_pct > 0.0 => current_pct,
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

    // ---------------- config ----------------

    pub fn config(&self) -> AppConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, cfg: AppConfig) {
        *self.config.write() = cfg;
    }
}

fn apply_update(j: &mut Job, u: &JobUpdate) {
    if let Some(s) = u.stage {
        j.stage = s;
    }
    if let Some(progress) = &u.progress {
        // Игнорировать откат pct, если он уже > 0.
        if !(progress.pct < j.progress.pct && j.progress.pct > 0.0) {
            j.progress = progress.clone();
        }
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
