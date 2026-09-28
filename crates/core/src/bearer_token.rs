//! Explicit Authorization header values using the RFC 6750 token alphabet.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i-u:authorization)(?:"[ \t\r\n]*:[ \t\r\n]*"|[ \t]*:[ \t]*)"#)
        .expect("static Authorization header regex")
});

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let mut line_start = 0;
    let mut inspected = 0;
    for header in HEADER.find_iter(text) {
        // Advance once through separators even when many malformed headers share a line.
        for (offset, _) in text[inspected..header.start()].match_indices(['\r', '\n']) {
            line_start = inspected + offset + 1;
        }
        inspected = header.start();
        let before = &text[..header.start()];
        let quoted = header.as_str().contains('"');
        if quoted {
            if !before.ends_with('"') {
                continue;
            }
            let key_prefix = before[..before.len() - 1].trim_end_matches([' ', '\t', '\r', '\n']);
            if !key_prefix.is_empty() && !key_prefix.ends_with(['{', ',']) {
                continue;
            }
        } else if !text[line_start..header.start()]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            continue;
        }
        let remaining = &text[header.end()..];
        let value = if quoted {
            let Some(end) = remaining.find('"') else {
                continue;
            };
            &remaining[..end]
        } else {
            remaining.split(['\r', '\n']).next().unwrap_or(remaining)
        };
        if let Some((start, end)) = token_range(value) {
            candidates.push(Candidate {
                label: Label::BearerToken,
                start_byte: header.end() + start,
                end_byte: header.end() + end,
            });
        }
    }
}

/// Reuse the header grammar when the immediate structured field supplies its name.
pub(super) fn detect_header_value(value: &str, candidates: &mut Vec<Candidate>) {
    if let Some((start_byte, end_byte)) = token_range(value) {
        candidates.push(Candidate {
            label: Label::BearerToken,
            start_byte,
            end_byte,
        });
    }
}

fn token_range(value: &str) -> Option<(usize, usize)> {
    let trimmed = value.trim_start_matches([' ', '\t']);
    if !trimmed.get(..6)?.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let after_scheme = &trimmed[6..];
    if !after_scheme.starts_with(' ') {
        return None;
    }
    let token = after_scheme.trim_start_matches(' ');
    let start = value.len() - token.len();
    let token = token.trim_end_matches([' ', '\t']);
    let unpadded = token.trim_end_matches('=');
    if unpadded.is_empty()
        || !unpadded.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
        })
    {
        return None;
    }
    Some((start, start + token.len()))
}
