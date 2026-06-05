//! Глобальное состояние приложения.
//!
//! Хранит карту задач, токены отмены, конфиг и broadcast-канал событий.
//! Доступ из sync и async кода — через `parking_lot::RwLock`.

use crate::config::AppConfig;
use crate::types::{BackendEvent, Job, JobId, JobPatch, JobStage};
use parking_lot::RwLock;
use std::collections::HashMap;
use tauri::AppHandle;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

const CHANNEL_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct AppState {
    pub jobs: RwLock<HashMap<JobId, Job>>,
    pub cancel_tokens: RwLock<HashMap<JobId, CancellationToken>>,
    pub config: RwLock<AppConfig>,
    pub events: broadcast::Sender<BackendEvent>,
    pub app: AppHandle,
}

impl AppState {
    pub fn new(config: AppConfig, app: AppHandle) -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            jobs: RwLock::new(HashMap::new()),
            cancel_tokens: RwLock::new(HashMap::new()),
            config: RwLock::new(config),
            events: tx,
            app,
        }
    }

    pub fn app_handle(&self) -> AppHandle { self.app.clone() }

    // ---------------- jobs ----------------

    pub fn register_token(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancel_tokens.write().insert(id.to_string(), token.clone());
        token
    }

    pub fn insert_job(&self, job: Job) {
        let id = job.id.clone();
        self.jobs.write().insert(id.clone(), job.clone());
        let _ = self.events.send(BackendEvent::JobCreated { job: Box::new(job) });
    }

    /// Применить патч к существующей задаче и разослать событие.
    pub fn patch_job(&self, id: &str, patch: JobPatch) {
        let mut jobs = self.jobs.write();
        if let Some(j) = jobs.get_mut(id) {
            apply_patch(j, &patch);
            let _ = self.events.send(BackendEvent::JobUpdated {
                id: id.to_string(),
                patch,
            });
        } else {
            drop(jobs);
            tracing::warn!("patch_job: job {id} not found");
        }
    }

    /// Установить стадию и (опционально) текстовый label прогресса.
    pub fn set_stage(&self, id: &str, stage: JobStage, label: impl Into<String>) {
        self.patch_job(
            id,
            JobPatch {
                stage: Some(stage),
                progress: Some(crate::types::Progress {
                    pct: match stage {
                        JobStage::Done | JobStage::Failed | JobStage::Cancelled => 100.0,
                        _ => 0.0,
                    },
                    label: label.into(),
                    speed: None,
                    eta: None,
                }),
                ..Default::default()
            },
        );
    }

    /// Отправить низкоуровневый лог в frontend (и залогировать в tracing).
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
            self.set_stage(id, JobStage::Cancelled, "Отменено");
            true
        } else {
            false
        }
    }

    pub fn remove_token(&self, id: &str) {
        self.cancel_tokens.write().remove(id);
    }

    // ---------------- config ----------------

    pub fn config(&self) -> AppConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, cfg: AppConfig) {
        *self.config.write() = cfg;
    }
}

fn apply_patch(j: &mut Job, p: &JobPatch) {
    if let Some(s) = p.stage {
        j.stage = s;
    }
    if let Some(progress) = &p.progress {
        j.progress = progress.clone();
    }
    if let Some(v) = &p.finished_at {
        j.finished_at = Some(v.clone());
    }
    if let Some(m) = &p.media {
        j.media = Some(m.clone());
    }
    if let Some(v) = &p.transcript_path {
        j.transcript_path = Some(v.clone());
    }
    if let Some(v) = &p.transcript_preview {
        j.transcript_preview = Some(v.clone());
    }
    if let Some(v) = &p.error {
        j.error = Some(v.clone());
    }
}
