//! Opt-in recognition of canonical UUID identifiers, not sensitivity inference.
use crate::{Candidate, Label};

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    // A canonical UUID's first hyphen is its ninth byte, so each hyphen names
    // the one window that could start there. Hyphens arrive in order and
    // accepted windows cannot overlap, so candidates stay in source order.
    let mut search = 0;
    while let Some(found) = text[search..].find('-') {
        let hyphen = search + found;
        search = hyphen + 1;
        if let Some(start) = hyphen.checked_sub(8) {
            if window_at(bytes, start) {
                candidates.push(Candidate {
                    label: Label::Uuid,
                    start_byte: start,
                    end_byte: start + 36,
                });
            }
        }
    }
}

fn window_at(bytes: &[u8], start: usize) -> bool {
    let Some(value) = bytes.get(start..start + 36) else {
        return false;
    };
    if start > 0 && identifier_byte(bytes[start - 1]) {
        return false;
    }
    if bytes
        .get(start + 36)
        .is_some_and(|byte| identifier_byte(*byte))
    {
        return false;
    }
    // RFC 9562 textual form, versions 1–8 and the IETF 10xx variant.
    // Nil and Max UUID sentinels deliberately do not match this policy.
    if !matches!(value[14], b'1'..=b'8')
        || !matches!(value[19], b'8' | b'9' | b'a' | b'A' | b'b' | b'B')
    {
        return false;
    }
    value.iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyphen_anchored_windows_match_every_window() {
        let id = "123e4567-e89b-42d3-a456-426614174000";
        for (before, after) in [
            ("", ""),
            (" ", "."),
            ("-", ""),
            ("", "-"),
            ("x", ""),
            ("é", "中"),
            ("{", "}"),
            ("12345678-", ""),
        ] {
            let text = format!(
                "id {before}{id}{after} {id}{id} -{id}- 00000000-0000-0000-0000-000000000000"
            );
            let mut found = Vec::new();
            detect(&text, &mut found);
            let reference: Vec<_> = (0..text.len().saturating_sub(35))
                .filter(|start| window_at(text.as_bytes(), *start))
                .map(|start| Candidate {
                    label: Label::Uuid,
                    start_byte: start,
                    end_byte: start + 36,
                })
                .collect();
            assert_eq!(found, reference, "{text}");
        }
    }
}
