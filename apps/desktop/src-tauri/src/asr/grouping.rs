//! Группировка per-token timestamps из sherpa-onnx в сегменты-предложения.
//!
//! sherpa-onnx возвращает `OfflineRecognizerResult.tokens: Vec<String>` и
//! `timestamps: Option<Vec<f32>>` + `durations: Option<Vec<f32>>` — по
//! одному значению на токен. Мы склеиваем токены в сегменты по правилу:
//! закрываем текущий сегмент на границе `.`/`!`/`?`/`,` либо при накоплении
//! `MAX_TOKENS_PER_SEGMENT` токенов. Это даёт SRT-блоки разумной длины
//! без сложного sentence-splitter'а.

use super::TimedSegment;

const MAX_TOKENS_PER_SEGMENT: usize = 12;

pub fn group_into_segments(
    tokens: &[String],
    timestamps: Option<&[f32]>,
    durations: Option<&[f32]>,
    chunk_offset_sec: f32,
    chunk_duration_sec: f32,
) -> Vec<TimedSegment> {
    if tokens.is_empty() {
        return Vec::new();
    }

    // Если timestamps отсутствуют (CTC, не transducer) — делим текст
    // равными кусками по ~MAX_TOKENS_PER_SEGMENT токенов, начиная с offset.
    let has_ts = timestamps.map(|t| t.len() == tokens.len()).unwrap_or(false);
    if !has_ts {
        return fallback_uniform(tokens, chunk_offset_sec, chunk_duration_sec);
    }

    let ts = timestamps.unwrap();
    let dur = durations.unwrap_or(&[]);

    let mut out = Vec::new();
    let mut buf_tokens: Vec<&str> = Vec::new();
    let mut seg_start: Option<f32> = None;
    let mut seg_end: f32 = chunk_offset_sec;

    for (i, tok) in tokens.iter().enumerate() {
        let local_start = ts.get(i).copied().unwrap_or(0.0);
        let local_end = ts
            .get(i)
            .and_then(|_| dur.get(i).copied())
            .map(|d| local_start + d)
            .unwrap_or(local_start);

        if seg_start.is_none() {
            seg_start = Some(chunk_offset_sec + local_start);
        }
        seg_end = chunk_offset_sec + local_end;
        buf_tokens.push(tok.as_str());

        let is_punct_end = matches!(
            tok.chars().last(),
            Some('.') | Some('!') | Some('?')
        );
        let is_punct_mid = matches!(tok.chars().last(), Some(','));
        let is_long = buf_tokens.len() >= MAX_TOKENS_PER_SEGMENT;

        if is_punct_end || is_long || (is_punct_mid && buf_tokens.len() >= 6) {
            flush(&mut buf_tokens, &mut seg_start, seg_end, &mut out);
        }
    }
    if !buf_tokens.is_empty() {
        flush(&mut buf_tokens, &mut seg_start, seg_end, &mut out);
    }
    out
}

fn flush(
    buf: &mut Vec<&str>,
    seg_start: &mut Option<f32>,
    seg_end: f32,
    out: &mut Vec<TimedSegment>,
) {
    if buf.is_empty() {
        return;
    }
    let text = buf.join(" ").trim().to_string();
    if !text.is_empty() {
        out.push(TimedSegment {
            start_sec: seg_start.unwrap_or(0.0),
            end_sec: seg_end,
            text,
        });
    }
    buf.clear();
    *seg_start = None;
}

/// Без timestamps — равномерно делим длительность чанка между токенами,
/// потом группируем по MAX_TOKENS_PER_SEGMENT.
fn fallback_uniform(
    tokens: &[String],
    chunk_offset_sec: f32,
    chunk_duration_sec: f32,
) -> Vec<TimedSegment> {
    if chunk_duration_sec <= 0.0 {
        return vec![TimedSegment {
            start_sec: chunk_offset_sec,
            end_sec: chunk_offset_sec,
            text: tokens.join(" "),
        }];
    }
    let per_token = chunk_duration_sec / tokens.len() as f32;
    let mut out = Vec::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut seg_start: Option<f32> = None;
    let mut seg_end: f32 = chunk_offset_sec;

    for (i, tok) in tokens.iter().enumerate() {
        let s = chunk_offset_sec + per_token * i as f32;
        let e = chunk_offset_sec + per_token * (i + 1) as f32;
        if seg_start.is_none() {
            seg_start = Some(s);
        }
        seg_end = e;
        buf.push(tok.as_str());
        if buf.len() >= MAX_TOKENS_PER_SEGMENT {
            flush(&mut buf, &mut seg_start, seg_end, &mut out);
        }
    }
    if !buf.is_empty() {
        flush(&mut buf, &mut seg_start, seg_end, &mut out);
    }
    out
}
