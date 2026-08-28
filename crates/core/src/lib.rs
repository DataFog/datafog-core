//! Core PII scanning API for DataFog.
use regex::Regex;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

/// A zero-based, end-exclusive range in the coordinate system named by its
/// containing field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Inclusive start offset.
    pub start: usize,
    /// Exclusive end offset.
    pub end: usize,
}

/// A piece of potentially sensitive content detected in an input string.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Canonical PII type, such as `EMAIL` or `SSN`.
    pub entity_type: String,
    /// Exact substring matched in the original input.
    pub matched_text: String,
    /// Range in UTF-8 bytes.
    pub byte_range: TextRange,
    /// Range in Unicode code points.
    pub codepoint_range: TextRange,
    /// Detection confidence in `0.0..=1.0`, when the detector produces one.
    pub confidence: Option<f32>,
    /// Stable name of the detector that produced this finding.
    pub detector_name: String,
    /// Version of the detector implementation, when available.
    pub detector_version: Option<String>,
}

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

    fn detector_name(self) -> &'static str {
        match self {
            Label::Email => "datafog-core/email",
            Label::Phone => "datafog-core/phone",
            Label::Ssn => "datafog-core/ssn",
            Label::CreditCard => "datafog-core/credit-card",
            Label::IpAddress => "datafog-core/ip-address",
            Label::Date => "datafog-core/date",
            Label::ZipCode => "datafog-core/zip-code",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    label: Label,
    start_byte: usize,
    end_byte: usize,
}

/// Scan text for supported PII findings.
pub fn scan(text: &str) -> Vec<Finding> {
    let mut candidates: Vec<Candidate> = Vec::new();
    detect_email(text, &mut candidates);
    detect_phone(text, &mut candidates);
    detect_ssn(text, &mut candidates);
    detect_credit_card(text, &mut candidates);
    detect_date(text, &mut candidates);
    detect_zip_code(text, &mut candidates);
    detect_ip_address(text, &mut candidates);
    finalize(text, candidates)
}

fn finalize(text: &str, mut candidates: Vec<Candidate>) -> Vec<Finding> {
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

            Finding {
                entity_type: candidate.label.as_str().to_owned(),
                matched_text: text[candidate.start_byte..candidate.end_byte].to_owned(),
                byte_range: TextRange {
                    start: candidate.start_byte,
                    end: candidate.end_byte,
                },
                codepoint_range: TextRange { start, end },
                confidence: None,
                detector_name: candidate.label.detector_name().to_owned(),
                detector_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
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

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_valid_date(year: u16, month: u8, day: u8) -> bool {
    year >= 1000 && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

fn fixed_digits_at(bytes: &[u8], start: usize, width: usize) -> Option<u16> {
    let end = start.checked_add(width)?;

    if end <= bytes.len() && bytes[start..end].iter().all(|byte| byte.is_ascii_digit()) {
        Some(digits_value(&bytes[start..end]))
    } else {
        None
    }
}

fn numeric_date_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    if start + 10 <= bytes.len() && bytes[start + 4] == b'-' && bytes[start + 7] == b'-' {
        let year = fixed_digits_at(bytes, start, 4)?;
        let month = fixed_digits_at(bytes, start + 5, 2)? as u8;
        let day = fixed_digits_at(bytes, start + 8, 2)? as u8;

        return Some((start + 10, year, month, day));
    }

    if start + 10 <= bytes.len()
        && (bytes[start + 2] == b'/' || bytes[start + 2] == b'-')
        && bytes[start + 5] == bytes[start + 2]
    {
        let month = fixed_digits_at(bytes, start, 2)? as u8;
        let day = fixed_digits_at(bytes, start + 3, 2)? as u8;
        let year = fixed_digits_at(bytes, start + 6, 4)?;

        return Some((start + 10, year, month, day));
    }

    None
}

const MONTH_NAMES: [(&[u8], u8); 23] = [
    (b"january", 1),
    (b"jan", 1),
    (b"february", 2),
    (b"feb", 2),
    (b"march", 3),
    (b"mar", 3),
    (b"april", 4),
    (b"apr", 4),
    (b"may", 5),
    (b"june", 6),
    (b"jun", 6),
    (b"july", 7),
    (b"jul", 7),
    (b"august", 8),
    (b"aug", 8),
    (b"september", 9),
    (b"sep", 9),
    (b"october", 10),
    (b"oct", 10),
    (b"november", 11),
    (b"nov", 11),
    (b"december", 12),
    (b"dec", 12),
];

fn month_name_at(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    for (name, month) in MONTH_NAMES {
        let end = start + name.len();

        if end <= bytes.len()
            && bytes[start..end].eq_ignore_ascii_case(name)
            && (end == bytes.len() || !bytes[end].is_ascii_alphabetic())
        {
            return Some((end, month));
        }
    }

    None
}

fn named_date_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    let (mut cursor, month) = month_name_at(bytes, start)?;

    if bytes.get(cursor) != Some(&b' ') {
        return None;
    }
    cursor += 1;

    let day_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && cursor - day_start < 2 {
        cursor += 1;
    }

    if cursor == day_start || (cursor < bytes.len() && bytes[cursor].is_ascii_digit()) {
        return None;
    }

    let day = digits_value(&bytes[day_start..cursor]) as u8;

    if bytes.get(cursor) != Some(&b',') {
        return None;
    }
    cursor += 1;

    if bytes.get(cursor) != Some(&b' ') {
        return None;
    }
    cursor += 1;

    let year = fixed_digits_at(bytes, cursor, 4)?;
    Some((cursor + 4, year, month, day))
}

fn date_parts_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    numeric_date_at(bytes, start).or_else(|| named_date_at(bytes, start))
}

