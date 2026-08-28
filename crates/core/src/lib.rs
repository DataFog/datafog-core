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

/// A privacy transformation applied to a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformationStrategy {
    /// Replace the finding with its unnumbered entity-type placeholder.
    Redact,
}

/// One transformation applied to the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Transformation {
    /// The source finding that was transformed.
    pub finding: Finding,
    /// Strategy applied to the finding.
    pub strategy: TransformationStrategy,
    /// Exact replacement inserted into the output text.
    pub replacement: String,
    /// Range of the replacement in UTF-8 bytes in the output text.
    pub output_byte_range: TextRange,
    /// Range of the replacement in Unicode code points in the output text.
    pub output_codepoint_range: TextRange,
}

/// Text and audit records produced by a transformation.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformResult {
    /// Transformed text.
    pub text: String,
    /// Applied transformations in source document order.
    pub transformations: Vec<Transformation>,
}

/// Reason a caller-supplied finding is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingValidationError {
    /// The UTF-8 byte range is empty or reversed.
    EmptyOrReversedByteRange,
    /// The UTF-8 byte range extends beyond the source text.
    ByteRangeOutOfBounds,
    /// A UTF-8 byte offset does not fall on a character boundary.
    InvalidUtf8Boundary,
    /// The Unicode code-point range is empty or reversed.
    EmptyOrReversedCodepointRange,
    /// The Unicode code-point range extends beyond the source text.
    CodepointRangeOutOfBounds,
    /// The byte and code-point ranges select different source spans.
    InconsistentRanges,
    /// `matched_text` differs from the source substring selected by the range.
    MatchedTextMismatch,
    /// Confidence is non-finite or outside `0.0..=1.0`.
    InvalidConfidence,
}

/// A transformation request could not be completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformError {
    /// Index of the invalid finding in the caller-supplied slice.
    pub finding_index: usize,
    /// Validation failure.
    pub kind: FindingValidationError,
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid finding at index {}: {:?}",
            self.finding_index, self.kind
        )
    }
}

impl std::error::Error for TransformError {}

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

/// Transform caller-supplied findings without scanning implicitly.
pub fn transform(
    text: &str,
    findings: &[Finding],
    strategy: TransformationStrategy,
) -> Result<TransformResult, TransformError> {
    for (finding_index, finding) in findings.iter().enumerate() {
        if let Err(kind) = validate_finding(text, finding) {
            return Err(TransformError {
                finding_index,
                kind,
            });
        }
    }

    let mut selected_findings: Vec<Finding> = Vec::with_capacity(findings.len());
    for finding in findings {
        if let Some(existing) = selected_findings
            .iter_mut()
            .find(|existing| findings_are_duplicates(existing, finding))
        {
            if duplicate_preference(finding, existing).is_lt() {
                *existing = finding.clone();
            }
        } else {
            selected_findings.push(finding.clone());
        }
    }
    selected_findings.sort_by_key(|finding| {
        (
            finding.codepoint_range.start,
            finding.codepoint_range.end,
            finding.entity_type.clone(),
        )
    });
    let selected_findings = resolve_overlaps(selected_findings);

    let mut output = String::with_capacity(text.len());
    let mut transformations = Vec::with_capacity(selected_findings.len());
    let mut source_byte_cursor = 0;

    for finding in &selected_findings {
        output.push_str(&text[source_byte_cursor..finding.byte_range.start]);
        let output_byte_start = output.len();
        let output_codepoint_start = output.chars().count();
        let replacement = match strategy {
            TransformationStrategy::Redact => format!("[{}]", finding.entity_type),
        };
        output.push_str(&replacement);

        transformations.push(Transformation {
            finding: finding.clone(),
            strategy,
            replacement,
            output_byte_range: TextRange {
                start: output_byte_start,
                end: output.len(),
            },
            output_codepoint_range: TextRange {
                start: output_codepoint_start,
                end: output.chars().count(),
            },
        });
        source_byte_cursor = finding.byte_range.end;
    }

    output.push_str(&text[source_byte_cursor..]);
    Ok(TransformResult {
        text: output,
        transformations,
    })
}

/// Scan text and transform the resulting findings in one explicit convenience operation.
pub fn scan_and_transform(
    text: &str,
    strategy: TransformationStrategy,
) -> Result<TransformResult, TransformError> {
    transform(text, &scan(text), strategy)
}

