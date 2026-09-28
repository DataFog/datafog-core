//! Context-required lexical detection; assignment and checksums are not validated.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

static DETECTOR: LazyLock<Regex> = LazyLock::new(|| {
    let horizontal = r"(?u:[ \t\x{00a0}\x{202f}])";
    let context = r"(?:NPI|NationalH+ProviderH+Identifier)".replace('H', horizontal);
    Regex::new(&format!(
        r"(?i-u:{context}{horizontal}*[:#-]?{horizontal}*(?P<value>[0-9]{{10}}))"
    ))
    .expect("static numeric identifier regex must compile")
});

fn ascii_boundary(text: &str, start: usize, end: usize) -> bool {
    !start
        .checked_sub(1)
        .is_some_and(|index| text.as_bytes()[index].is_ascii_alphanumeric())
        && !text
            .as_bytes()
            .get(end)
            .is_some_and(u8::is_ascii_alphanumeric)
}

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    for captures in DETECTOR.captures_iter(text) {
        // Both captures are unconditional in the statically compiled expressions.
        let whole = captures.get(0).expect("whole regex match");
        let value = captures.name("value").expect("numeric identifier capture");
        if ascii_boundary(text, whole.start(), whole.end())
            && ascii_boundary(text, value.start(), value.end())
        {
            candidates.push(Candidate {
                label: Label::Npi,
                start_byte: value.start(),
                end_byte: value.end(),
            });
        }
    }
}
