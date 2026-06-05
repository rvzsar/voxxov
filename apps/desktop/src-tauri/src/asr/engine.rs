//! ASR через Python сабпроцесс, вызывающий GigaAM.
//!
//! Ожидается, что в окружении установлены:
//!   * `gigaam` (pip install .) с моделью, скачанной по инструкции
//!   * либо кастомный CLI, который принимает `<input.wav>` и
//!     печатает JSON `{ "text": "..." }` в stdout.
//!
//! Если `model_path` начинается с `cmd:` — содержимое после префикса
//! трактуется как команда: `{cmd} {input}`.
//! Иначе используется `python -m gigaam ... <input>`.

use crate::config::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub async fn transcribe(
    state: &AppState,
    job_id: &str,
    audio: &Path,
    cfg: &AsrConfig,
) -> AppResult<String> {
    if !audio.is_file() {
        return Err(AppError::Asr(format!("audio not found: {}", audio.display())));
    }

    let (program, args) = build_command(cfg, audio);
    state.log_line(job_id, format!("ASR: {program} {}", args.join(" ")));

    let mut cmd = Command::new(&program);
    cmd.args(&args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::Asr(format!("spawn {program}: {e}")))?;
    let stdout = child.stdout.take().ok_or_else(|| AppError::Asr("no stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| AppError::Asr("no stderr".into()))?;

    let st = state.clone();
    let jid = job_id.to_string();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            st.log_line(&jid, format!("asr: {line}"));
        }
    });

    let token = CancellationToken::new();
    let cancel = state.cancel_tokens.read().get(job_id).cloned();
    if let Some(t) = cancel { /* опционально: не используем токен в этой версии */ let _ = t; }

    let status = tokio::select! {
        res = child.wait() => res.map_err(|e| AppError::Asr(format!("wait: {e}")))?,
        _ = token.cancelled() => {
            let _ = child.kill().await;
            return Err(AppError::Cancelled);
        }
    };
    let mut buf = String::new();
    let mut reader = BufReader::new(stdout);
    use tokio::io::AsyncReadExt;
    reader.read_to_string(&mut buf).await.ok();
    if !status.success() {
        return Err(AppError::Asr(format!("exit {:?}: {}", status.code(), buf)));
    }
    parse_text(&buf).ok_or_else(|| AppError::Asr("no text in output".into()))
}

fn build_command(cfg: &AsrConfig, audio: &Path) -> (String, Vec<String>) {
    // Кастомная команда: `cmd:my-cli --flag`
    if let Some(stripped) = cfg.model_path.strip_prefix("cmd:") {
        let mut tokens = shell_split(stripped);
        let program = tokens.remove(0);
        tokens.push(audio.to_string_lossy().to_string());
        return (program, tokens);
    }
    // По умолчанию — gigaam
    let program = "gigaam".to_string();
    let mut args: Vec<String> = vec!["transcribe".into()];
    if !cfg.model_path.is_empty() {
        args.push("--model".into()); args.push(cfg.model_path.clone());
    }
    if !cfg.language.is_empty() {
        args.push("--language".into()); args.push(cfg.language.clone());
    }
    args.push("--output-format".into()); args.push("json".into());
    args.push(audio.to_string_lossy().to_string());
    (program, args)
}

fn parse_text(out: &str) -> Option<String> {
    let trimmed = out.trim();
    if trimmed.is_empty() { return None; }
    // 1) Строгий JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            return Some(t.to_string());
        }
        if let Some(arr) = v.get("segments").and_then(|x| x.as_array()) {
            let mut s = String::new();
            for seg in arr {
                if let Some(t) = seg.get("text").and_then(|x| x.as_str()) {
                    s.push_str(t); s.push(' ');
                }
            }
            if !s.is_empty() { return Some(s); }
        }
    }
    // 2) JSONL
    let mut acc = String::new();
    for line in trimmed.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
                acc.push_str(t); acc.push(' ');
            }
        }
    }
    if !acc.is_empty() { return Some(acc); }
    // 3) Plain text
    Some(trimmed.to_string())
}

/// Минимальный shell-сплиттер с поддержкой кавычек.
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