fn findings_are_duplicates(left: &Finding, right: &Finding) -> bool {
    left.entity_type == right.entity_type
        && left.matched_text == right.matched_text
        && left.byte_range == right.byte_range
        && left.codepoint_range == right.codepoint_range
}

fn duplicate_preference(candidate: &Finding, existing: &Finding) -> std::cmp::Ordering {
    if let (Some(candidate_confidence), Some(existing_confidence)) =
        (candidate.confidence, existing.confidence)
    {
        let confidence_order = existing_confidence.total_cmp(&candidate_confidence);
        if !confidence_order.is_eq() {
            return confidence_order;
        }
    }

    (&candidate.detector_name, &candidate.detector_version)
        .cmp(&(&existing.detector_name, &existing.detector_version))
}

fn resolve_overlaps(mut remaining: Vec<Finding>) -> Vec<Finding> {
    let mut selected = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let mut preferred_index = 0;
        for candidate_index in 1..remaining.len() {
            if overlap_preference(&remaining[candidate_index], &remaining[preferred_index]).is_lt()
            {
                preferred_index = candidate_index;
            }
        }

        let preferred = remaining.remove(preferred_index);
        remaining.retain(|finding| !findings_overlap(&preferred, finding));
        selected.push(preferred);
    }

    selected.sort_by(|left, right| {
        left.byte_range
            .start
            .cmp(&right.byte_range.start)
            .then_with(|| left.byte_range.end.cmp(&right.byte_range.end))
            .then_with(|| left.entity_type.cmp(&right.entity_type))
    });
    selected
}

fn findings_overlap(left: &Finding, right: &Finding) -> bool {
    left.byte_range.start < right.byte_range.end && right.byte_range.start < left.byte_range.end
}

fn overlap_preference(left: &Finding, right: &Finding) -> std::cmp::Ordering {
    let left_contains_right = left.byte_range.start <= right.byte_range.start
        && left.byte_range.end >= right.byte_range.end;
    let right_contains_left = right.byte_range.start <= left.byte_range.start
        && right.byte_range.end >= left.byte_range.end;
    match (left_contains_right, right_contains_left) {
        (true, false) => return std::cmp::Ordering::Less,
        (false, true) => return std::cmp::Ordering::Greater,
        _ => {}
    }

    let left_length = left.codepoint_range.end - left.codepoint_range.start;
    let right_length = right.codepoint_range.end - right.codepoint_range.start;
    let length_order = right_length.cmp(&left_length);
    if !length_order.is_eq() {
        return length_order;
    }

    if let (Some(left_confidence), Some(right_confidence)) = (left.confidence, right.confidence) {
        let confidence_order = right_confidence.total_cmp(&left_confidence);
        if !confidence_order.is_eq() {
            return confidence_order;
        }
    }

    left.codepoint_range
        .start
        .cmp(&right.codepoint_range.start)
        .then_with(|| left.entity_type.cmp(&right.entity_type))
        .then_with(|| left.detector_name.cmp(&right.detector_name))
        .then_with(|| left.detector_version.cmp(&right.detector_version))
}

fn validate_finding(text: &str, finding: &Finding) -> Result<(), FindingValidationError> {
    if finding.byte_range.start >= finding.byte_range.end {
        return Err(FindingValidationError::EmptyOrReversedByteRange);
    }
    if finding.byte_range.end > text.len() {
        return Err(FindingValidationError::ByteRangeOutOfBounds);
    }
    if !text.is_char_boundary(finding.byte_range.start)
        || !text.is_char_boundary(finding.byte_range.end)
    {
        return Err(FindingValidationError::InvalidUtf8Boundary);
    }
    if finding.codepoint_range.start >= finding.codepoint_range.end {
        return Err(FindingValidationError::EmptyOrReversedCodepointRange);
    }

    let Some(codepoint_start_byte) = byte_offset_at_codepoint(text, finding.codepoint_range.start)
    else {
        return Err(FindingValidationError::CodepointRangeOutOfBounds);
    };
    let Some(codepoint_end_byte) = byte_offset_at_codepoint(text, finding.codepoint_range.end)
    else {
        return Err(FindingValidationError::CodepointRangeOutOfBounds);
    };
    if codepoint_start_byte != finding.byte_range.start
        || codepoint_end_byte != finding.byte_range.end
    {
        return Err(FindingValidationError::InconsistentRanges);
    }
    if text[finding.byte_range.start..finding.byte_range.end] != finding.matched_text {
        return Err(FindingValidationError::MatchedTextMismatch);
    }
    if finding
        .confidence
        .is_some_and(|confidence| !confidence.is_finite() || !(0.0..=1.0).contains(&confidence))
    {
        return Err(FindingValidationError::InvalidConfidence);
    }
    Ok(())
}

