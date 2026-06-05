# GigaAM Desktop

Десктопное приложение для скачивания видео (yt-dlp) и распознавания русской речи
([GigaAM V3](https://github.com/salute-developers/GigaAM)) с поддержкой прокси,
конфигурируемого пайплайна и удобного Svelte-UI.

## Стек

- **Tauri 2.x** — нативный десктоп (Windows / Linux / macOS).
- **Rust** — оркестрация, скачивание, ffmpeg-обвязка, ASR.
- **Svelte 5** (runes) + **Vite** + **TypeScript** — UI.
- **yt-dlp** + **ffmpeg** — внешние sidecar-бинари.
- **GigaAM v3** — Python-сабпроцесс (или подключаемый CLI).

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
├── scripts/                # install-sidecars.ps1, dev.cmd, setup-gigaam.cmd
├── models/                 # сюда скачиваются модели GigaAM
└── .github/workflows/      # CI/CD
```

## Быстрый старт

1. Поставить зависимости:

    ```powershell
    pwsh scripts/install-sidecars.ps1   # yt-dlp + ffmpeg в ./sidecars
    pwsh scripts/setup-gigaam.cmd       # Python venv + gigaam (опционально)
    ```

2. Указать пути к бинарям и модели в `Settings` UI (или в
   `%APPDATA%/GigaAM/config.toml`).

3. Запустить dev-режим:

    ```cmd
    scripts\dev.cmd
    ```

## Конфигурация

Файл: `%APPDATA%\GigaAM\config.toml` (Windows), `~/.config/GigaAM/...` (Linux).

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
model_path = ""        # путь к ONNX/модели или "cmd:my-cli --flag"
sample_rate = 16000
language = "ru"

[output]
dir = ""               # пусто → ~/.../transcripts
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

MIT — см. [LICENSE](./LICENSE).
