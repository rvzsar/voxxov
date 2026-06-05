// ===== Базовые типы, общие для фронта и бэка =====

export type JobId = string;
export type JobStage =
  | 'queued'
  | 'fetching_metadata'
  | 'downloading'
  | 'extracting_audio'
  | 'transcribing'
  | 'done'
  | 'failed'
  | 'cancelled';

export type JobProgress = {
  /** 0..1 */
  pct: number;
  /** Человекочитаемая подпись, например "Скачивание 12.3 MB / 48 MB" */
  label: string;
  /** Скорость скачивания (если применимо) */
  speed?: string;
  /** ETA (если применимо) */
  eta?: string;
};

export type MediaInfo = {
  id: string;
  title: string;
  uploader?: string;
  durationSec?: number;
  thumbnailUrl?: string;
  url: string;
};

export type Job = {
  id: JobId;
  url: string;
  stage: JobStage;
  progress: JobProgress;
  media?: MediaInfo;
  /** Путь к распознанному тексту (.txt / .srt) */
  transcriptPath?: string;
  /** Краткий превью текста (первые ~200 символов) */
  transcriptPreview?: string;
  error?: string;
  createdAt: string;
  finishedAt?: string;
};

export type ProxyKind = 'none' | 'http' | 'https' | 'socks5';

export type ProxyConfig = {
  kind: ProxyKind;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  /** Обход прокси для доменов (через запятую) */
  noProxy?: string;
};

export type DownloadConfig = {
  /** Формат yt-dlp merge_output, например "bv*+ba/b" */
  format: string;
  /** Максимальная высота (720, 1080) или null = без лимита */
  maxHeight?: number | null;
  /** Скачивать только аудио (без видео) */
  audioOnly: boolean;
  /** Ограничить размер файла, например "500M" */
  maxFilesize?: string;
  /** Доп. аргументы yt-dlp */
  extraArgs: string[];
  /** Cookies-файл (Netscape) */
  cookiesFile?: string;
};

export type AsrConfig = {
  /** Путь к папке с ONNX-моделями */
  modelDir: string;
  /** Размер пре-трансформера: nvidia/salute-ai/GigaAM-V3 — варианты RNN-T */
  modelVariant: 'v3_rnnt' | 'v3_ctc' | 'v3_e2e';
  /** Устройство инференса */
  device: 'cpu' | 'openvino_cpu' | 'openvino_gpu' | 'openvino_npu';
  /** Число потоков CPU для OpenVINO */
  threads: number;
  /** Длина чанка в секундах (для длинных аудио) */
  chunkLengthSec: number;
  /** Сколько оверлапа (в секундах) между чанками */
  chunkOverlapSec: number;
  /** Использовать VAD (Silero) для пропуска тишины */
  useVad: boolean;
};

export type OutputConfig = {
  /** Форматы вывода: txt / srt / json / vtt */
  formats: Array<'txt' | 'srt' | 'json' | 'vtt'>;
  /** Максимальная длина строки для txt */
  maxLineLength: number;
  /** Папка для сохранения результатов (пустая = <downloads>/transcripts) */
  outputDir?: string;
};

export type LoggingConfig = {
  level: 'error' | 'warn' | 'info' | 'debug' | 'trace';
  /** Хранить лог-файл */
  file: boolean;
  /** Максимальный размер лог-файла, MB */
  maxFileSizeMb: number;
};

export type AppConfig = {
  proxy: ProxyConfig;
  download: DownloadConfig;
  asr: AsrConfig;
  output: OutputConfig;
  logging: LoggingConfig;
  /** Тема UI */
  theme: 'auto' | 'dark' | 'light';
};

export const DEFAULT_CONFIG: AppConfig = {
  proxy: { kind: 'none' },
  download: {
    format: 'bv*+ba/b',
    maxHeight: 1080,
    audioOnly: false,
    maxFilesize: '2G',
    extraArgs: [],
  },
  asr: {
    modelDir: '',
    modelVariant: 'v3_rnnt',
    device: 'openvino_cpu',
    threads: 8,
    chunkLengthSec: 20,
    chunkOverlapSec: 2,
    useVad: true,
  },
  output: {
    formats: ['txt', 'srt'],
    maxLineLength: 90,
  },
  logging: {
    level: 'info',
    file: true,
    maxFileSizeMb: 20,
  },
  theme: 'auto',
};

// ===== События от бэка (Tauri events) =====

export type BackendEvent =
  | { kind: 'job:created'; job: Job }
  | { kind: 'job:updated'; id: JobId; patch: Partial<Job> }
  | { kind: 'job:log'; id: JobId; line: string }
  | { kind: 'job:done'; id: JobId; transcriptPath: string; preview: string }
  | { kind: 'job:failed'; id: JobId; error: string }
  | { kind: 'download:progress'; id: JobId; pct: number; label: string; speed?: string; eta?: string };
