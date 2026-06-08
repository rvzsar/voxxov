//! Главный пайплайн: enqueue → metadata → download → audio → transcribe → done.

use crate::asr::{self, TimedSegment};
use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::ffmpeg::{FfmpegEvent, FfmpegRunner};
use crate::models;
use crate::paths;
use crate::state::AppState;
use crate::types::{Job, JobSource, JobStage, JobUpdate, MediaInfo, Progress};
use chrono::Utc;
use std::path::PathBuf;

async fn ensure_asr_model(state: &AppState, job_id: &str) -> AppResult<PathBuf> {
    // User-configured path имеет приоритет.
    let user_dir = state.config().asr.model_dir.clone();
    if !user_dir.is_empty() {
        return Ok(PathBuf::from(user_dir));
    }
    let dir = models::default_model_dir();
    let status = models::check_status(&dir);
    if status.missing.is_empty() {
        return Ok(dir);
    }
    state.log_line(
        job_id,
        format!(
            "ASR: downloading {} file(s) (~{} MB) from GitHub release {}",
            status.missing.len(),
            status
                .missing
                .iter()
                .map(|m| m.size.max(0))
                .sum::<u64>()
                / 1_000_000,
            status.release_tag
        ),
    );
    models::download_all(&dir, |downloaded, total| {
        // Per-file callback, межфайловый прогресс не виден — только лог.
        tracing::info!("model: {} / {} bytes", downloaded, total);
    })
    .await?;
    Ok(dir)
}

pub async fn run_job(state: AppState, cfg: AppConfig, job: Job) -> AppResult<()> {
    let id = job.id.clone();
    let token = state.register_token(&id);

    let result = run_inner(&state, &cfg, &job, token.clone()).await;

    match result {
        Ok((transcript_path, preview)) => {
            state.update_job(
                &id,
                JobUpdate {
                    stage: Some(JobStage::Done),
                    progress: Some(Progress {
                        pct: 1.0,
                        label: "Готово".into(),
                        speed: None,
                        eta: None,
                    }),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    transcript_path: Some(transcript_path.to_string_lossy().to_string()),
                    transcript_preview: Some(preview.clone()),
                    error: None,
                    media: None,
                },
            );
            let _ = state.events.send(crate::types::BackendEvent::JobDone {
                id: id.clone(),
                transcript_path: transcript_path.to_string_lossy().to_string(),
                preview,
            });
        }
        Err(e) => {
            let (stage, error_msg) = if matches!(e, AppError::Cancelled) {
                (JobStage::Cancelled, "cancelled by user".to_string())
            } else {
                (JobStage::Failed, e.to_string())
            };
            state.update_job(
                &id,
                JobUpdate {
                    stage: Some(stage),
                    progress: Some(Progress {
                        pct: 1.0,
                        label: stage_label(&stage).into(),
                        speed: None,
                        eta: None,
                    }),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    error: Some(error_msg.clone()),
                    ..Default::default()
                },
            );
            if !matches!(e, AppError::Cancelled) {
                let _ = state.events.send(crate::types::BackendEvent::JobFailed {
                    id: id.clone(),
                    error: error_msg,
                });
            }
        }
    }
    Ok(())
}

