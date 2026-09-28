//! Explicit Authorization header values using the RFC 6750 token alphabet.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i-u:authorization)(?:"[ \t\r\n]*:[ \t\r\n]*"|[ \t]*:[ \t]*)(?i-u:bearer) +"#)
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
        let value = value.trim_end_matches([' ', '\t']);
        let unpadded = value.trim_end_matches('=');
        if unpadded.is_empty()
            || !unpadded.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
            })
        {
            continue;
        }
        candidates.push(Candidate {
            label: Label::BearerToken,
            start_byte: header.end(),
            end_byte: header.end() + value.len(),
        });
    }
}
