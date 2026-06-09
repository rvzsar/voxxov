# GigaAM Desktop

Десктоп-приложение для скачивания видео (yt-dlp) и распознавания русской речи на базе [GigaAM-V3](https://github.com/salute-developers/GigaAM).

Rust (Tauri 2) + Svelte 5. Портативно — все данные рядом с .exe.

English version: [README.md](./README.md).

## Пайплайн

URL или локальный файл → yt-dlp → ffmpeg (16 kHz mono WAV) → GigaAM RNN-T (sherpa-onnx) → TXT / SRT / JSON

- Параллельные стадии download/ffmpeg; одновременно декодирует только одну ASR (CPU-сериализация через семафор)
- Отмена на любой стадии
- SOCKS5 / HTTP(S) прокси с user/pass и no-proxy списком
- Устройства ASR: CPU / CUDA / DirectML / OpenVINO
- Beam search (1..64), greedy по умолчанию
- Префикс `cmd:` в `modelDir` — внешний CLI для ASR
- Авто-скачивание yt-dlp, ffmpeg и модели GigaAM при первом запуске

## Запуск

```cmd
scripts\dev.cmd
```

Автоматически скачивает:
- `yt-dlp.exe`, `ffmpeg.exe` → `<exe_dir>/bin/`
- 4 файла GigaAM-V3 RNN-T (~320 МБ) → `<exe_dir>/models/`

Если .exe в read-only месте — приложение упадёт при старте с понятной ошибкой.

## Конфигурация

`<exe_dir>/data/config.toml` — все секции опциональны, при отсутствии применяются defaults.

```toml
[proxy]
kind = "socks5"        # none | http | https | socks5
host = "127.0.0.1"
port = 1080
noProxy = "localhost,127.0.0.1"

[download]
format = "bv*+ba/b"
maxHeight = 1080       # 0 = без ограничения
audioOnly = false
concurrentFragments = 4
retries = 3
cookieFile = ""
userAgent = ""

[asr]
modelDir = ""          # пусто = авто-скачивание; "cmd:..." = внешний CLI
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

## Раскладка

```
GigaAM Desktop/
├── gigaam-desktop.exe
├── data/             # config.toml, logs/, jobs/<id>/, transcripts/
├── models/           # авто-скачивается GigaAM-V3
└── bin/              # авто-скачиваются yt-dlp, ffmpeg
```

## Сборка

- Dev: `scripts\dev.cmd` (Windows)
- Release: только через GitHub Actions (`release.yml`)

## Лицензия

**GPL-3.0-only** — см. [LICENSE](./LICENSE).

Файлы модели GigaAM скачиваются из релизов [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite). Сама модель GigaAM разработана [Salute Developers (Sber)](https://github.com/salute-developers/GigaAM) под **некоммерческой лицензией** — для коммерческого использования требуется отдельное соглашение с правообладателем.

## Благодарности

- [GigaAM](https://github.com/salute-developers/GigaAM) — модель распознавания речи, Salute Developers (Sber)
- [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite) — упаковка и хостинг релизов файлов модели GigaAM-V3
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (csukuangfj) — ONNX-рантайм, статически слинкованная C-библиотека для ASR-инференса (Apache-2.0)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — загрузчик видео (Unlicense)
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — Windows-бинари ffmpeg (GPL)
- [Tauri](https://github.com/tauri-apps/tauri) — десктоп-фреймворк (Apache-2.0 / MIT)
