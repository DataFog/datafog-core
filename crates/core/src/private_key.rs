//! Lexical detection of complete private-key armor; no cryptographic validation.
use crate::{Candidate, Label};

const KEY_TYPES: [&str; 7] = [
    "PRIVATE KEY",
    "RSA PRIVATE KEY",
    "EC PRIVATE KEY",
    "DSA PRIVATE KEY",
    "OPENSSH PRIVATE KEY",
    "ENCRYPTED PRIVATE KEY",
    "PGP PRIVATE KEY BLOCK",
];

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let mut active: Option<(&str, usize)> = None;
    let mut body_length = 0usize;
    let mut padding = 0usize;
    let mut offset = 0usize;
    let bytes = text.as_bytes();
    // Preserve source offsets for both actual newlines and JSON/env escaped newlines.
    while offset < text.len() {
        let mut end = offset;
        while end < bytes.len()
            && bytes[end] != b'\n'
            && !(bytes[end] == b'\\' && bytes.get(end + 1) == Some(&b'n'))
        {
            end += 1;
        }
        let raw_line = &text[offset..end];
        let line = raw_line
            .strip_suffix("\\r")
            .unwrap_or(raw_line)
            .trim_end_matches('\r');
        let begin = line
            .strip_prefix("-----BEGIN ")
            .and_then(|line| line.strip_suffix("-----"))
            .filter(|key_type| KEY_TYPES.contains(key_type));
        if let Some(key_type) = begin {
            active = Some((key_type, offset));
            body_length = 0;
            padding = 0;
        } else if let Some((key_type, start_byte)) = active {
            let pgp = key_type == "PGP PRIVATE KEY BLOCK";
            if let Some(end_type) = line
                .strip_prefix("-----END ")
                .and_then(|line| line.strip_suffix("-----"))
            {
                if end_type == key_type && body_length > 0 && body_length.is_multiple_of(4) {
                    candidates.push(Candidate {
                        label: Label::PrivateKey,
                        start_byte,
                        end_byte: offset + line.len(),
                    });
                }
                active = None;
            } else if pgp
                && ((body_length == 0
                    && (line.is_empty()
                        || line.starts_with("Version:")
                        || line.starts_with("Comment:")))
                    || (body_length > 0
                        && line.len() == 5
                        && line.starts_with('=')
                        && line[1..]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/'))))
            {
                // OpenPGP ASCII armor metadata and optional checksum line.
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
        if end == bytes.len() {
            break;
        }
        offset = end + if bytes[end] == b'\\' { 2 } else { 1 };
    }
}
