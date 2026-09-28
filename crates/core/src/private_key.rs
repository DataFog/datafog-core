//! Lexical detection of complete PEM private-key blocks; no cryptographic validation.
use crate::{Candidate, Label};

const KEY_TYPES: [&str; 6] = [
    "PRIVATE KEY",
    "RSA PRIVATE KEY",
    "EC PRIVATE KEY",
    "DSA PRIVATE KEY",
    "OPENSSH PRIVATE KEY",
    "ENCRYPTED PRIVATE KEY",
];

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let mut active: Option<(&str, usize)> = None;
    let mut body_length = 0usize;
    let mut padding = 0usize;
    let mut offset = 0usize;
    for raw_line in text.split_inclusive('\n') {
        let line = raw_line
            .strip_suffix('\n')
            .map_or(raw_line, |line| line.strip_suffix('\r').unwrap_or(line));
        let begin = line
            .strip_prefix("-----BEGIN ")
            .and_then(|line| line.strip_suffix("-----"))
            .filter(|key_type| KEY_TYPES.contains(key_type));
        if let Some(key_type) = begin {
            // A new header recovers after a malformed block without rescanning its body.
            active = Some((key_type, offset));
            body_length = 0;
            padding = 0;
        } else if let Some((key_type, start_byte)) = active {
            let end = line
                .strip_prefix("-----END ")
                .and_then(|line| line.strip_suffix("-----"));
            if let Some(end_type) = end {
                if end_type == key_type && body_length > 0 && body_length.is_multiple_of(4) {
                    candidates.push(Candidate {
                        label: Label::PrivateKey,
                        start_byte,
                        end_byte: offset + line.len(),
                    });
                }
                active = None;
            } else if line.is_empty() {
                active = None;
            } else {
                for byte in line.bytes() {
                    if byte == b'=' && padding < 2 {
                        padding += 1;
                    } else if padding != 0
                        || !(byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
                    {
                        active = None;
                        break;
                    }
                    body_length += 1;
                }
            }
        }
        offset += raw_line.len();
    }
}
