//! Compact JWT recognition; no authentication or claim validation.
use crate::{Candidate, Label};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    // Consume maximal token-like runs once, preventing suffix rescans and partial
    // matches of padded, standard-base64, or extra-segment tokens.
    let mut offset = 0;
    for run in text.as_bytes().split(|byte| !token_byte(*byte)) {
        // One final period may be sentence punctuation. Prefer the full run so
        // the empty signature delimiter of an unsecured JWT remains in its span.
        let token = if valid(run) {
            Some(run)
        } else {
            run.strip_suffix(b".").filter(|token| valid(token))
        };
        if let Some(token) = token {
            candidates.push(Candidate {
                label: Label::Jwt,
                start_byte: offset,
                end_byte: offset + token.len(),
            });
        }
        offset += run.len() + 1;
    }
}

fn token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'=' | b'+' | b'/')
}

fn valid(run: &[u8]) -> bool {
    let mut parts = run.split(|byte| *byte == b'.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some(header) = object(header) else {
        return false;
    };
    let Some(algorithm) = header
        .get("alg")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return false;
    };
    if object(payload).is_none() {
        return false;
    }
    if algorithm == "none" {
        signature.is_empty()
    } else {
        !signature.is_empty() && URL_SAFE_NO_PAD.decode(signature).is_ok()
    }
}

fn object(segment: &[u8]) -> Option<serde_json::Map<String, Value>> {
    let decoded = URL_SAFE_NO_PAD.decode(segment).ok()?;
    match serde_json::from_slice(&decoded) {
        Ok(Value::Object(object)) => Some(object),
        _ => None,
    }
}
