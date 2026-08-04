// ===== Базовые типы, общие для фронта и бэка =====
// Имена полей — camelCase, соответствуют serde(rename_all) в Rust.

export type JobId = string;
export type JobStage =
  | "queued"
  | "fetching_metadata"
  | "downloading"
  | "extracting_audio"
  | "transcribing"
  | "done"
  | "failed"
  | "cancelled";

export type JobSource = "url" | "local_file";

export type JobProgress = {
  pct: number;
  label: string;
  speed?: string;
  eta?: string;
};

export type MediaInfo = {
  id: string;
  url: string;
  title: string;
  uploader?: string;
  durationSec?: number;
  thumbnail?: string;
};

export type Job = {
  id: JobId;
  url: string;
  source: JobSource;
  stage: JobStage;
  progress: JobProgress;
  createdAt: string;
  finishedAt?: string;
  media?: MediaInfo;
  transcriptPath?: string;
  transcriptPreview?: string;
  error?: string;
};

// ===== Конфиг — mirror of Rust config.rs =====

export type ProxyKind = "none" | "http" | "https" | "socks5";

export type ProxyConfig = {
  kind: ProxyKind;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  noProxy?: string;
};

export type DownloadConfig = {
  format: string;
  maxHeight: number;
  audioOnly: boolean;
  embedSubs: boolean;
  concurrentFragments: number;
  retries: number;
  overwrite: boolean;
  cookieFile?: string;
  userAgent?: string;
};

export type AsrDevice = "cpu" | "cuda" | "directml" | "openvino";

export type AsrConfig = {
  /** Folder containing the 4 GigaAM files (encoder/decoder/joiner.onnx
   *  + tokens.txt). Empty = auto-download to <exe_dir>/models.
   *  "cmd:some-cli --args" = delegate to an external CLI. */
  modelDir: string;
  sampleRate: number;
  language: string;
  device: AsrDevice;
  beamSize: number;
};

export type OutputConfig = {
  /** Куда дополнительно копировать транскрипты; пусто = только workdir. */
  dir: string;
  formats: string[];
};

export type LoggingConfig = {
  level: string;
  file: boolean;
  maxSizeMb: number;
  keepFiles: number;
};

export type FileInfo = {
  path: string;
  name: string;
  extension: string;
  sizeBytes: number;
};

export type AppConfig = {
  proxy: ProxyConfig;
  download: DownloadConfig;
  asr: AsrConfig;
  output: OutputConfig;
  logging: LoggingConfig;
};

export const DEFAULT_CONFIG: AppConfig = {
  proxy: { kind: "none" },
  download: {
    format: "bv*+ba/b",
    maxHeight: 1080,
    audioOnly: false,
    embedSubs: false,
    concurrentFragments: 4,
    retries: 3,
    overwrite: false,
  },
  asr: {
    modelDir: "",
    sampleRate: 16000,
    language: "ru",
    device: "cpu",
    beamSize: 5,
  },
  output: {
    dir: "",
    formats: ["txt", "srt", "json"],
  },
  logging: {
    level: "info",
    file: true,
    maxSizeMb: 5,
    keepFiles: 3,
  },
};

// ===== События от бэка (Tauri events) =====

export type BackendEvent =
  | { kind: "job:created"; job: Job }
  | { kind: "job:updated"; id: JobId; update: Partial<Job> }
  | { kind: "job:log"; id: JobId; line: string }
  | { kind: "job:done"; id: JobId; transcriptPath: string; preview: string }
  | { kind: "job:failed"; id: JobId; error: string };
