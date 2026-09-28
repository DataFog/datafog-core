//! Locale-gated lexical detection, without official identifier validation.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

pub(super) fn labels() -> Vec<Label> {
    DEFINITIONS.iter().map(|(label, _, _)| *label).collect()
}

const DEFINITIONS: &[(Label, &str, &str)] = &[
    (Label::DeIban, "", r"DE[0-9]{2}(?:H?[0-9]{4}){4}H?[0-9]{2}"),
    (Label::DeVatId, "", r"DE(?:H|-)?[0-9]{9}"),
    (
        Label::DeTaxId,
        r"(?:Steuer(?:H|-)?ID|Steueridentifikationsnummer|Identifikationsnummer|IdNr\.?|Tax(?:H|-)?ID)",
        r"[0-9]{2}H?[0-9]{3}H?[0-9]{3}H?[0-9]{3}",
    ),
    (
        Label::DeSocialSecurityNumber,
        r"(?:Rentenversicherungsnummer|Sozialversicherungsnummer|RVNR|SVNR)",
        r"[0-9]{2}H?[0-9]{6}H?[A-Za-z]H?[0-9]{3}",
    ),
    (
        Label::DePostalCode,
        "",
        r"(?:PLZ(?:H|:|-)?|DE(?:H|-)|D(?:H|-))[0-9]{5}",
    ),
    (
        Label::DePassportNumber,
        r"(?:Passnummer|Reisepass(?:nummer)?|Passport(?:H+No\.?|H+Number)?)",
        r"[A-Za-z][0-9]{8}",
    ),
    (
        Label::DeResidencePermitNumber,
        r"(?:Aufenthaltstitel|Aufenthaltserlaubnis|ResidenceH+Permit|eAT)",
        r"AT[0-9]{7}",
    ),
];

// ASCII case folding is intentional. H excludes newlines and other Unicode whitespace.
static DETECTORS: LazyLock<Vec<(Label, Regex)>> = LazyLock::new(|| {
    let horizontal = r"(?u:[ \t\x{00a0}\x{202f}])";
    let gap = format!("{horizontal}*[:#-]?{horizontal}*");
    DEFINITIONS
        .iter()
        .copied()
        .map(|(label, context, value)| {
            let context = context.replace('H', horizontal);
            let value = value.replace('H', horizontal);
            let pattern = if context.is_empty() {
                format!("(?i-u:(?P<value>{value}))")
            } else {
                format!("(?i-u:{context}{gap}(?P<value>{value}))")
            };
            (
                label,
                Regex::new(&pattern).expect("static German detector regex must compile"),
            )
        })
        .collect()
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
    for (label, regex) in DETECTORS.iter() {
        for captures in regex.captures_iter(text) {
            // Both captures are unconditional in the statically compiled expressions.
            let whole = captures.get(0).expect("whole regex match");
            let value = captures.name("value").expect("German value capture");
            if ascii_boundary(text, whole.start(), whole.end())
                && ascii_boundary(text, value.start(), value.end())
            {
                candidates.push(Candidate {
                    label: *label,
                    start_byte: value.start(),
                    end_byte: value.end(),
                });
            }
        }
    }
}
