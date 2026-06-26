# Voxxov

Десктоп-приложение для скачивания видео (yt-dlp) и распознавания русской речи на базе [GigaAM-V3](https://github.com/salute-developers/GigaAM).

Rust (Tauri 2) + Svelte 5. Портативно — все данные рядом с .exe.

English version: [README.md](./README.md).

## Пайплайн

```
URL или локальный файл
  → yt-dlp                 (загрузка)
  → ffmpeg                 (→ 16 kHz mono WAV)
  → SileroVad              (сегментация: 0.25–30s куски речи)
  → GigaAM-V3 RNN-T INT8   (sherpa-onnx, по одному decode на чанк)
  → TXT / SRT / JSON
```

- Параллельные стадии download / ffmpeg; одновременно декодирует только одну ASR (CPU-сериализация через семафор)
- Отмена на любой стадии
- SOCKS5 / HTTP(S) прокси с user/pass и no-proxy списком
- Устройства ASR: CPU / CUDA / DirectML / OpenVINO
- Beam search (1..64), greedy по умолчанию
- Префикс `cmd:` в `modelDir` — внешний CLI для ASR
- Авто-скачивание yt-dlp, ffmpeg, модели GigaAM и SileroVad при первом запуске
- Прогресс в реальном времени: per-stage скорость/ETA, ASR RTF (real-time factor)

## Как работает ASR

1. **SileroVad** (`silero_vad.onnx`, 629 KB) режет аудио на речевые сегменты по 0.25–30 сек. Границы — на паузах ≥ 500 мс, никогда не посреди слова.
2. Каждый сегмент отдаётся в **GigaAM-V3 RNN-T INT8** (`v3_rnnt_encoder_int8.onnx`, ~215 MB) одним вызовом decode. Модель получает 25–3000 fbank-фреймов контекста — достаточно для распознавания целых фраз.
3. Per-token таймстампы из декодера группируются в display-сегменты (макс 25 токенов или на границе предложения).
4. На выходе: `text` (полный чистый текст), `tokens` (BPE-строки), `timestamps` (per-token секунды), `durations` (per-token длительности).

Пайплайн повторяет архитектуру [gigaam.transcribe_longform](https://github.com/salute-developers/GigaAM) и [ekhodzitsky/gigastt](https://github.com/ekhodzitsky/gigastt) — та же модель, тот же конфиг, та же стратегия чанкинга.

**WER**: rnnt голова даёт ~3.55% на чистом чтении (vs ~8.6% у e2e_rnnt). Выход — lowercase без пунктуации; при необходимости добавить постпроцессинг.

## bench.json — перформанс-метрики по задаче

Пишется в `<workdir>/bench.json` после каждой задачи:

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

- `*_sec` — wall-clock время стадии
- `asr_rtf` — Real-Time Factor: `transcribe_sec / audio_duration_sec`. < 1.0 = быстрее realtime
- `asr_throughput` — аудио-семплов в секунду wall-time
- `extract_mb_per_sec` — входных MB ffmpeg в секунду

## Запуск

```cmd
scripts\dev.cmd
```

Автоматически скачивает:
- `yt-dlp.exe`, `ffmpeg.exe` → `<exe_dir>/bin/`
- 4 файла GigaAM-V3 RNN-T (encoder_int8, decoder, joint, tokens) + `silero_vad.onnx` → `<exe_dir>/models/`

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
Voxxov/
├── voxxov.exe
├── data/             # config.toml, logs/, jobs/<id>/, bench.json, transcripts/
├── models/           # GigaAM-V3 + silero_vad.onnx (авто-скачиваются)
└── bin/              # yt-dlp, ffmpeg (авто-скачиваются)
```

## Сборка

- Dev: `scripts\dev.cmd` (Windows)
- Release: только через GitHub Actions (`release.yml`)

## Лицензия

**GPL-3.0-only** — см. [LICENSE](./LICENSE).

Файлы модели GigaAM (rnnt голова, INT8 квантизация) скачиваются из релизов [ekhodzitsky/gigastt](https://github.com/ekhodzitsky/gigastt). Сама модель GigaAM разработана [Salute Developers (Sber)](https://github.com/salute-developers/GigaAM) под **некоммерческой лицензией** — для коммерческого использования требуется отдельное соглашение с правообладателем.

## Благодарности

- [GigaAM](https://github.com/salute-developers/GigaAM) — модель распознавания речи, Salute Developers (Sber)
- [ekhodzitsky/gigastt](https://github.com/ekhodzitsky/gigastt) — упаковка файлов модели GigaAM-V3 rnnt, INT8 квантизация и хостинг релизов
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (csukuangfj) — ONNX-рантайм, статически слинкованная C-библиотека для ASR-инференса (Apache-2.0)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — загрузчик видео (Unlicense)
- [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) — Windows-бинари ffmpeg (GPL)
- [Tauri](https://github.com/tauri-apps/tauri) — десктоп-фреймворк (Apache-2.0 / MIT)