fn detect_date(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if (!bytes[start].is_ascii_digit() && !bytes[start].is_ascii_alphabetic())
            || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some((end, year, month, day)) = date_parts_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        if is_valid_date(year, month, day) {
            candidates.push(Candidate {
                label: Label::Date,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
    }
}

fn zip_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    let five_digit_end = start + 5;

    if five_digit_end > bytes.len()
        || !bytes[start..five_digit_end]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    if bytes.get(five_digit_end) == Some(&b'-') {
        let zip_plus_four_end = five_digit_end + 5;

        if zip_plus_four_end <= bytes.len()
            && bytes[five_digit_end + 1..zip_plus_four_end]
                .iter()
                .all(|byte| byte.is_ascii_digit())
        {
            return Some(zip_plus_four_end);
        }

        return None;
    }

    Some(five_digit_end)
}

fn detect_zip_code(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some(end) = zip_end_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        candidates.push(Candidate {
            label: Label::ZipCode,
            start_byte: start,
            end_byte: end,
        });
        start = end;
    }
}

fn ipv4_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;

    for group in 0..4 {
        let group_start = end;

        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }

        if !(1..=3).contains(&(end - group_start)) {
            return None;
        }

        if group < 3 {
            if bytes.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
    }

    Some(end)
}

fn ipv6_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    if !bytes[start].is_ascii_hexdigit() && bytes[start] != b':' {
        return None;
    }

    let mut end = start;
    let mut has_colon = false;

    while end < bytes.len() && (bytes[end].is_ascii_hexdigit() || bytes[end] == b':') {
        has_colon |= bytes[end] == b':';
        end += 1;
    }

    has_colon.then_some(end)
}

fn has_ip_boundaries(bytes: &[u8], start: usize, end: usize) -> bool {
    (start == 0 || !bytes[start - 1].is_ascii_alphanumeric())
        && (end == bytes.len() || !bytes[end].is_ascii_alphanumeric())
}

