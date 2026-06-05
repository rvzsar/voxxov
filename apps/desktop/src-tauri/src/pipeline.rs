//! Главный пайплайн: enqueue → metadata → download → audio → transcribe → done.

use crate::asr;
use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::ffmpeg::FfmpegRunner;
use crate::paths;
use crate::state::AppState;
use crate::types::{Job, JobPatch, JobStage, Progress};
use crate::ytdlp::YtDlpRunner;
use chrono::Utc;
use std::path::PathBuf;

pub async fn run_job(state: AppState, cfg: AppConfig, job: Job) -> AppResult<()> {
    let id = job.id.clone();
    let token = state.register_token(&id);

    let result = run_inner(&state, &cfg, &job, token.clone()).await;

    match result {
        Ok((transcript_path, preview)) => {
            state.patch_job(&id, JobPatch {
                stage: Some(JobStage::Done),
                progress: Some(Progress { pct: 100.0, label: "Готово".into(), speed: None, eta: None }),
                finished_at: Some(Utc::now().to_rfc3339()),
                transcript_path: Some(transcript_path.to_string_lossy().to_string()),
                transcript_preview: Some(preview),
                error: None,
            });
            let _ = state.events.send(crate::types::BackendEvent::JobDone {
                id: id.clone(),
                transcript_path: transcript_path.to_string_lossy().to_string(),
                preview,
            });
        }
        Err(e) => {
            if matches!(e, AppError::Cancelled) {
                state.patch_job(&id, JobPatch {
                    stage: Some(JobStage::Cancelled),
                    progress: Some(Progress { pct: 100.0, label: "Отменено".into(), speed: None, eta: None }),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    error: Some("cancelled".into()),
                    ..Default::default()
                });
            } else {
                let msg = e.to_string();
                state.patch_job(&id, JobPatch {
                    stage: Some(JobStage::Failed),
                    progress: Some(Progress { pct: 100.0, label: "Ошибка".into(), speed: None, eta: None }),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    error: Some(msg.clone()),
                    ..Default::default()
                });
                let _ = state.events.send(crate::types::BackendEvent::JobFailed {
                    id: id.clone(),
                    error: msg,
                });
            }
        }
    }
    state.remove_token(&id);
    Ok(())
}

async fn run_inner(
    state: &AppState,
    cfg: &AppConfig,
    job: &Job,
    token: tokio_util::sync::CancellationToken,
) -> AppResult<(PathBuf, String)> {
    let ytdlp = YtDlpRunner::resolve(cfg)?;
    let ffmpeg = FfmpegRunner::resolve(cfg.download.custom_ffmpeg_path.as_deref())?;

    // 1) metadata
    state.set_stage(&job.id, JobStage::FetchingMetadata, "Получаем метаданные…");
    let media = ytdlp.fetch_metadata(&job.url).await?;
    state.patch_job(&job.id, JobPatch {
        media: Some(media.clone()),
        progress: Some(Progress { pct: 5.0, label: format!("{} · {}", media.title, short_dur(media.duration_sec)), speed: None, eta: None }),
        ..Default::default()
    });

    // 2) download
    let workdir = state.app_handle()
        .map(|h| paths::job_workdir(&h, &job.id))
        .unwrap_or_else(|| paths::jobs_dir(None).join(&job.id));
    std::fs::create_dir_all(&workdir)?;
    let tmpl = cfg.download.output_template.clone();
    let downloaded = {
        let st = state.clone();
        let id = job.id.clone();
        let url = job.url.clone();
        let wd = workdir.clone();
        let cfg2 = cfg.clone();
        let tk = token.clone();
        st.set_stage(&id, JobStage::Downloading, "Скачиваем видео…");
        ytdlp.download(&st, &id, &url, &wd, &tmpl, &cfg2, tk).await?
    };

    // 3) extract audio
    let audio_wav = workdir.join("audio.wav");
    {
        let st = state.clone();
        let id = job.id.clone();
        let in_p = downloaded.clone();
        let out_p = audio_wav.clone();
        let sr = cfg.asr.sample_rate;
        let tk = token.clone();
        st.set_stage(&id, JobStage::ExtractingAudio, "Извлекаем аудио…");
        let st2 = st.clone();
        let id2 = id.clone();
        ffmpeg.extract_audio(
            &in_p,
            &out_p,
            sr,
            true,
            tk,
            move |line| st2.log_line(&id2, format!("ffmpeg: {line}")),
        ).await?;
    }

    // 4) transcribe
    let st2 = state.clone();
    let id2 = job.id.clone();
    state.set_stage(&id2, JobStage::Transcribing, "Распознаём речь…");
    let text = asr::transcribe(&st2, &id2, &audio_wav, &cfg.asr).await?;

    // 5) write outputs (txt/srt/json) в output dir
    let out_dir = state.app_handle()
        .map(|h| paths::transcripts_dir(Some(&h)))
        .unwrap_or_else(|| paths::transcripts_dir(None));
    std::fs::create_dir_all(&out_dir)?;
    let base = media.title.clone();
    let safe = sanitize(&base);
    let stem = if safe.is_empty() { media.id.clone() } else { safe };
    let mut last_path: Option<PathBuf> = None;
    for fmt in &cfg.output.formats {
        let path = out_dir.join(format!("{stem}.{fmt}"));
        let body = match fmt.as_str() {
            "txt" => text.clone(),
            "srt" => text_to_srt(&text),
            "json" => format!("{{\"title\":{:?},\"id\":{:?},\"text\":{:?}}}\n",
                              media.title, media.id, text),
            _ => text.clone(),
        };
        std::fs::write(&path, body)?;
        last_path = Some(path);
    }

    let preview = if text.len() > 280 {
        let mut end = 280;
        while !text.is_char_boundary(end) && end > 0 { end -= 1; }
        format!("{}…", &text[..end])
    } else { text };
    Ok((last_path.unwrap_or(out_dir.join(format!("{stem}.txt"))), preview))
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

fn text_to_srt(_text: &str) -> String {
    // Без сегментных меток из GigaAM оставим один общий блок.
    // Полноценные тайм-коды подключим, когда ASR будет возвращать сегменты.
    String::new()
}

fn short_dur(s: Option<u64>) -> String {
    let s = match s { Some(s) => s, None => return "".to_string() };
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 { format!("{h}:{:02}:{:02}", m, sec) } else { format!("{m}:{:02}", sec) }
}
