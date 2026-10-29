//! A deliberately partial lexical rule; configured models retain account coverage.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

static DETECTOR: LazyLock<Regex> = LazyLock::new(|| {
    let horizontal = r"(?u:[ \t\x{00a0}\x{202f}])";
    Regex::new(&format!(
        r"\b(?i-u:(?:account|acct){horizontal}+(?:number|no\.)(?:{horizontal}+|{horizontal}*[:#]{horizontal}*))(?P<value>[0-9]{{6,17}}|(?:[Xx]{{4,12}}|\*{{4,12}})[0-9]{{4}})\b"
    ))
    .expect("static labelled account regex must compile")
});

static ENDING_DETECTOR: LazyLock<Regex> = LazyLock::new(|| {
    let horizontal = r"(?u:[ \t\x{00a0}\x{202f}])";
    Regex::new(&format!(
        r"\b(?i-u:(?:account|acct)(?:{horizontal}+(?:number|no\.))?{horizontal}+ending{horizontal}+in{horizontal}+)(?P<value>[0-9]{{4}})\b"
    ))
    .expect("static account-ending regex must compile")
});

fn horizontal(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\u{00a0}' | '\u{202f}')
}

// Never classify just the first group of a decimal, date or split numeric value.
fn continues_numeric_value(tail: &str) -> bool {
    let tail = tail.trim_start_matches(horizontal);
    if tail.starts_with(|character: char| character.is_numeric()) {
        return true;
    }
    tail.strip_prefix(['.', ',', '/', '-']).is_some_and(|rest| {
        rest.trim_start_matches(horizontal)
            .starts_with(|character: char| character.is_numeric())
    })
}

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    for detector in [&*DETECTOR, &*ENDING_DETECTOR] {
        for captures in detector.captures_iter(text) {
            let Some(value) = captures.name("value") else {
                continue;
            };
            if !continues_numeric_value(&text[value.end()..]) {
                candidates.push(Candidate {
                    label: Label::AccountNumber,
                    start_byte: value.start(),
                    end_byte: value.end(),
                });
            }
        }
    }
}
