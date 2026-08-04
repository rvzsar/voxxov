# Voxxov

Desktop application for video downloading (yt-dlp) and Russian speech recognition using [GigaAM-V3](https://github.com/salute-developers/GigaAM).

Written in Rust (Tauri 2) + Svelte 5. Portable — all data lives next to the executable.

Русская версия: [README.ru.md](./README.ru.md).

## Pipeline

```
URL or local file
  → yt-dlp                 (download)
  → ffmpeg                 (→ 16 kHz mono WAV)
  → SileroVad              (segmentation: 0.25–30s speech chunks)
  → GigaAM-V3 RNN-T INT8   (sherpa-onnx, one decode per chunk)
  → TXT / SRT / JSON
```

- Parallel download / ffmpeg stages; single ASR decoder at a time (CPU-serialized via semaphore)
- Cancellation at any stage
- SOCKS5 / HTTP(S) proxy with user/pass and no-proxy list
- ASR devices: CPU / CUDA / DirectML / OpenVINO
- Beam search (1..64), greedy fallback
- `cmd:` prefix in `modelDir` — delegate ASR to external CLI
- Auto-downloads yt-dlp, ffmpeg, GigaAM model, and SileroVad on first launch
- Real-time progress: per-stage speed / ETA, ASR RTF (real-time factor)

## How ASR works

1. **SileroVad** (`silero_vad.onnx`, 629 KB) splits the audio into speech segments of 0.25–30 seconds each. Boundaries fall on silences ≥ 500 ms — never mid-word.
2. Each segment is sent to **GigaAM-V3 RNN-T INT8** (`v3_rnnt_encoder_int8.onnx`, ~215 MB) as a single decode call. The model receives 25–3000 fbank frames of context — enough to recognize whole phrases.
3. Per-token timestamps from the decoder are grouped into display segments (max 25 tokens or at sentence boundaries).
4. Output: `text` (full clean text), `tokens` (BPE strings), `timestamps` (per-token seconds), `durations` (per-token durations).

The pipeline matches [gigaam.transcribe_longform](https://github.com/salute-developers/GigaAM) and [ekhodzitsky/gigastt](https://github.com/ekhodzitsky/gigastt) architecture — same model, same config, same chunking strategy.

**WER**: e2e_rnnt head with punctuation and casing built into the model.

## bench.json — per-job performance metrics

Written to `<workdir>/bench.json` after each job:

```json
{
  "metadata_sec":      3.49,
  "download_sec":    660.31,
  "extract_sec":       0.62,
  "transcribe_sec":  926.45,
  "total_sec":      1593.08,
  "asr_rtf":           0.051,
  "asr_throughput":  313497.72,
  "extract_mb_per_sec": 230.09
}
```

- `*_sec` — wall-clock time per stage
- `asr_rtf` — Real-Time Factor: `transcribe_sec / audio_duration_sec`. < 1.0 means faster than real-time
- `asr_throughput` — audio samples processed per second of wall time
- `extract_mb_per_sec` — ffmpeg input MB per second

## Quick Start

```cmd
scripts\dev.cmd
```

Auto-downloads:
- `yt-dlp.exe`, `ffmpeg.exe` → `<exe_dir>/bin/`
- 4 GigaAM-V3 RNN-T files (encoder_int8, decoder, joint, tokens) + `silero_vad.onnx` → `<exe_dir>/models/`

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
Voxxov/
├── voxxov.exe
├── data/             # config.toml, logs/, jobs/<id>/
├── models/           # GigaAM-V3 + silero_vad.onnx (auto-downloaded)
└── bin/              # yt-dlp, ffmpeg (auto-downloaded)
```

## Build

- Dev: `scripts\dev.cmd` (Windows)
- Release: GitHub Actions only (`release.yml`)

## License

**GPL-3.0-only** — see [LICENSE](./LICENSE).

GigaAM-V3 e2e_rnnt model files are downloaded from the `model-gigaam-v3` release of [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite); `silero_vad.onnx` comes from the official [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) releases. The GigaAM model is developed by [Salute Developers (Sber)](https://github.com/salute-developers/GigaAM) and is released under the **MIT license**.

## Credits

- [GigaAM](https://github.com/salute-developers/GigaAM) — speech recognition model, Salute Developers (Sber)
- [ekhodzitsky/gigastt](https://github.com/ekhodzitsky/gigastt) — GigaAM parameter research and benchmarks
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (csukuangfj) — ONNX runtime, static-linked C library for ASR inference (Apache-2.0)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — video downloader (Unlicense)
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — ffmpeg Windows binaries (GPL)
- [Tauri](https://github.com/tauri-apps/tauri) — desktop application framework (Apache-2.0 / MIT)
