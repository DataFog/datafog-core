//! Core PII scanning API for the DataFog Rust proof of concept.
use regex::Regex;
use std::sync::LazyLock;
/// A PII entity detected in an input string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    /// The detected PII label, such as `EMAIL` or `SSN`.
    pub label: String,
    /// The exact matched substring from the input text.
    pub text: String,
    /// Zero-based Unicode code-point offset where the entity starts.
    pub start: usize,
    /// Exclusive Unicode code-point offset where the entity ends.
    pub end: usize,
}

/// private enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Label {
    Email,
    Phone,
    Ssn,
    CreditCard,
    IpAddress,
    Date,
    ZipCode,
}

/// define a helper function for Label
impl Label {
    fn as_str(self) -> &'static str {
        match self {
            Label::Email => "EMAIL",
            Label::Phone => "PHONE",
            Label::Ssn => "SSN",
            Label::CreditCard => "CREDIT_CARD",
            Label::IpAddress => "IP_ADDRESS",
            Label::Date => "DATE",
            Label::ZipCode => "ZIP_CODE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    label: Label,
    start_byte: usize,
    end_byte: usize,
}

/// Scan text for supported PII entities.
///
pub fn scan(text: &str) -> Vec<Entity> {
    let mut candidates: Vec<Candidate> = Vec::new();
    detect_email(text, &mut candidates);
    detect_phone(text, &mut candidates);
    finalize(text, candidates)
}

fn finalize(text: &str, mut candidates: Vec<Candidate>) -> Vec<Entity> {
    candidates.sort_by(|left, right| {
        left.start_byte
            .cmp(&right.start_byte)
            .then_with(|| right.end_byte.cmp(&left.end_byte))
            .then_with(|| left.label.cmp(&right.label))
    });

    candidates.dedup_by(|left, right| {
        left.label == right.label
            && left.start_byte == right.start_byte
            && left.end_byte == right.end_byte
    });

    let is_ascii = text.is_ascii();

    candidates
        .into_iter()
        .map(|candidate| {
            debug_assert!(text.is_char_boundary(candidate.start_byte));
            debug_assert!(text.is_char_boundary(candidate.end_byte));

            let start = if is_ascii {
                candidate.start_byte
            } else {
                code_point_offset(text, candidate.start_byte)
            };

            let end = if is_ascii {
                candidate.end_byte
            } else {
                code_point_offset(text, candidate.end_byte)
            };

            Entity {
                label: candidate.label.as_str().to_owned(),
                text: text[candidate.start_byte..candidate.end_byte].to_owned(),
                start,
                end,
            }
        })
        .collect()
}

fn code_point_offset(text: &str, byte_offset: usize) -> usize {
    text[..byte_offset].chars().count()
}
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?:
            (?:\+?1[\s.-]?)?
            (?:\(\d{3}\)|\d{3})
            [\s.-]?\d{3}[\s.-]?\d{4}
          |
            \+\d(?:[\s.-]?\d){6,14}
        )",
    )
    .expect("phone regex is valid")
});

fn has_phone_boundaries(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();

    let before_is_alphanumeric = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
    let after_is_alphanumeric = end < bytes.len() && bytes[end].is_ascii_alphanumeric();

    !before_is_alphanumeric && !after_is_alphanumeric
}

fn is_valid_phone(candidate: &str) -> bool {
    let digits: String = candidate
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .map(char::from)
        .collect();

    if candidate.starts_with('+') {
        (7..=15).contains(&digits.len())
    } else {
        digits.len() == 10 || (digits.len() == 11 && digits.starts_with('1'))
    }
}

fn detect_phone(text: &str, candidates: &mut Vec<Candidate>) {
    for matched in PHONE_RE.find_iter(text) {
        if !has_phone_boundaries(text, matched.start(), matched.end()) {
            continue;
        }

        if !is_valid_phone(matched.as_str()) {
            continue;
        }

        candidates.push(Candidate {
            label: Label::Phone,
            start_byte: matched.start(),
            end_byte: matched.end(),
        });
    }
}

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[A-Z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[A-Z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?\.)+[A-Z]{2,}",
    )
    .expect("email regex is valid")
});

fn detect_email(text: &str, candidates: &mut Vec<Candidate>) {
    for matched in EMAIL_RE.find_iter(text) {
        candidates.push(Candidate {
            label: Label::Email,
            start_byte: matched.start(),
            end_byte: matched.end(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Entity, scan};

    #[test]
    fn empty_input_has_no_entities() {
        assert!(scan("").is_empty());
    }

    #[test]
    fn detects_email_without_trailing_punctuation() {
        assert_eq!(
            scan("Contact jane@example.com."),
            vec![Entity {
                label: "EMAIL".to_owned(),
                text: "jane@example.com".to_owned(),
                start: 8,
                end: 24,
            }]
        );
    }

    #[test]
    fn reports_email_offsets_as_unicode_code_points() {
        assert_eq!(
            scan("👋 jane@example.com"),
            vec![Entity {
                label: "EMAIL".to_owned(),
                text: "jane@example.com".to_owned(),
                start: 2,
                end: 18,
            }]
        );
    }

    #[test]
    fn detects_north_american_phone_number() {
        assert_eq!(
            scan("Call (212) 555-0100."),
            vec![Entity {
                label: "PHONE".to_owned(),
                text: "(212) 555-0100".to_owned(),
                start: 5,
                end: 19,
            }]
        );
    }

    #[test]
    fn detects_international_phone_number() {
        assert_eq!(
            scan("Intl +44 20 7946 0958"),
            vec![Entity {
                label: "PHONE".to_owned(),
                text: "+44 20 7946 0958".to_owned(),
                start: 5,
                end: 21,
            }]
        );
    }

    #[test]
    fn rejects_short_or_embedded_phone_candidates() {
        assert!(scan("Call 555-0100").is_empty());
        assert!(scan("order212-555-0100x").is_empty());
    }
}
