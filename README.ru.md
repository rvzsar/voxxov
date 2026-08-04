# Voxxov

Портативное Windows-приложение: скачивание видео (yt-dlp), извлечение аудио (ffmpeg), офлайн-распознавание русской речи (GigaAM-V3 через sherpa-onnx), экспорт в TXT / SRT / JSON.

Название — игра слов на собственную тему: **VOX** (лат. «голос») и его зеркальное отражение **XOV**.

Rust (Tauri 2) + Svelte 5. Все данные — рядом с `.exe`.

English version: [README.md](./README.md).

## Возможности

- Офлайн-ASR — без облака, аудио не покидает машину
- Одно-проходный стриминговый пайплайн: память O(чанк), а не O(файл)
- Прогресс в реальном времени: % по стадиям, скорость/ETA, RTF (real-time factor)
- Отмена на любой стадии
- Прокси: SOCKS5 / HTTP(S) с авторизацией и no-proxy списком
- Устройства ASR: CPU / CUDA / DirectML / OpenVINO, beam search (1–64)
- Внешний ASR-CLI через префикс `cmd:` в `modelDir`
- Пакетная обработка локальных папок (рекурсивное сканирование)
- Авто-скачивание yt-dlp, ffmpeg, модели GigaAM и SileroVAD при первом запуске
- Аппаратно-зависимые параметры: потоки ASR подбираются по числу физических ядер

## Пайплайн

```
URL или локальный файл
  → yt-dlp            скачивание (параллельные потоки, прогресс по каждому)
  → ffmpeg            16 кГц mono WAV
  → SileroVAD         речевые сегменты (стриминг, фид 64 мс)
  → склейка           чанки 15–22 с (те же константы, что в GigaAM segment_audio_file)
  → GigaAM-V3         e2e_rnnt, INT8, sherpa-onnx
  → TXT / SRT / JSON
```

- Чанк декодируется сразу после закрытия — один проход по файлу
- Чанки длиннее 30 с режутся в самом тихом месте, а не посреди слова
- Пунктуация и регистр встроены в модель
- Каждая задача пишет `bench.json` (время стадий, RTF, пропускная способность)

## Требования

- Windows 10/11 x64
- WebView2 runtime (предустановлен на Windows 11)
- ~1 ГБ на диске, рекомендуется 8 ГБ RAM
- `.exe` должен лежать в доступной для записи папке (портативно, без установщика)

## Запуск

```cmd
scripts\dev.cmd
```

Ставит npm/pnpm-зависимости и запускает `tauri dev`. Готовые бинарники
публикуются в GitHub Releases: версионные сборки по тегам `v*`, плюс
rolling-релиз, пересобираемый при каждом push в `main`.

При первом запуске скачиваются бинарники и модель (~330 МБ) в `bin/` и
`models/`. Дальше приложение работает офлайн.

## Конфигурация

`<exe_dir>/data/config.toml` — все секции опциональны, применяются defaults.

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
maxHeight = 1080       # 0 = без ограничения
audioOnly = false
embedSubs = false
concurrentFragments = 4
retries = 3
overwrite = false
cookieFile = ""        # файл cookies в формате Netscape
userAgent = ""
mirrorPrefix = ""      # префикс-зеркало для загрузок с GitHub, напр. "https://mirror.example.com/"

[asr]
modelDir = ""          # пусто = авто-скачивание; "cmd:cli --args" = внешний ASR
sampleRate = 16000     # фиксировано: GigaAM/VAD работают только на 16 кГц
language = "ru"
device = "cpu"         # cpu | cuda | directml | openvino
beamSize = 5           # 1 = greedy

[output]
dir = ""               # дополнительно копировать транскрипты сюда; пусто = только папка задачи
formats = ["txt", "srt", "json"]

[logging]
level = "info"         # error | warn | info | debug | trace
file = true
maxSizeMb = 5
keepFiles = 3
```

## Раскладка

```
voxxov.exe
├── data/
│   ├── config.toml
│   ├── logs/app.log
│   └── jobs/<job_id>/   ← видео, audio.wav, transcript.{txt,srt,json}, bench.json
├── models/              ← GigaAM-V3 + silero_vad.onnx (авто-скачиваются)
└── bin/                 ← yt-dlp.exe, ffmpeg.exe (авто-скачиваются)
```

## Производительность

`<job_id>/bench.json` после каждой задачи:

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

`asr_rtf` < 1.0 — быстрее реального времени. На среднестатистическом
десктопном CPU стадия распознавания идёт примерно со скоростью 0.09×
от реального времени (≈ в 11 раз быстрее длительности аудио).

## Лицензия

Приложение: **GPL-3.0-only** — см. [LICENSE](./LICENSE).

Сторонние компоненты:
- [GigaAM](https://github.com/salute-developers/GigaAM) (Сбер) — MIT; файлы модели скачиваются из релиза `model-gigaam-v3` репозитория [amidexe/govorun-lite](https://github.com/amidexe/govorun-lite)
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — Apache-2.0; `silero_vad.onnx` — из его официальных релизов
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — Unlicense
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — GPL
- [Tauri](https://github.com/tauri-apps/tauri) — Apache-2.0 / MIT
