# GigaAM Desktop

Десктопное приложение для скачивания видео (yt-dlp) и распознавания русской речи
([GigaAM V3](https://github.com/salute-developers/GigaAM)) с поддержкой прокси,
конфигурируемого пайплайна и удобного Svelte-UI.

## Стек

- **Tauri 2.x** — нативный десктоп (Windows / Linux / macOS).
- **Rust** — оркестрация, скачивание, ffmpeg-обвязка, ASR.
- **Svelte 5** (runes) + **Vite** + **TypeScript** — UI.
- **yt-dlp** + **ffmpeg** — внешние sidecar-бинари.
- **GigaAM v3** — нативный Rust-инференс через ONNX Runtime (`ort` crate).

## Железо (target)

- Intel Core Ultra 5 125U (12 ядер, AVX-VNNI, NPU Intel AI Boost).
- Инференс: GigaAM v3 (`v3_e2e_ctc` или `v3_rnnt`).

## Структура

```
.
├── apps/desktop/           # Tauri-приложение (Rust + Svelte)
│   ├── src/                # Svelte 5 фронт
│   │   ├── lib/components/ # URL-input, JobList, Settings, LogView, ...
│   │   └── lib/stores/     # jobs.svelte.ts (rune-стор)
│   └── src-tauri/          # Rust-бэкенд
│       ├── src/            # modules: config, pipeline, ytdlp, ffmpeg, asr, ...
│       └── tauri.conf.json
├── scripts/                # install-sidecars.ps1, dev.cmd
├── models/                 # сюда кладутся ONNX-модели GigaAM
└── .github/workflows/      # CI/CD
```

## Структура (portable)

Все мутабельные данные лежат **рядом с .exe**. Можно копировать всю папку
на USB, запускать с любого диска, не оставлять следов в системе.

```
GigaAM Desktop/                ← можно назвать как угодно
├── gigaam-desktop.exe
├── data/
│   ├── config.toml
│   ├── logs/app.log
│   ├── cache/
│   ├── downloads/              ← временные скачивания yt-dlp
│   ├── jobs/<job_id>/          ← промежуточные WAV
│   └── transcripts/<title>.{txt,srt,json}
├── models/                     ← сюда скачаются 4 файла GigaAM при первом запуске
│   ├── gigaam_v3_e2e_rnnt_encoder_int8.onnx
│   ├── gigaam_v3_e2e_rnnt_decoder.onnx
│   ├── gigaam_v3_e2e_rnnt_joint.onnx
│   └── gigaam_v3_e2e_rnnt_tokens.txt
└── bin/                        ← yt-dlp + ffmpeg (тоже авто-скачивание)
    ├── yt-dlp.exe
    └── ffmpeg.exe
```

**Override через env var** `GIGAAM_DATA_DIR=...` — если хочется вынести
данные на RAM-диск или на другой диск (но оставить .exe на месте).

**Если .exe в read-only месте** (например, `C:\Program Files\`) — приложение
упадёт при старте с понятной ошибкой. Решения: перенести .exe в
пишущуюся папку, или задать `GIGAAM_DATA_DIR`.

## Quick Start

1. Запустить dev-режим (yt-dlp + ffmpeg + модель GigaAM скачаются
   автоматически при первом запуске — в `<exe_dir>/bin/` и
   `<exe_dir>/models/` соответственно):

    ```cmd
    scripts\dev.cmd
    ```

2. Запустить распознавание. Папку с моделью можно указать в `Settings → ASR`
   (пусто = авто-скачивание в `<exe_dir>/models`).

## Sidecars

yt-dlp и ffmpeg **не поставляются в бинарь** приложения и не требуют
ручной установки. Крейт `yt-dlp` (GPL-3.0) скачивает их сам при первом
использовании и кладёт в `<exe_dir>/bin/`.

Если нужно указать другой путь (например, системный yt-dlp) — создайте
`bin/yt-dlp[.exe]` и `bin/ffmpeg[.exe]` в нужной директории до запуска.

## Конфигурация

Файл: `<exe_dir>/data/config.toml` (рядом с .exe, см. «Структура» выше).

```toml
[proxy]
kind = "socks5"        # none | http | https | socks5
host = "127.0.0.1"
port = 1080

[download]
format = "bv*+ba/b"
max_height = 1080
concurrent_fragments = 4
custom_ytdlp_path = "" # пусто → автодетект
custom_ffmpeg_path = ""

[asr]
# Пусто = авто-скачивание в <exe_dir>/models. Иначе — путь к папке с
# 4 файлами: encoder/decoder/joiner.onnx + tokens.txt.
model_dir = ""
sample_rate = 16000
language = "ru"

[output]
dir = ""               # пусто → <exe_dir>/data/transcripts
formats = ["txt", "srt", "json"]

[logging]
level = "info"
file = true
```

## Пайплайн

```
URL → yt-dlp (download) → ffmpeg (16kHz mono WAV + loudnorm) → GigaAM (ASR) → txt/srt/json
```

Все стадии и подробные логи приходят во frontend через Tauri-событие `job:event`.

## Поддерживаемые прокси

`none | http | https | socks5` (с user/password и `no_proxy`-списком).

## Лицензия моделей

GigaAM-V3 распространяется под **GigaAM License (non-commercial)**.
Для коммерческого использования требуется отдельное соглашение с правообладателем (Salute).

## Лицензия кода

**GPL-3.0-only** — см. [LICENSE](./LICENSE).

Использование крейта `yt-dlp` ([boul2gom/yt-dlp](https://github.com/boul2gom/yt-dlp))
также под GPL-3.0, поэтому весь проект — GPL-3.0.

Salute GigaAM модели — non-commercial (см. выше).
