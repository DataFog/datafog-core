//! Labelled, number-first English street lines; models retain broader address coverage.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

static DETECTOR: LazyLock<Regex> = LazyLock::new(|| {
    let horizontal = r"(?u:[ \t\x{00a0}\x{202f}])";
    let word = r"\p{L}[\p{L}\p{M}]*(?:['’\-]\p{L}[\p{L}\p{M}]*)*";
    let suffix = r"(?i-u:street|road|avenue|lane|drive|court|terrace|way|close|place|crescent|st\.?|rd\.?|ave\.?|ln\.?|dr\.?|ct\.?|ter\.?|pl\.?|cres\.?)";
    let direction = r"(?i-u:NE|NW|SE|SW|N|S|E|W)";
    let unit = r"(?i-u:apt|suite|unit|flat|floor)";
    Regex::new(&format!(
        r"\b(?i-u:(?:street|shipping|billing){horizontal}+address:){horizontal}*(?P<value>[0-9]{{1,6}}[A-Za-z]?{horizontal}+{word}(?:{horizontal}+{word}){{0,4}}{horizontal}+{suffix}(?:{horizontal}+{direction})?(?:(?:{horizontal}*,{horizontal}*|{horizontal}+){unit}{horizontal}+[A-Za-z0-9]{{1,8}})?)"
    ))
    .expect("static labelled street-address regex must compile")
});

fn horizontal(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\u{00a0}' | '\u{202f}')
}

fn unit_continuation(tail: &str) -> bool {
    let word = tail
        .trim_start_matches(char::is_whitespace)
        .split(|character: char| !character.is_ascii_alphabetic())
        .next()
        .unwrap_or_default();
    // Reject malformed, unsupported or additional units instead of protecting
    // just the street prefix and leaving a unit identifier visible.
    [
        "apt",
        "apartment",
        "suite",
        "ste",
        "unit",
        "flat",
        "floor",
        "room",
        "building",
        "level",
    ]
    .iter()
    .any(|marker| word.eq_ignore_ascii_case(marker))
}

fn terminated(tail: &str) -> bool {
    let tail = tail.trim_start_matches(horizontal);
    match tail.chars().next() {
        None | Some(';') => true,
        Some('\r' | '\n') => !unit_continuation(tail),
        Some(',') => !unit_continuation(&tail[1..]),
        Some('.' | '!' | '?') => tail[1..].chars().next().is_none_or(char::is_whitespace),
        _ => false,
    }
}

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    for captures in DETECTOR.captures_iter(text) {
        let Some(value) = captures.name("value") else {
            continue;
        };
        if terminated(&text[value.end()..]) {
            candidates.push(Candidate {
                label: Label::StreetAddress,
                start_byte: value.start(),
                end_byte: value.end(),
            });
        }
    }
}
