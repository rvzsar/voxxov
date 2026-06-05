# GigaAM Desktop

Десктопное приложение для скачивания видео (yt-dlp) и распознавания русской речи (GigaAM-V3 RNNT) с ускорением на Intel через OpenVINO.

## Стек

- **Tauri 2.x** — нативный десктоп-обвес (Windows / Linux / macOS).
- **Rust** — оркестрация, скачивание, обработка аудио, инференс ONNX.
- **Svelte 5** (runes) + **Vite** + **TypeScript** — UI.
- **ONNX Runtime** (`ort`) + **OpenVINO Execution Provider** — инференс GigaAM-RNNT.
- **yt-dlp** + **ffmpeg** — внешние sidecar-бинари.

## Железо (target)

- Intel Core Ultra 5 125U (12 ядер, AVX-VNNI, NPU Intel AI Boost).
- Без дискретного GPU → основной путь инференса: OpenVINO CPU.
- Сборка — **только на GitHub Actions** (`windows-2022`).

## Структура

```
.
├── apps/desktop/           # Tauri-приложение (Rust + Svelte)
│   ├── src/                # Svelte 5 фронт
│   └── src-tauri/          # Rust-бэкенд
├── tools/                  # Утилиты разработчика (экспорт ONNX, sidecars)
├── scripts/                # Локальные скрипты (PowerShell)
├── models/                 # Сюда скачиваются ONNX-модели
└── .github/workflows/      # CI/CD: сборка и релизы
```

## Быстрый старт (разработка UI)

Локально компилировать Rust **не нужно** — это делает GitHub Actions. Достаточно Node.js 20+.

```powershell
cd apps/desktop
npm install
npm run dev
```

Откроется dev-сервер Svelte на `http://localhost:1420`. UI работает в мок-режиме, без вызова Rust.

## Сборка релизного билда

Сборка происходит на GitHub Actions при push в `main` или вручную через вкладку Actions.
Артефакты (`.msi` / `.exe` для Windows) публикуются в Releases.

## Конфигурация

Первый запуск создаёт `~/.config/gigaam-desktop/config.toml`. Структура:

```toml
[proxy]   # none | http | https | socks5 | custom
[download]
[asr]     # model_dir, device, threads, chunk_length_s
[output]  # formats, max_line_length
[logging]
```

Полная схема — в [`apps/desktop/src-tauri/src/core/config.rs`](apps/desktop/src-tauri/src/core/config.rs).

## Лицензия моделей

GigaAM-V3 распространяется под **GigaAM License (non-commercial)**.
Для коммерческого использования требуется отдельное соглашение с правообладателем (Salute).

## Лицензия кода

MIT — см. [LICENSE](./LICENSE).
