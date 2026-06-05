// ===== Tauri-обёртки. В мок-режиме (dev без Rust) возвращают фейки =====

import type { AppConfig, BackendEvent, Job, JobId, MediaInfo } from './types';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function tInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) throw new Error(`Tauri backend недоступен (команда: ${cmd})`);
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

async function tListen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  const un = await listen<T>(event, (e) => handler(e.payload));
  return un;
}

// ===== Mocks (только для UI-разработки без Rust) =====

const MOCK_JOBS: Job[] = [];

function uid(): string {
  return Math.random().toString(36).slice(2, 10);
}

function delay(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

async function mockRunJob(url: string): Promise<JobId> {
  const id = uid();
  const job: Job = {
    id,
    url,
    stage: 'queued',
    progress: { pct: 0, label: 'В очереди…' },
    createdAt: new Date().toISOString(),
  };
  MOCK_JOBS.unshift(job);
  (async () => {
    const stages: Array<{ stage: Job['stage']; label: string; to: number }> = [
      { stage: 'fetching_metadata', label: 'Получаю метаданные…', to: 0.05 },
      { stage: 'downloading', label: 'Скачиваю видео (мок)', to: 0.6 },
      { stage: 'extracting_audio', label: 'Извлекаю аудио (мок)', to: 0.75 },
      { stage: 'transcribing', label: 'Распознаю речь (мок)', to: 0.99 },
    ];
    for (const s of stages) {
      job.stage = s.stage;
      for (let p = job.progress.pct; p < s.to; p += 0.02) {
        job.progress = { pct: p, label: s.label };
        await delay(120);
      }
    }
    job.stage = 'done';
    job.progress = { pct: 1, label: 'Готово' };
    job.transcriptPreview = 'Это мок-результат. Реальный инференс появится в Sprint 3.';
    job.finishedAt = new Date().toISOString();
  })();
  return id;
}

// ===== Публичный API =====

export const api = {
  isTauri,

  async enqueueUrl(url: string): Promise<JobId> {
    if (!isTauri) return mockRunJob(url);
    return tInvoke<JobId>('enqueue_url', { url });
  },

  async fetchMetadata(url: string): Promise<MediaInfo> {
    if (!isTauri) {
      await delay(400);
      return {
        id: uid(),
        url,
        title: 'Мок: ' + url,
        uploader: 'mock',
        durationSec: 123,
      };
    }
    return tInvoke<MediaInfo>('fetch_metadata', { url });
  },

  async listJobs(): Promise<Job[]> {
    if (!isTauri) return [...MOCK_JOBS];
    return tInvoke<Job[]>('list_jobs');
  },

  async cancelJob(id: JobId): Promise<void> {
    if (!isTauri) {
      const i = MOCK_JOBS.findIndex((j) => j.id === id);
      if (i >= 0) MOCK_JOBS.splice(i, 1);
      return;
    }
    return tInvoke('cancel_job', { id });
  },

  async getConfig(): Promise<AppConfig> {
    if (!isTauri) {
      const stored = localStorage.getItem('gigaam:config');
      if (stored) return JSON.parse(stored);
      const { DEFAULT_CONFIG } = await import('./types');
      return DEFAULT_CONFIG;
    }
    return tInvoke<AppConfig>('get_config');
  },

  async saveConfig(cfg: AppConfig): Promise<void> {
    if (!isTauri) {
      localStorage.setItem('gigaam:config', JSON.stringify(cfg));
      return;
    }
    return tInvoke('save_config', { config: cfg });
  },

  async revealInFolder(path: string): Promise<void> {
    if (!isTauri) {
      console.warn('[mock] revealInFolder:', path);
      return;
    }
    const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
    return revealItemInDir(path);
  },

  /** Подписка на единый поток BackendEvent от бэка */
  onJobEvent(handler: (e: BackendEvent) => void): () => void {
    if (!isTauri) return () => {};
    let unsub: (() => void) | null = null;
    let cancelled = false;
    tListen<BackendEvent>('job:event', handler).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  },

  onDownloadProgress(
    handler: (p: { id: JobId; pct: number; label: string; speed?: string; eta?: string }) => void,
  ): () => void {
    if (!isTauri) return () => {};
    let unsub: (() => void) | null = null;
    let cancelled = false;
    tListen('download:progress', handler).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  },

  async diagnose(): Promise<{ ytdlp?: string; ffmpeg?: string; tauri: boolean }> {
    if (!isTauri) return { tauri: false };
    return tInvoke('diagnose');
  },
};