async fn run_inner(
    state: &AppState,
    cfg: &AppConfig,
    job: &Job,
    token: tokio_util::sync::CancellationToken,
) -> AppResult<(PathBuf, String)> {
    let ffmpeg = FfmpegRunner::resolve(None)?;

    let workdir = paths::job_workdir(&job.id);
    std::fs::create_dir_all(&workdir)?;

    // 1-2) Metadata + source file (URL → yt-dlp, local → direct).
    let (media, source_file) = if job.source == JobSource::LocalFile {
        let path = PathBuf::from(&job.url);
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let media = MediaInfo {
            id: String::new(),
            url: job.url.clone(),
            title: name,
            uploader: None,
            duration_sec: None,
            thumbnail: None,
        };
        state.update_job(
            &job.id,
            JobUpdate {
                media: Some(media.clone()),
                progress: Some(Progress {
                    pct: 0.05,
                    label: format!("Локальный файл: {}", media.title),
                    speed: None,
                    eta: None,
                }),
                ..Default::default()
            },
        );
        (media, path)
    } else {
        state.set_stage(&job.id, JobStage::FetchingMetadata, "Получаем метаданные…");
        let media = crate::ytdlp::YtDlpRunner::fetch_metadata(&job.url).await?;
        state.update_job(
            &job.id,
            JobUpdate {
                media: Some(media.clone()),
                progress: Some(Progress {
                    pct: 0.05,
                    label: format!("{} · {}", media.title, short_dur(media.duration_sec)),
                    speed: None,
                    eta: None,
                }),
                ..Default::default()
            },
        );
        state.set_stage(&job.id, JobStage::Downloading, "Скачиваем видео…");
        let downloaded = crate::ytdlp::YtDlpRunner::download(
            &state, &job.id, &job.url, &workdir, cfg, token.clone(),
        )
        .await?;
        (media, downloaded)
    };

    // 3) Extract audio через ffmpeg.
    let audio_wav = workdir.join("audio.wav");
    {
        state.set_stage(
            &job.id,
            JobStage::ExtractingAudio,
            "Извлекаем аудио…",
        );
        let in_p = source_file;
        let out_p = audio_wav.clone();
        let sr = cfg.asr.sample_rate;
        let st2 = state.clone();
        let id2 = job.id.clone();
        ffmpeg
            .extract_audio(
                &in_p,
                &out_p,
                sr,
                true,
                media.duration_sec.map(|d| d as f32),
                token.clone(),
                move |event| match event {
                    FfmpegEvent::Log(line) => st2.log_line(&id2, format!("ffmpeg: {line}")),
                    FfmpegEvent::Progress(pct) => st2.update_job(
                        &id2,
                        JobUpdate {
                            progress: Some(Progress {
                                pct,
                                label: "Извлекаем аудио…".into(),
                                speed: None,
                                eta: None,
                            }),
                            ..Default::default()
                        },
                    ),
                },
            )
            .await?;
    }

    // 4) Transcribe.
    state.set_stage(&job.id, JobStage::Transcribing, "Распознаём речь…");
    let model_path = ensure_asr_model(&state, &job.id).await?;
    let transcription = asr::transcribe(
        &state.clone(),
        &job.id,
        &audio_wav,
        &model_path.to_string_lossy(),
        &cfg.asr,
        token.clone(),
    )
    .await?;
    let text = transcription.text;
    let segments = transcription.segments;

    // 5) Write outputs (txt/srt/json).
    let out_dir = paths::transcripts_dir();
    std::fs::create_dir_all(&out_dir)?;
    let stem = if media.title.is_empty() {
        media.id.clone()
    } else {
        sanitize(&media.title)
    };
    let mut last_path: Option<PathBuf> = None;
    for fmt in &cfg.output.formats {
        let path = out_dir.join(format!("{stem}.{fmt}"));
        let body = match fmt.as_str() {
            "txt" => {
                if segments.is_empty() {
                    // No timestamps from ASR (cmd: fallback) — write raw.
                    text.clone()
                } else {
                    text_to_txt_from_segments(&segments)
                }
            }
            "srt" => {
                if segments.is_empty() {
                    text_to_srt_fallback(&text)
                } else {
                    text_to_srt_from_segments(&segments)
                }
            }
            "json" => segments_to_json(&media, &text, &segments),
            _ => text.clone(),
        };
        std::fs::write(&path, body)?;
        last_path = Some(path);
    }

    // Полный текст (без обрезки) — UI сам обрежет при показе.
    Ok((last_path.unwrap_or(out_dir.join(format!("{stem}.txt"))), text))
}

fn stage_label(s: &JobStage) -> &'static str {
    match s {
        JobStage::Done => "Готово",
        JobStage::Failed => "Ошибка",
        JobStage::Cancelled => "Отменено",
        _ => "",
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn text_to_srt_fallback(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    // Без per-token timestamps (cmd_fallback): оценка ~150 симв/мин.
    let duration_sec = ((text.len() as f64 / 150.0) * 60.0).ceil() as u64;
    let end = format_srt_time(duration_sec);
    format!("1\n00:00:00,000 --> {end}\n{text}\n")
}

fn text_to_srt_from_segments(segments: &[TimedSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if seg.text.is_empty() {
            continue;
        }
        let start = format_srt_time_f(seg.start_sec);
        let end = format_srt_time_f(seg.end_sec.max(seg.start_sec + 0.05));
        let text = &seg.text;
        out.push_str(&format!(
            "{i}\n{start} --> {end}\n{text}\n\n"
        ));
    }
    out
}

/// Человеко-читаемая TXT-расшифровка: одна строка на сегмент с
/// `[HH:MM:SS → HH:MM:SS] текст`. Удобно для чтения, в отличие от
/// SRT (где `-->` и номера) и от голого текста (всё в одну строку).
fn text_to_txt_from_segments(segments: &[TimedSegment]) -> String {
    let mut out = String::new();
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "[{} → {}] {}\n",
            format_hms(seg.start_sec),
            format_hms(seg.end_sec),
            text,
        ));
    }
    out
}

/// Секунды → `HH:MM:SS` (zero-padded). Для видео короче часа HH=00.
fn format_hms(sec: f32) -> String {
    let total = sec.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn segments_to_json(media: &MediaInfo, text: &str, segments: &[TimedSegment]) -> String {
    let segs: Vec<String> = segments
        .iter()
        .map(|s| {
            // serde_json::Value::String экранирует кавычки/backslash/etc.
            format!(
                "{{\"start\":{:.3},\"end\":{:.3},\"text\":{}}}",
                s.start_sec,
                s.end_sec,
                serde_json::Value::String(s.text.clone()),
            )
        })
        .collect();
    format!(
        "{{\"title\":{},\"id\":{},\"text\":{},\"segments\":[{}]}}\n",
        serde_json::Value::String(media.title.clone()),
        serde_json::Value::String(media.id.clone()),
        serde_json::Value::String(text.to_string()),
        segs.join(","),
    )
}

fn format_srt_time(total_sec: u64) -> String {
    let h = total_sec / 3600;
    let m = (total_sec % 3600) / 60;
    let s = total_sec % 60;
    format!("{h:02}:{m:02}:{s:02},000")
}

fn format_srt_time_f(total_sec: f32) -> String {
    let total_ms = (total_sec.max(0.0) * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms / 60_000) % 60;
    let s = (total_ms / 1000) % 60;
    let ms = total_ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

fn short_dur(s: Option<u64>) -> String {
    let s = match s {
        Some(s) => s,
        None => return String::new(),
    };
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}
