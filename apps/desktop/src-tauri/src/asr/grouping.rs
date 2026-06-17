//! Группировка per-token timestamps из sherpa-onnx в сегменты-предложения.
//!
//! sherpa-onnx возвращает `OfflineRecognizerResult.tokens: Vec<String>` —
//! BPE-сабтокены (от GigaAM BPE). Каждый токен уже несёт встроенные пробелы
//! где нужно (модель эмитит « » как отдельный токен). Поэтому при склейке
//! сегмента токены соединяем БЕЗ разделителя: `buf.join("")` — иначе между
//! каждой буквой BPE появляется пробел («П ря мо» вместо «Прямо»).
//!
//! Сегмент закрывается на границе `.`/`!`/`?`/`/`, либо при накоплении
//! `MAX_TOKENS_PER_SEGMENT` токенов. Это даёт SRT-блоки разумной длины
//! без сложного sentence-splitter'а.

use super::TimedSegment;

const MAX_TOKENS_PER_SEGMENT: usize = 25;

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

        let is_punct_end = matches!(tok.chars().last(), Some('.') | Some('!') | Some('?'));
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
    // BPE-сабтокены соединяем без разделителя: пробелы уже внутри токенов
    // (напр. « », «▁word»). `join(" ")` ломает текст («П ря мо»).
    let text = buf.concat().trim().to_string();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(times: &[f32]) -> Vec<f32> {
        times.to_vec()
    }

    #[test]
    fn bpe_subwords_join_without_spaces() {
        // GigaAM BPE: каждый символ — отдельный токен, пробелы — отдельный токен.
        // Раньше join(" ") ломал «Прямо» в «П ря мо». Теперь join("") даёт «Прямо ».
        let tokens = vec![
            "П".to_string(),
            "р".to_string(),
            "я".to_string(),
            "м".to_string(),
            "о".to_string(),
            " ".to_string(),
            "ба".to_string(),
            "зов".to_string(),
            "ая".to_string(),
            ",".to_string(),
        ];
        let timestamps = ts(&[0.0, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8]);
        let durations = ts(&[0.05; 10]);
        let segs = group_into_segments(&tokens, Some(&timestamps), Some(&durations), 0.0, 1.0);
        assert!(!segs.is_empty(), "should produce at least 1 segment");
        let text = segs[0].text.clone();
        assert!(
            !text.contains("П ря мо"),
            "must not split subwords with spaces, got: {text:?}"
        );
        assert!(
            text.contains("Прямо") || text.contains("Прямо"),
            "should join subwords, got: {text:?}"
        );
        assert!(
            text.contains("базовая") || text.contains("базовая"),
            "should join subwords, got: {text:?}"
        );
    }

    #[test]
    fn splits_at_sentence_end() {
        let tokens = vec![
            "Привет".into(),
            " ".into(),
            "мир".into(),
            "!".into(),
            " ".into(),
            "Пока".into(),
        ];
        let timestamps = ts(&[0.0, 0.3, 0.5, 0.8, 0.9, 1.0]);
        let durations = ts(&[0.3, 0.2, 0.3, 0.1, 0.1, 0.2]);
        let segs = group_into_segments(&tokens, Some(&timestamps), Some(&durations), 0.0, 1.5);
        // После «!» — закрытие сегмента; «Пока» в новом.
        assert!(
            segs.len() >= 2,
            "expected split after '!', got {} segs",
            segs.len()
        );
        assert_eq!(segs[0].text, "Привет мир!");
        assert_eq!(segs[1].text, "Пока");
    }

    #[test]
    fn splits_at_max_tokens() {
        // 30 токенов без пунктуации — должно разбить по MAX_TOKENS=25.
        let tokens: Vec<String> = (0..30).map(|i| format!("t{i}")).collect();
        let timestamps: Vec<f32> = (0..30).map(|i| i as f32 * 0.05).collect();
        let durations = vec![0.05; 30];
        let segs = group_into_segments(&tokens, Some(&timestamps), Some(&durations), 0.0, 2.0);
        assert!(
            segs.len() >= 2,
            "30 tokens without punctuation should split, got {}",
            segs.len()
        );
    }
}
