//! Compact JWT recognition; no authentication or claim validation.
use crate::{Candidate, Label};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    // Consume maximal token-like runs once, preventing suffix rescans and partial
    // matches of padded, standard-base64, or extra-segment tokens. Every JWT has
    // two periods, so only runs around a period are examined.
    let bytes = text.as_bytes();
    let mut search = 0;
    while let Some(found) = text[search..].find('.') {
        let period = search + found;
        let start = bytes[..period]
            .iter()
            .rposition(|byte| !token_byte(*byte))
            .map_or(0, |delimiter| delimiter + 1);
        let end = bytes[period..]
            .iter()
            .position(|byte| !token_byte(*byte))
            .map_or(bytes.len(), |delimiter| period + delimiter);
        let run = &bytes[start..end];
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
                start_byte: start,
                end_byte: start + token.len(),
            });
        }
        // Run delimiters are ASCII or the first byte of a character.
        search = end;
    }
}

fn token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'=' | b'+' | b'/')
}

fn valid(run: &[u8]) -> bool {
    // A header that decodes to a JSON object begins with `{` or JSON whitespace,
    // whose base64url encodings start with one of these characters. This rejects
    // ordinary words before any splitting or decoding.
    if !matches!(run.first(), Some(b'e' | b'I' | b'C' | b'D')) {
        return false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(text: &str) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        let mut offset = 0;
        for run in text.as_bytes().split(|byte| !token_byte(*byte)) {
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
        candidates
    }

    #[test]
    fn period_search_matches_the_whole_text_walk() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#);
        let none = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"sub":"1"}"#);
        let tokens = [
            format!("{header}.{payload}.c2ln"),
            format!("{none}.{payload}."),
            format!("{none}.{payload}"),
            format!("{header}.{payload}.c2ln.extra"),
            format!("{header}.{payload}.c2ln="),
            format!("{header}..c2ln"),
            ".".into(),
            "..".into(),
            "a.b.c".into(),
        ];
        for token in &tokens {
            for (before, after) in [
                ("", ""),
                (" ", "."),
                ("é", "中"),
                ("x", ""),
                ("(", ")."),
                ("..", ".."),
                ("Bearer ", " ok"),
            ] {
                let text = format!("start {before}{token}{after} mid {token}");
                let mut found = Vec::new();
                detect(&text, &mut found);
                assert_eq!(found, reference(&text), "{text}");
            }
        }
    }

    #[test]
    fn every_json_object_header_passes_the_first_character_filter() {
        // serde_json accepts exactly these four whitespace bytes before `{`.
        for leading in ["", " ", "\t", "\n", "\r", " \n\t\r"] {
            let header = URL_SAFE_NO_PAD.encode(format!("{leading}{{\"alg\":\"none\"}}"));
            let payload = URL_SAFE_NO_PAD.encode("{}");
            assert!(object(header.as_bytes()).is_some());
            assert!(
                valid(format!("{header}.{payload}.").as_bytes()),
                "{leading:?}"
            );
        }
        for byte in 0..=u8::MAX {
            let encoded = URL_SAFE_NO_PAD.encode([byte, b'{', b'}']);
            let parses = matches!(
                serde_json::from_slice::<Value>(&[byte, b'{', b'}']),
                Ok(Value::Object(_))
            ) || byte == b'{';
            if parses {
                assert!(matches!(encoded.as_bytes()[0], b'e' | b'I' | b'C' | b'D'));
            }
        }
    }
}
