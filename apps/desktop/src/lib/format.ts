import type { Job, JobStage, JobSource } from './types';

const STAGE_LABELS: Record<JobStage, string> = {
  queued: 'В очереди',
  fetching_metadata: 'Метаданные',
  downloading: 'Загрузка',
  extracting_audio: 'Аудио',
  transcribing: 'Распознавание',
  done: 'Готово',
  failed: 'Ошибка',
  cancelled: 'Отменено',
};

export function stageLabel(stage: JobStage): string {
  return STAGE_LABELS[stage] ?? stage;
}

/**
 * Диапазон общего прогресса задачи для стадии. Стадии весят по-разному:
 * скачивание — основная часть для URL, ASR — для локальных файлов.
 * Это позволяет показывать единый непрерывный бар 0..100% вместо того,
 * чтобы каждая стадия обнулялась и выглядела «зависшей».
 */
export function stageRange(stage: JobStage, source: JobSource): [number, number] {
  if (source === 'local_file') {
    switch (stage) {
      case 'extracting_audio': return [0, 0.3];
      case 'transcribing': return [0.3, 1];
      default: return [0, 0];
    }
  }
  switch (stage) {
    case 'fetching_metadata': return [0, 0.05];
    case 'downloading': return [0.05, 0.7];
    case 'extracting_audio': return [0.7, 0.75];
    case 'transcribing': return [0.75, 1];
    default: return [0, 0];
  }
}

/** Общий прогресс задачи 0..1: стадия + прогресс внутри неё. */
export function overallPct(job: Job): number {
  if (job.stage === 'done') return 1;
  if (job.stage === 'failed' || job.stage === 'cancelled') {
    return Math.min(1, job.progress.pct || 0);
  }
  const [a, b] = stageRange(job.stage, job.source);
  const inner = Math.min(1, Math.max(0, job.progress.pct || 0));
  return Math.min(1, a + (b - a) * inner);
}

export function isProbablyUrl(s: string): boolean {
  return /^https?:\/\/\S+$/i.test(s.trim());
}

export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function fmtDuration(sec: number): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  return `${m}:${String(s).padStart(2, '0')}`;
}

/** Русская плюрализация: 1 задача, 2 задачи, 5 задач. */
export function fmtJobs(n: number): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return `${n} задача`;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return `${n} задачи`;
  return `${n} задач`;
}
