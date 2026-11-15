//! Delimit original source slices before applying the shared email matcher.
use crate::{Candidate, ScanFormat, detect_email};

pub(super) fn detect(text: &str, format: ScanFormat, candidates: &mut Vec<Candidate>) {
    match format {
        ScanFormat::Text => {
            let first = candidates.len();
            detect_email(text, candidates);
            refine_text_candidates(text, candidates, first);
        }
        ScanFormat::Env => detect_env(text, candidates),
        ScanFormat::Sql => detect_sql(text, candidates),
    }
}

/// Text mode applies the raw RFC local-part alphabet, which also admits the
/// punctuation that label-style syntaxes put in front of a value, so a match
/// can start inside `label=` or swallow an opening quote. Shrink each
/// candidate the way the env and sql formats already delimit their slices:
/// drop a wrapping leading quote, then a leading identifier-prefixed `=`.
/// Inner `=` stays part of the local part, matching the reviewed env
/// fixture semantics. A quote without its closing partner is a real local-part
/// character and is kept.
fn refine_text_candidates(text: &str, candidates: &mut [Candidate], first: usize) {
    for candidate in &mut candidates[first..] {
        let mut start = candidate.start_byte;
        let end = candidate.end_byte;
        let bytes = text.as_bytes();
        let Some(at) = text[start..end].find('@').map(|offset| start + offset) else {
            continue;
        };
        if matches!(bytes.get(start), Some(b'\'' | b'"')) && bytes.get(end) == Some(&bytes[start]) {
            start += 1;
        }
        // Field delimiters and markup are syntax rather than mailbox identity.
        if let Some(delimiter) = text[start..at].rfind(['&', '?', ';', '|']) {
            start += delimiter + 1;
        }
        while start < at && matches!(bytes[start], b'*' | b'{' | b'/' | b'|') {
            start += 1;
        }
        if let Some(value_start) = label_value_start(text, start, at) {
            start = value_start;
        }
        if start != candidate.start_byte {
            candidate.start_byte = start;
        }
    }
}

/// End of the `identifier=` prefix when the candidate's local part starts with
/// one, or None. The identifier follows the env assignment rules (alphabetic
/// or `_` first, then letters, digits, `_` and `-`), and at least one
/// local-part character must remain before the `@`.
fn label_value_start(text: &str, start: usize, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut cursor = start;
    match bytes.get(cursor) {
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => cursor += 1,
        _ => return None,
    }
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-' || *byte == b'.')
    {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'=') && cursor + 1 < at {
        Some(cursor + 1)
    } else {
        None
    }
}

fn detect_slice(text: &str, start: usize, end: usize, candidates: &mut Vec<Candidate>) {
    let first = candidates.len();
    detect_email(&text[start..end], candidates);
    for candidate in &mut candidates[first..] {
        candidate.start_byte += start;
        candidate.end_byte += start;
    }
}

fn line_end(text: &str, start: usize) -> usize {
    text[start..]
        .find('\n')
        .map_or(text.len(), |end| start + end)
}

fn skip_horizontal_space(bytes: &[u8], mut cursor: usize) -> usize {
    while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\r')) {
        cursor += 1;
    }
    cursor
}

fn env_value_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut cursor = skip_horizontal_space(bytes, 0);
    if line[cursor..].starts_with("export") && matches!(bytes.get(cursor + 6), Some(b' ' | b'\t')) {
        cursor = skip_horizontal_space(bytes, cursor + 6);
    }
    if !bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return None;
    }
    cursor += 1;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        cursor += 1;
    }
    cursor = skip_horizontal_space(bytes, cursor);
    if bytes.get(cursor) != Some(&b'=') {
        return None;
    }
    Some(skip_horizontal_space(bytes, cursor + 1))
}

// Return the closing delimiter or EOF. No values are decoded, so offsets and
// escaped local parts remain contiguous spans of the caller's source text.
fn quote_end(bytes: &[u8], opening: usize, doubled: bool, backslash: bool) -> usize {
    let quote = bytes[opening];
    let mut cursor = opening + 1;
    while cursor < bytes.len() {
        if backslash && bytes[cursor] == b'\\' {
            cursor += if cursor + 1 < bytes.len() { 2 } else { 1 };
        } else if bytes[cursor] == quote {
            if doubled && bytes.get(cursor + 1) == Some(&quote) {
                cursor += 2;
            } else {
                return cursor;
            }
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

fn detect_env(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < text.len() {
        let end = line_end(text, cursor);
        if let Some(value) = env_value_start(&text[cursor..end]) {
            let start = cursor + value;
            if matches!(bytes.get(start), Some(b'\'' | b'"')) {
                let close = quote_end(bytes, start, false, bytes[start] == b'"');
                detect_slice(text, start + 1, close, candidates);
                if close == text.len() {
                    break;
                }
                let tail_end = line_end(text, close + 1);
                detect_env_unquoted(text, close + 1, tail_end, candidates);
                cursor = tail_end;
            } else {
                detect_env_unquoted(text, start, end, candidates);
                cursor = end;
            }
        } else {
            detect_env_unquoted(text, cursor, end, candidates);
            cursor = end;
        }
        if cursor < text.len() {
            cursor += 1;
        }
    }
}

fn detect_env_unquoted(text: &str, start: usize, end: usize, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    for cursor in start..end {
        if bytes[cursor] == b'#' && (cursor == start || bytes[cursor - 1].is_ascii_whitespace()) {
            detect_slice(text, start, cursor, candidates);
            detect_slice(text, cursor + 1, end, candidates);
            return;
        }
    }
    detect_slice(text, start, end, candidates);
}

fn detect_sql(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut plain_start = 0;
    while cursor < bytes.len() {
        let end = if matches!(bytes[cursor], b'\'' | b'"') {
            detect_slice(text, plain_start, cursor, candidates);
            let close = quote_end(bytes, cursor, true, false);
            detect_slice(text, cursor + 1, close, candidates);
            if close < bytes.len() {
                close + 1
            } else {
                close
            }
        } else if bytes[cursor..].starts_with(b"--") {
            detect_slice(text, plain_start, cursor, candidates);
            let end = line_end(text, cursor);
            detect_slice(text, cursor + 2, end, candidates);
            end
        } else if bytes[cursor..].starts_with(b"/*") {
            detect_slice(text, plain_start, cursor, candidates);
            let mut end = cursor + 2;
            let mut comment_start = end;
            let mut depth = 1;
            while end < bytes.len() && depth > 0 {
                if bytes[end..].starts_with(b"/*") {
                    detect_slice(text, comment_start, end, candidates);
                    depth += 1;
                    end += 2;
                    comment_start = end;
                } else if bytes[end..].starts_with(b"*/") {
                    detect_slice(text, comment_start, end, candidates);
                    depth -= 1;
                    end += 2;
                    comment_start = end;
                } else {
                    end += 1;
                }
            }
            detect_slice(text, comment_start, end, candidates);
            end
        } else {
            cursor += 1;
            continue;
        };
        cursor = end;
        plain_start = end;
    }
    detect_slice(text, plain_start, text.len(), candidates);
}
