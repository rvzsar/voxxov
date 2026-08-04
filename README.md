# Voxxov

Portable Windows desktop app: download a video (yt-dlp), extract audio (ffmpeg), transcribe Russian speech offline (GigaAM-V3 via sherpa-onnx), export to TXT / SRT / JSON.

The name is a palindrome of its own theme: **VOX** (Latin for "voice") and its mirror **XOV**.

Rust (Tauri 2) + Svelte 5. All data lives next to the `.exe`.

Русская версия: [README.ru.md](./README.ru.md).

## Features

- Offline ASR — no cloud, audio never leaves the machine
- Single-pass streaming pipeline: memory stays O(chunk), not O(file)
- Real-time progress: per-stage %, speed/ETA, ASR RTF (real-time factor)
- Cancellation at any stage
- Proxy: SOCKS5 / HTTP(S) with auth and no-proxy list
- ASR devices: CPU / CUDA / DirectML / OpenVINO, beam search (1–64)
- External ASR CLI via `cmd:` prefix in `modelDir`
- Batch processing of local folders (recursive scan)
- Auto-download of yt-dlp, ffmpeg, GigaAM model and SileroVAD on first run
- Hardware-aware: ASR threads are picked from physical core count at startup

## Pipeline

```
URL or local file
  → yt-dlp            download (parallel streams, per-stream progress)
  → ffmpeg            16 kHz mono WAV
  → SileroVAD         speech segments (streaming, 64 ms feed)
  → merge             chunks of 15–22 s (same constants as GigaAM segment_audio_file)
  → GigaAM-V3         e2e_rnnt, INT8, sherpa-onnx
  → TXT / SRT / JSON
```

- Chunks are decoded as soon as they close — one pass over the file
- Chunks longer than 30 s are split at the quietest point, not in the middle of a word
- Punctuation and casing are built into the model
- Every job writes `bench.json` (per-stage wall time, RTF, throughput)

## Requirements

- Windows 10/11 x64
- WebView2 runtime (preinstalled on Windows 11)
- ~1 GB free disk, 8 GB RAM recommended
- The `.exe` must live in a writable folder (portable, no installer)

## Getting started

```cmd
scripts\dev.cmd
```

Installs npm/pnpm deps and starts `tauri dev`. Prebuilt binaries are
published on GitHub Releases: versioned builds from `v*` tags, plus a
`rolling` release rebuilt on every push to `main`.

First run downloads the binaries and models (~330 MB) into `bin/` and
`models/`. Subsequent runs work offline.

## Configuration

`<exe_dir>/data/config.toml` — all sections optional, defaults apply.

```toml
[proxy]
kind = "none"          # none | http | https | socks5
host = "127.0.0.1"
port = 1080
username = ""
password = ""
noProxy = "localhost,127.0.0.1"

[download]
format = "bv*+ba/b"
maxHeight = 1080       # 0 = no limit
audioOnly = false
embedSubs = false
concurrentFragments = 4
retries = 3
overwrite = false
cookieFile = ""        # Netscape-format cookies file
userAgent = ""
mirrorPrefix = ""      # mirror prefix for GitHub downloads, e.g. "https://mirror.example.com/"

[asr]
modelDir = ""          # empty = auto-download; "cmd:cli --args" = external ASR
sampleRate = 16000     # fixed: GigaAM/VAD are 16 kHz only
language = "ru"
device = "cpu"         # cpu | cuda | directml | openvino
beamSize = 5           # 1 = greedy

[output]
dir = ""               # copy transcripts here too; empty = job folder only
formats = ["txt", "srt", "json"]

[logging]
level = "info"         # error | warn | info | debug | trace
file = true
maxSizeMb = 5
keepFiles = 3
```

## Layout

```
voxxov.exe
├── data/
│   ├── config.toml
│   ├── logs/app.log
│   └── jobs/<job_id>/   ← video, audio.wav, transcript.{txt,srt,json}, bench.json
├── models/              ← GigaAM-V3 + silero_vad.onnx (auto-downloaded)
└── bin/                 ← yt-dlp.exe, ffmpeg.exe (auto-downloaded)
```

## Performance

`<job_id>/bench.json` after each job:

```json
{
  "metadata_sec": 3.5,
  "download_sec": 648.4,
  "extract_sec": 2.2,
  "transcribe_sec": 267.8,
  "total_sec": 922.2,
  "asr_rtf": 0.09,
  "asr_throughput": 313497.7,
  "extract_mb_per_sec": 230.1
}
```

`asr_rtf` < 1.0 means faster than real-time. On a mid-range desktop CPU the
transcribe stage runs at roughly 0.09× real-time (≈11× faster than the
audio duration).

## License

App: **GPL-3.0-only** — see [LICENSE](./LICENSE).

Third-party components:
- [GigaAM](https://github.com/salute-developers/GigaAM) (Sber) — MIT; model files are downloaded from the `model-gigaam-v3` release of [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite)
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — Apache-2.0; `silero_vad.onnx` from its official releases
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — Unlicense
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — GPL
- [Tauri](https://github.com/tauri-apps/tauri) — Apache-2.0 / MIT
