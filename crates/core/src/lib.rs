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
    detect_ssn(text, &mut candidates);
    detect_credit_card(text, &mut candidates);
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

fn digits_value(digits: &[u8]) -> u16 {
    digits
        .iter()
        .fold(0, |value, digit| value * 10 + u16::from(*digit - b'0'))
}

fn digits_value_u32(digits: &[u8]) -> u32 {
    digits
        .iter()
        .fold(0, |value, digit| value * 10 + u32::from(*digit - b'0'))
}

fn is_valid_ssn(area: u16, group: u16, serial: u16) -> bool {
    area != 0 && area != 666 && area < 900 && group != 0 && serial != 0
}

fn ssn_parts_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u16, u16)> {
    if start + 11 <= bytes.len()
        && bytes[start + 3] == b'-'
        && bytes[start + 6] == b'-'
        && bytes[start..start + 3]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && bytes[start + 4..start + 6]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && bytes[start + 7..start + 11]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some((
            start + 11,
            digits_value(&bytes[start..start + 3]),
            digits_value(&bytes[start + 4..start + 6]),
            digits_value(&bytes[start + 7..start + 11]),
        ));
    }

    if start + 9 <= bytes.len()
        && bytes[start..start + 9]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some((
            start + 9,
            digits_value(&bytes[start..start + 3]),
            digits_value(&bytes[start + 3..start + 5]),
            digits_value(&bytes[start + 5..start + 9]),
        ));
    }

    None
}

fn detect_ssn(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some((end, area, group, serial)) = ssn_parts_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        if is_valid_ssn(area, group, serial) {
            candidates.push(Candidate {
                label: Label::Ssn,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
    }
}

fn passes_luhn(digits: &[u8]) -> bool {
    let mut sum = 0_u32;

    for (position, digit) in digits.iter().rev().enumerate() {
        let mut value = u32::from(*digit - b'0');

        if position % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }

        sum += value;
    }

    sum % 10 == 0
}

fn has_supported_card_prefix(digits: &[u8]) -> bool {
    match digits {
        [b'4', ..] => true,                                           // Visa
        [b'3', b'4' | b'7', ..] => true,                              // American Express
        [b'5', second, ..] if (b'1'..=b'5').contains(second) => true, // Mastercard
        [b'2', _, _, _, ..] => (2221..=2720).contains(&digits_value(&digits[..4])),
        [b'6', b'0', b'1', b'1', ..] => true, // Discover 6011
        [b'6', b'5', ..] => true,             // Discover 65
        [b'6', b'4', third, ..] if (b'4'..=b'9').contains(third) => true, // Discover 644–649
        [b'6', b'2', _, _, _, _, ..] => (622126..=622925).contains(&digits_value_u32(&digits[..6])),
        _ => false,
    }
}

fn card_parts_at(bytes: &[u8], start: usize) -> (usize, Vec<u8>) {
    let mut end = start;
    let mut last_digit_end = start;
    let mut digits = Vec::with_capacity(19);
    let mut previous_was_separator = false;

    while end < bytes.len() {
        let byte = bytes[end];

        if byte.is_ascii_digit() {
            if digits.len() == 19 {
                break;
            }

            digits.push(byte);
            end += 1;
            last_digit_end = end;
            previous_was_separator = false;
        } else if (byte == b' ' || byte == b'-') && !previous_was_separator {
            end += 1;
            previous_was_separator = true;
        } else {
            break;
        }
    }

    (last_digit_end, digits)
}

fn detect_credit_card(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let (end, digits) = card_parts_at(bytes, start);

        if end == start || (end < bytes.len() && bytes[end].is_ascii_alphanumeric()) {
            start += 1;
            continue;
        }

        if (13..=19).contains(&digits.len())
            && has_supported_card_prefix(&digits)
            && passes_luhn(&digits)
        {
            candidates.push(Candidate {
                label: Label::CreditCard,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
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

    #[test]
    fn detects_dashed_ssn() {
        assert_eq!(
            scan("SSN: 123-45-6789"),
            vec![Entity {
                label: "SSN".to_owned(),
                text: "123-45-6789".to_owned(),
                start: 5,
                end: 16,
            }]
        );
    }

    #[test]
    fn detects_undashed_ssn() {
        assert_eq!(scan("123456789")[0].label, "SSN");
    }

    #[test]
    fn rejects_invalid_or_embedded_ssn_candidates() {
        assert!(scan("000-12-3456").is_empty());
        assert!(scan("666-12-3456").is_empty());
        assert!(scan("900-12-3456").is_empty());
        assert!(scan("123-00-4567").is_empty());
        assert!(scan("123-45-0000").is_empty());
        assert!(scan("ref123-45-6789x").is_empty());
    }

    #[test]
    fn detects_formatted_credit_card() {
        assert_eq!(
            scan("Card: 4111-1111-1111-1111"),
            vec![Entity {
                label: "CREDIT_CARD".to_owned(),
                text: "4111-1111-1111-1111".to_owned(),
                start: 6,
                end: 25,
            }]
        );
    }

    #[test]
    fn detects_supported_card_issuers() {
        assert_eq!(scan("5555 5555 5555 4444")[0].label, "CREDIT_CARD");
        assert_eq!(scan("378282246310005")[0].label, "CREDIT_CARD");
        assert_eq!(scan("6011111111111117")[0].label, "CREDIT_CARD");
    }

    #[test]
    fn rejects_invalid_credit_card_candidates() {
        assert!(scan("4111-1111-1111-1112").is_empty()); // bad Luhn checksum
        assert!(scan("1234-5678-9012-3452").is_empty()); // unsupported prefix
        assert!(scan("x4111-1111-1111-1111y").is_empty()); // embedded
    }
}
