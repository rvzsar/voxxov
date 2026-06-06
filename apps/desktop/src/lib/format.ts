import type { JobStage } from './types';

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