fn byte_offset_at_codepoint(text: &str, codepoint_offset: usize) -> Option<usize> {
    text.char_indices()
        .map(|(byte_offset, _)| byte_offset)
        .chain(std::iter::once(text.len()))
        .nth(codepoint_offset)
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
    use super::{
        Finding, FindingValidationError, TextRange, TransformError, TransformationStrategy, scan,
        scan_and_transform, transform,
    };

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

    fn supplied_ascii_finding(
        text: &str,
        entity_type: &str,
        start: usize,
        end: usize,
        confidence: Option<f32>,
        detector_name: &str,
    ) -> Finding {
        Finding {
            entity_type: entity_type.to_owned(),
            matched_text: text[start..end].to_owned(),
            byte_range: TextRange { start, end },
            codepoint_range: TextRange { start, end },
            confidence,
            detector_name: detector_name.to_owned(),
            detector_version: Some("1".to_owned()),
        }
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

    #[test]
    fn redacts_explicit_findings_and_reports_output_ranges() {
        let text = "Contact jane@example.com";
        let findings = scan(text);

        let result = transform(text, &findings, TransformationStrategy::Redact).unwrap();

        assert_eq!(result.text, "Contact [EMAIL]");
        assert_eq!(result.transformations.len(), 1);
        let transformation = &result.transformations[0];
        assert_eq!(transformation.finding, findings[0]);
        assert_eq!(transformation.strategy, TransformationStrategy::Redact);
        assert_eq!(transformation.replacement, "[EMAIL]");
        assert_eq!(
            transformation.output_byte_range,
            TextRange { start: 8, end: 15 }
        );
        assert_eq!(
            transformation.output_codepoint_range,
            TextRange { start: 8, end: 15 }
        );
    }

    #[test]
    fn rejects_a_finding_whose_matched_text_differs_from_the_source() {
        let text = "Contact jane@example.com";
        let mut findings = scan(text);
        findings[0].matched_text = "other@example.com".to_owned();

        assert_eq!(
            transform(text, &findings, TransformationStrategy::Redact),
            Err(TransformError {
                finding_index: 0,
                kind: FindingValidationError::MatchedTextMismatch,
            })
        );
    }

    #[test]
    fn rejects_malformed_ranges_and_confidence() {
        let text = "👋 jane@example.com";
        let original = scan(text).remove(0);

        let cases = [
            (
                Finding {
                    byte_range: TextRange { start: 5, end: 5 },
                    ..original.clone()
                },
                FindingValidationError::EmptyOrReversedByteRange,
            ),
            (
                Finding {
                    byte_range: TextRange { start: 5, end: 99 },
                    ..original.clone()
                },
                FindingValidationError::ByteRangeOutOfBounds,
            ),
            (
                Finding {
                    byte_range: TextRange { start: 1, end: 21 },
                    ..original.clone()
                },
                FindingValidationError::InvalidUtf8Boundary,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 2, end: 2 },
                    ..original.clone()
                },
                FindingValidationError::EmptyOrReversedCodepointRange,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 2, end: 99 },
                    ..original.clone()
                },
                FindingValidationError::CodepointRangeOutOfBounds,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 1, end: 17 },
                    ..original.clone()
                },
                FindingValidationError::InconsistentRanges,
            ),
            (
                Finding {
                    confidence: Some(f32::NAN),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
            (
                Finding {
                    confidence: Some(-0.1),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
            (
                Finding {
                    confidence: Some(1.1),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
        ];

        for (finding, expected_kind) in cases {
            assert_eq!(
                transform(text, &[finding], TransformationStrategy::Redact),
                Err(TransformError {
                    finding_index: 0,
                    kind: expected_kind,
                })
            );
        }
    }

    #[test]
    fn collapses_duplicate_findings_and_retains_higher_confidence() {
        let text = "Email jane@example.com";
        let mut lower_confidence = scan(text).remove(0);
        lower_confidence.confidence = Some(0.7);
        lower_confidence.detector_name = "z-detector".to_owned();
        let mut higher_confidence = lower_confidence.clone();
        higher_confidence.confidence = Some(0.9);
        higher_confidence.detector_name = "a-detector".to_owned();

        let result = transform(
            text,
            &[lower_confidence, higher_confidence.clone()],
            TransformationStrategy::Redact,
        )
        .unwrap();

        assert_eq!(result.text, "Email [EMAIL]");
        assert_eq!(result.transformations.len(), 1);
        assert_eq!(result.transformations[0].finding, higher_confidence);
    }

    #[test]
    fn containing_overlap_wins_even_when_the_inner_finding_has_higher_confidence() {
        let text = "Acme Corporation announced";
        let outer = supplied_ascii_finding(
            text,
            "ORGANIZATION",
            0,
            16,
            Some(0.6),
            "organization-detector",
        );
        let inner = supplied_ascii_finding(text, "PERSON", 0, 4, Some(0.99), "person-detector");

        let result = transform(
            text,
            &[inner, outer.clone()],
            TransformationStrategy::Redact,
        )
        .unwrap();

        assert_eq!(result.text, "[ORGANIZATION] announced");
        assert_eq!(result.transformations.len(), 1);
        assert_eq!(result.transformations[0].finding, outer);
    }

    #[test]
    fn scan_and_transform_redacts_unicode_input_with_exact_output_ranges() {
        let text = "👋 jane@example.com and jane@example.com";

        let result = scan_and_transform(text, TransformationStrategy::Redact).unwrap();

        assert_eq!(result.text, "👋 [EMAIL] and [EMAIL]");
        assert_eq!(result.transformations.len(), 2);
        assert_eq!(
            result.transformations[0].output_byte_range,
            TextRange { start: 5, end: 12 }
        );
        assert_eq!(
            result.transformations[0].output_codepoint_range,
            TextRange { start: 2, end: 9 }
        );
        assert_eq!(
            &result.text[result.transformations[1].output_byte_range.start
                ..result.transformations[1].output_byte_range.end],
            "[EMAIL]"
        );
        assert_eq!(result.transformations[0].replacement, "[EMAIL]");
        assert_eq!(result.transformations[1].replacement, "[EMAIL]");
    }

    #[test]
    fn equal_length_overlaps_use_confidence_when_both_findings_provide_it() {
        let text = "123456789";
        let lower = supplied_ascii_finding(text, "ALPHA", 0, 9, Some(0.7), "a");
        let higher = supplied_ascii_finding(text, "ZETA", 0, 9, Some(0.9), "z");

        let result = transform(
            text,
            &[lower, higher.clone()],
            TransformationStrategy::Redact,
        )
        .unwrap();

        assert_eq!(result.text, "[ZETA]");
        assert_eq!(result.transformations[0].finding, higher);
    }

    #[test]
    fn missing_confidence_does_not_rank_as_zero() {
        let text = "123456789";
        let unscored = supplied_ascii_finding(text, "ALPHA", 0, 9, None, "z");
        let scored = supplied_ascii_finding(text, "ZETA", 0, 9, Some(0.99), "a");

        let result = transform(
            text,
            &[scored, unscored.clone()],
            TransformationStrategy::Redact,
        )
        .unwrap();

        assert_eq!(result.text, "[ALPHA]");
        assert_eq!(result.transformations[0].finding, unscored);
    }

    #[test]
    fn equal_partial_overlaps_prefer_the_earlier_source_position() {
        let text = "abcdef";
        let earlier = supplied_ascii_finding(text, "ZETA", 0, 4, None, "z");
        let later = supplied_ascii_finding(text, "ALPHA", 2, 6, None, "a");

        let result = transform(
            text,
            &[later, earlier.clone()],
            TransformationStrategy::Redact,
        )
        .unwrap();

        assert_eq!(result.text, "[ZETA]ef");
        assert_eq!(result.transformations[0].finding, earlier);
    }

    #[test]
    fn empty_findings_leave_text_unchanged() {
        let result = transform("plain text", &[], TransformationStrategy::Redact).unwrap();

        assert_eq!(result.text, "plain text");
        assert!(result.transformations.is_empty());
    }
}
