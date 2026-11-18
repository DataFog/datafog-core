//! Delimit original source slices before applying the shared email matcher.
use std::sync::LazyLock;

use regex::Regex;

use crate::{
    Candidate, EMAIL_CONTINUATION_RE, ScanFormat, detect_email, detect_email_with_context,
};

pub(super) fn detect(text: &str, format: ScanFormat, candidates: &mut Vec<Candidate>) {
    match format {
        ScanFormat::Text => detect_email_with_context(text, candidates, true),
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
pub(super) fn text_candidate_start(text: &str, mut start: usize, end: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let at = text[start..end].find('@').map(|offset| start + offset)?;
    if matches!(bytes.get(start), Some(b'\'' | b'"')) && bytes.get(end) == Some(&bytes[start]) {
        start += 1;
    }
    // Field delimiters and markup are syntax rather than mailbox identity.
    if let Some(delimiter) = text[start..at].rfind(['&', '?', ';', '|']) {
        start += delimiter + 1;
    }
    // A local part cannot start with a dot, so one left in front by the
    // trimming above or below is skipped rather than rejecting the address.
    while start < at && matches!(bytes[start], b'*' | b'{' | b'/' | b'|' | b'.') {
        start += 1;
    }
    let mut start = label_value_start(text, start, at).unwrap_or(start);
    while start < at && bytes[start] == b'.' {
        start += 1;
    }
    (start < at).then_some(start)
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

/// A letter or digit outside ASCII and the Latin script. Combining marks take
/// the script of their base and are classified separately.
static OTHER_SCRIPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\x00-\x7F\p{Latin}\p{M}]").expect("script regex is valid"));

static MARK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{M}").expect("mark regex is valid"));

fn is_other_script(character: char) -> bool {
    OTHER_SCRIPT_RE.is_match(character.encode_utf8(&mut [0; 4]))
}

fn is_mark(character: char) -> bool {
    MARK_RE.is_match(character.encode_utf8(&mut [0; 4]))
}

/// Narrow a match from text containing non-ASCII letters to the address.
///
/// The matcher accepts every letter, so in text written without spaces it runs
/// an address into the prose and the values around it: `alice@example.comとbob`
/// takes the local part of the next address as a top-level domain, and
/// `SSNは123-45-6789でメールはalice@example.com` takes the sentence as a
/// local part. Internationalized addresses stay whole: a part is only split
/// where an ASCII or accented-Latin address meets another script, or where the
/// end of a domain is the beginning of another address.
///
/// The returned end can lie past `end` when two addresses share letters that
/// cannot be assigned to either; see [`domain_end`].
pub(super) fn script_bounds(text: &str, start: usize, end: usize) -> (usize, usize) {
    let Some(at) = text[start..end].find('@').map(|offset| start + offset) else {
        return (start, end);
    };
    let domain_end = domain_end(text, at + 1, end);
    (
        local_start(text, start, at, domain_end.min(end)),
        domain_end,
    )
}

/// End of the domain that starts at `domain_start`; `end` is the matcher's.
///
/// The domain stops at its first non-ASCII character when the labels before it
/// are a complete ASCII or accented-Latin domain and either another address
/// begins there (`alice@example.comébob@example.net`) or the character starts
/// a different script in the last label (`alice@example.comです`): no
/// top-level domain continues from Latin letters into another script.
///
/// A domain that runs straight into the `@` of another address without such a
/// point gives up its trailing ASCII or Latin letters to that address. When it
/// has none, the two addresses cannot be told apart and are reported as one
/// span, so the second domain is never left out.
fn domain_end(text: &str, domain_start: usize, end: usize) -> usize {
    let domain = &text[domain_start..end];
    let mut address_follows = None;
    let mut address_follows =
        || *address_follows.get_or_insert_with(|| EMAIL_CONTINUATION_RE.is_match(&text[end..]));
    let mut has_dot = false;
    let mut label_characters = 0;
    let mut label_is_alphabetic = true;
    for (offset, character) in domain.char_indices() {
        if !character.is_ascii() {
            let other_script = is_other_script(character);
            if has_dot
                && label_characters >= 2
                && label_is_alphabetic
                && ((other_script && !domain[offset..].contains('.')) || address_follows())
            {
                return domain_start + offset;
            }
            if other_script {
                // Past this point the domain itself is internationalized and
                // has no script boundary to split at.
                break;
            }
        }
        if character == '.' {
            has_dot = true;
            label_characters = 0;
            label_is_alphabetic = true;
        } else {
            label_characters += 1;
            label_is_alphabetic &= character != '-' && !character.is_numeric();
        }
    }
    if !text[end..].starts_with('@') {
        return end;
    }
    let next_local = OTHER_SCRIPT_RE
        .find_iter(domain)
        .last()
        .map(|other_script| other_script.end())
        .or_else(|| {
            domain
                .char_indices()
                .rev()
                .find(|(_, character)| !character.is_ascii())
                .map(|(offset, character)| offset + character.len_utf8())
        })
        .map(|tail| {
            tail + domain[tail..]
                .chars()
                .take_while(|character| !character.is_ascii() && is_mark(*character))
                .map(char::len_utf8)
                .sum::<usize>()
        });
    if let Some(next_local) = next_local.filter(|next_local| *next_local < domain.len()) {
        return domain_start + next_local;
    }
    let mut end = end;
    while text[end..].starts_with('@') {
        match EMAIL_CONTINUATION_RE.find(&text[end..]) {
            Some(continuation) => end += continuation.end(),
            None => break,
        }
    }
    end
}

/// Start of the local part that ends at `at`, at or after `start`.
///
/// In front of an ASCII or accented-Latin domain, letters of another script
/// are surrounding text when ASCII or Latin characters follow them, so the
/// local part starts after the last such letter: `メールはalice@example.com`
/// reports `alice@example.com`. A local part that ends in another script
/// (`用户@example.com`) and any local part of an internationalized domain
/// (`用户@例子.广告`) are kept whole, because nothing separates such a mailbox
/// name from text written before it.
fn local_start(text: &str, start: usize, at: usize, end: usize) -> usize {
    if OTHER_SCRIPT_RE.is_match(&text[at + 1..end]) {
        return start;
    }
    let Some(other_script) = OTHER_SCRIPT_RE.find_iter(&text[start..at]).last() else {
        return start;
    };
    let mut local = start + other_script.end();
    for character in text[local..at].chars() {
        if character != '.' && (character.is_ascii() || !is_mark(character)) {
            break;
        }
        local += character.len_utf8();
    }
    if local < at { local } else { start }
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