fn is_part_of_longer_ipv4(bytes: &[u8], start: usize, end: usize) -> bool {
    (start >= 2 && bytes[start - 1] == b'.' && bytes[start - 2].is_ascii_digit())
        || (bytes.get(end) == Some(&b'.')
            && bytes.get(end + 1).is_some_and(|byte| byte.is_ascii_digit()))
}

fn is_valid_ipv4(bytes: &[u8], start: usize, end: usize) -> bool {
    let Ok(candidate) = std::str::from_utf8(&bytes[start..end]) else {
        return false;
    };

    candidate.parse::<Ipv4Addr>().is_ok()
}

fn is_valid_ipv6(bytes: &[u8], start: usize, end: usize) -> bool {
    let Ok(candidate) = std::str::from_utf8(&bytes[start..end]) else {
        return false;
    };

    candidate.parse::<Ipv6Addr>().is_ok()
}

fn detect_ip_address(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if (!bytes[start].is_ascii_hexdigit() && bytes[start] != b':')
            || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        if let Some(end) = ipv4_end_at(bytes, start) {
            if has_ip_boundaries(bytes, start, end)
                && !is_part_of_longer_ipv4(bytes, start, end)
                && is_valid_ipv4(bytes, start, end)
            {
                candidates.push(Candidate {
                    label: Label::IpAddress,
                    start_byte: start,
                    end_byte: end,
                });
                start = end;
                continue;
            }
        }

        if let Some(end) = ipv6_end_at(bytes, start) {
            if has_ip_boundaries(bytes, start, end) && is_valid_ipv6(bytes, start, end) {
                candidates.push(Candidate {
                    label: Label::IpAddress,
                    start_byte: start,
                    end_byte: end,
                });
                start = end;
                continue;
            }
        }

        start += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{Finding, TextRange, scan};

    fn expected_finding(
        entity_type: &str,
        matched_text: &str,
        byte_range: (usize, usize),
        codepoint_range: (usize, usize),
        detector_name: &str,
    ) -> Finding {
        Finding {
            entity_type: entity_type.to_owned(),
            matched_text: matched_text.to_owned(),
            byte_range: TextRange {
                start: byte_range.0,
                end: byte_range.1,
            },
            codepoint_range: TextRange {
                start: codepoint_range.0,
                end: codepoint_range.1,
            },
            confidence: None,
            detector_name: detector_name.to_owned(),
            detector_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    fn expected_ascii_finding(
        entity_type: &str,
        matched_text: &str,
        start: usize,
        end: usize,
        detector_name: &str,
    ) -> Finding {
        expected_finding(
            entity_type,
            matched_text,
            (start, end),
            (start, end),
            detector_name,
        )
    }

    #[test]
    fn empty_input_has_no_entities() {
        assert!(scan("").is_empty());
    }

    #[test]
    fn detects_email_without_trailing_punctuation() {
        assert_eq!(
            scan("Contact jane@example.com."),
            vec![expected_ascii_finding(
                "EMAIL",
                "jane@example.com",
                8,
                24,
                "datafog-core/email",
            )]
        );
    }

    #[test]
    fn reports_email_byte_and_codepoint_ranges() {
        let text = "👋 jane@example.com";
        assert_eq!(
            scan(text),
            vec![expected_finding(
                "EMAIL",
                "jane@example.com",
                (5, 21),
                (2, 18),
                "datafog-core/email",
            )]
        );

        let finding = &scan(text)[0];
        assert_eq!(
            &text[finding.byte_range.start..finding.byte_range.end],
            finding.matched_text
        );
        assert_eq!(
            text.chars()
                .skip(finding.codepoint_range.start)
                .take(finding.codepoint_range.end - finding.codepoint_range.start)
                .collect::<String>(),
            finding.matched_text
        );
    }

    #[test]
    fn detects_north_american_phone_number() {
        assert_eq!(
            scan("Call (212) 555-0100."),
            vec![expected_ascii_finding(
                "PHONE",
                "(212) 555-0100",
                5,
                19,
                "datafog-core/phone",
            )]
        );
    }

    #[test]
    fn detects_international_phone_number() {
        assert_eq!(
            scan("Intl +44 20 7946 0958"),
            vec![expected_ascii_finding(
                "PHONE",
                "+44 20 7946 0958",
                5,
                21,
                "datafog-core/phone",
            )]
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
            vec![expected_ascii_finding(
                "SSN",
                "123-45-6789",
                5,
                16,
                "datafog-core/ssn",
            )]
        );
    }

    #[test]
    fn detects_undashed_ssn() {
        assert_eq!(scan("123456789")[0].entity_type, "SSN");
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
            vec![expected_ascii_finding(
                "CREDIT_CARD",
                "4111-1111-1111-1111",
                6,
                25,
                "datafog-core/credit-card",
            )]
        );
    }

    #[test]
    fn detects_supported_card_issuers() {
        assert_eq!(scan("5555 5555 5555 4444")[0].entity_type, "CREDIT_CARD");
        assert_eq!(scan("378282246310005")[0].entity_type, "CREDIT_CARD");
        assert_eq!(scan("6011111111111117")[0].entity_type, "CREDIT_CARD");
    }

    #[test]
    fn rejects_invalid_credit_card_candidates() {
        assert!(scan("4111-1111-1111-1112").is_empty()); // bad Luhn checksum
        assert!(scan("1234-5678-9012-3452").is_empty()); // unsupported prefix
        assert!(scan("x4111-1111-1111-1111y").is_empty()); // embedded
    }

    #[test]
    fn detects_numeric_and_named_dates() {
        assert_eq!(
            scan("Date: 2024-02-29"),
            vec![expected_ascii_finding(
                "DATE",
                "2024-02-29",
                6,
                16,
                "datafog-core/date",
            )]
        );

        assert_eq!(scan("12/27/1988")[0].entity_type, "DATE");
        assert_eq!(scan("Jan 15, 2024")[0].entity_type, "DATE");
    }

    #[test]
    fn rejects_invalid_or_unsupported_dates() {
        assert!(scan("2023-02-29").is_empty());
        assert!(scan("2024-02-30").is_empty());
        assert!(scan("12/27/88").is_empty());
        assert!(scan("27/12/2024").is_empty());
        assert!(scan("version2024-02-29x").is_empty());
    }

    #[test]
    fn detects_five_digit_and_plus_four_zip_codes() {
        assert_eq!(
            scan("ZIP: 94105-1234"),
            vec![expected_ascii_finding(
                "ZIP_CODE",
                "94105-1234",
                5,
                15,
                "datafog-core/zip-code",
            )]
        );

        assert_eq!(scan("94105")[0].entity_type, "ZIP_CODE");
    }

    #[test]
    fn rejects_invalid_or_embedded_zip_codes() {
        assert!(scan("1234").is_empty());
        assert!(scan("123456").is_empty());
        assert!(scan("x94105").is_empty());
        assert!(scan("94105x").is_empty());
        assert!(scan("94105-123").is_empty());
    }

    #[test]
    fn detects_ipv4_and_ipv6_addresses() {
        assert_eq!(
            scan("IPv4: 192.168.1.10"),
            vec![expected_ascii_finding(
                "IP_ADDRESS",
                "192.168.1.10",
                6,
                18,
                "datafog-core/ip-address",
            )]
        );

        assert_eq!(scan("2001:db8::1")[0].entity_type, "IP_ADDRESS");
        assert_eq!(scan("[2001:db8::1]:443")[0].matched_text, "2001:db8::1");
        assert_eq!(scan("192.168.1.10:8080")[0].matched_text, "192.168.1.10");
    }

    #[test]
    fn rejects_invalid_or_embedded_ip_addresses() {
        assert!(scan("256.1.1.1").is_empty());
        assert!(scan("1.2.3.4.5").is_empty());
        assert!(scan("2001:db8::zzzz").is_empty());
        assert!(scan("host192.168.1.10name").is_empty());
    }
}
