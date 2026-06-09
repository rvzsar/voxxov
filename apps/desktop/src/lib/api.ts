// ===== Tauri-обёртки =====

import type {
  AppConfig,
  BackendEvent,
  FileInfo,
  Job,
  JobId,
  MediaInfo,
} from "./types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

async function tInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(cmd, args);
}

async function tListen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  const un = await listen<T>(event, (e: { payload: T }) => handler(e.payload));
  return un;
}

// ===== Публичный API =====

export const api = {
  async enqueueUrl(url: string): Promise<JobId> {
    return tInvoke<JobId>("enqueue_url", { url });
  },

  async fetchMetadata(url: string): Promise<MediaInfo> {
    return tInvoke<MediaInfo>("fetch_metadata", { url });
  },

  async listJobs(): Promise<Job[]> {
    return tInvoke<Job[]>("list_jobs");
  },

  async scanFolder(path: string): Promise<FileInfo[]> {
    return tInvoke<FileInfo[]>("scan_folder", { path });
  },

  async enqueueLocal(path: string): Promise<JobId> {
    return tInvoke<JobId>("enqueue_local", { path });
  },

  async cancelJob(id: JobId): Promise<void> {
    return tInvoke("cancel_job", { id });
  },

  async getConfig(): Promise<AppConfig> {
    return tInvoke<AppConfig>("get_config");
  },

  async saveConfig(cfg: AppConfig): Promise<void> {
    return tInvoke("save_config", { cfg });
  },

  async revealInFolder(path: string): Promise<void> {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    return revealItemInDir(path);
  },

  /** Абсолютный путь к папке задачи (`<data_root>/data/jobs/<id>/`). */
  async getJobWorkdir(jobId: JobId): Promise<string> {
    return tInvoke<string>("get_job_workdir", { jobId });
  },

  /**
   * Скопировать всю папку задачи (видео + аудио + txt/srt/json) в
   * `<destDir>/<jobId>/`. Возвращает путь к созданной папке.
   */
  async saveJob(jobId: JobId, destDir: string): Promise<string> {
    return tInvoke<string>("save_job", { jobId, destDir });
  },

  /**
   * Нативный диалог выбора папки. Возвращает путь или null.
   * `title` — заголовок окна.
   */
  async pickFolder(title: string): Promise<string | null> {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const result = await open({ directory: true, multiple: false, title });
    return typeof result === "string" ? result : null;
  },

  /** Подписка на единый поток BackendEvent от бэка */
  onJobEvent(handler: (e: BackendEvent) => void): () => void {
    let unsub: (() => void) | null = null;
    let cancelled = false;
    tListen<BackendEvent>("job:event", handler).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  },

  async diagnose(): Promise<{
    ytdlp?: string;
    ffmpeg?: string;
    appData: string;
    appVersion: string;
  }> {
    return tInvoke("diagnose");
  },
};
