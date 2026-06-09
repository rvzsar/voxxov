# GigaAM Desktop

Desktop application for video downloading (yt-dlp) and Russian speech recognition using [GigaAM-V3](https://github.com/salute-developers/GigaAM).

Written in Rust (Tauri 2) + Svelte 5. Portable — all data lives next to the executable.

Русская версия: [README.ru.md](./README.ru.md).

## Pipeline

URL or local file → yt-dlp → ffmpeg (16 kHz mono WAV) → GigaAM RNN-T (sherpa-onnx) → TXT / SRT / JSON

- Parallel download/ffmpeg stages; single ASR decoder at a time (CPU-serialized via semaphore)
- Cancellation at any stage
- SOCKS5 / HTTP(S) proxy with user/pass and no-proxy list
- ASR devices: CPU / CUDA / DirectML / OpenVINO
- Beam search (1..64), greedy fallback
- `cmd:` prefix in `modelDir` — delegate ASR to external CLI
- Auto-downloads yt-dlp, ffmpeg, and GigaAM model files on first launch

## Quick Start

```cmd
scripts\dev.cmd
```

Auto-downloads:
- `yt-dlp.exe`, `ffmpeg.exe` → `<exe_dir>/bin/`
- 4 GigaAM-V3 RNN-T files (~320 MB) → `<exe_dir>/models/`

If the executable lives in a read-only location, the app panics at startup with a clear error.

## Configuration

`<exe_dir>/data/config.toml` — all sections optional, defaults applied if the file or any section is missing.

```toml
[proxy]
kind = "socks5"        # none | http | https | socks5
host = "127.0.0.1"
port = 1080
noProxy = "localhost,127.0.0.1"

[download]
format = "bv*+ba/b"
maxHeight = 1080       # 0 = no limit
audioOnly = false
concurrentFragments = 4
retries = 3
cookieFile = ""
userAgent = ""

[asr]
modelDir = ""          # empty = auto-download; "cmd:..." = external CLI
sampleRate = 16000
language = "ru"
device = "cpu"         # cpu | cuda | directml | openvino
beamSize = 5

[output]
dir = ""
formats = ["txt", "srt", "json"]

[logging]
level = "info"         # error | warn | info | debug | trace
file = true
maxSizeMb = 5
keepFiles = 3
```

## Layout

```
GigaAM Desktop/
├── gigaam-desktop.exe
├── data/             # config.toml, logs/, jobs/<id>/, transcripts/
├── models/           # auto-downloaded GigaAM-V3
└── bin/              # auto-downloaded yt-dlp, ffmpeg
```

## Build

- Dev: `scripts\dev.cmd` (Windows)
- Release: GitHub Actions only (`release.yml`)

## License

**GPL-3.0-only** — see [LICENSE](./LICENSE).

GigaAM model files are downloaded from [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite) releases. The GigaAM model itself is developed by [Salute Developers (Sber)](https://github.com/salute-developers/GigaAM) under a **non-commercial license** — commercial use requires a separate agreement with the rights holder.

## Credits

- [GigaAM](https://github.com/salute-developers/GigaAM) — speech recognition model, Salute Developers (Sber)
- [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite) — GigaAM-V3 model file packaging and release hosting
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (csukuangfj) — ONNX runtime, static-linked C library for ASR inference (Apache-2.0)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — video downloader (Unlicense)
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — ffmpeg Windows binaries (GPL)
- [Tauri](https://github.com/tauri-apps/tauri) — desktop application framework (Apache-2.0 / MIT)
