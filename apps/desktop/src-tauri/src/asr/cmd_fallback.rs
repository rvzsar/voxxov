//! Fallback: запуск внешнего CLI для ASR (cmd:my-cli --flag).

use super::Transcription;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub async fn transcribe_cmd(
    state: &AppState,
    job_id: &str,
    audio: &Path,
    cmd_str: &str,
    cancel: CancellationToken,
) -> AppResult<Transcription> {
    let mut tokens = shell_split(cmd_str);
    let program = tokens.remove(0);
    tokens.push(audio.to_string_lossy().to_string());

    state.log_line(job_id, format!("ASR cmd: {program} {}", tokens.join(" ")));

    let mut cmd = Command::new(&program);
    cmd.args(&tokens);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::sidecar::hide_console(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Asr(format!("spawn {program}: {e}")))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Asr("no stderr".into()))?;
    let st = state.clone();
    let jid = job_id.to_string();
    let drain_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            st.log_line(&jid, format!("asr: {line}"));
        }
    });

    // stdout read в отдельной таске — чтобы tokio::select! мог отменить и
    // stdout-чтение, и wait на child одновременно.
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Asr("no stdout".into()))?;
    let read_task = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf).await;
        buf
    });

    let status = tokio::select! {
        res = child.wait() => res.map_err(|e| AppError::Asr(format!("wait: {e}"))),
        _ = cancel.cancelled() => {
            let _ = child.start_kill();
            drain_task.abort();
            read_task.abort();
            return Err(AppError::Cancelled);
        }
    }?;

    // Child вышел → stdout pipe закрыт → read_task досчитает мгновенно.
    let buf = read_task
        .await
        .map_err(|e| AppError::Asr(format!("stdout join: {e}")))?;

    if !status.success() {
        return Err(AppError::Asr(format!(
            "exit {:?}: {}",
            status.code(),
            buf
        )));
    }

    let text = parse_text(&buf).ok_or_else(|| AppError::Asr("no text in output".into()))?;
    // Внешний CLI не отдаёт per-token таймстампы; pipeline сделает fallback.
    Ok(Transcription {
        text,
        segments: Vec::new(),
    })
}

fn parse_text(out: &str) -> Option<String> {
    let trimmed = out.trim();
    if trimmed.is_empty() { return None; }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            return Some(t.to_string());
        }
    }
    Some(trimmed.to_string())
}

fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q: Option<char> = None;
    for c in s.chars() {
        match in_q {
            Some(q) if c == q => { in_q = None; }
            Some(_) => cur.push(c),
            None => match c {
                '"' | '\'' => in_q = Some(c),
                c if c.is_whitespace() => {
                    if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
                }
                _ => cur.push(c),
            },
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}
