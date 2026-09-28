//! Opt-in recognition of canonical UUID identifiers, not sensitivity inference.
use crate::{Candidate, Label};

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    // Fixed-size windows keep matching bounded, including long near-matches.
    for (start, value) in bytes.windows(36).enumerate() {
        if start > 0 && identifier_byte(bytes[start - 1]) {
            continue;
        }
        if bytes
            .get(start + 36)
            .is_some_and(|byte| identifier_byte(*byte))
        {
            continue;
        }
        // RFC 9562 textual form, versions 1–8 and the IETF 10xx variant.
        // Nil and Max UUID sentinels deliberately do not match this policy.
        if !matches!(value[14], b'1'..=b'8')
            || !matches!(value[19], b'8' | b'9' | b'a' | b'A' | b'b' | b'B')
        {
            continue;
        }
        if value.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }) {
            candidates.push(Candidate {
                label: Label::Uuid,
                start_byte: start,
                end_byte: start + 36,
            });
        }
    }
}
