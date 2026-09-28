//! Credential-bearing PostgreSQL URI recognition, without connection validation.
use crate::{Candidate, Label};
use std::net::{Ipv4Addr, Ipv6Addr};

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if !matches!(bytes[cursor], b'p' | b'P') {
            cursor += 1;
            continue;
        }
        let remaining = &bytes[cursor..];
        let prefix = [b"postgresql://".as_slice(), b"postgres://".as_slice()]
            .into_iter()
            .find(|prefix| {
                remaining
                    .get(..prefix.len())
                    .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
            });
        let Some(prefix) = prefix else {
            cursor += 1;
            continue;
        };
        if cursor > 0
            && (bytes[cursor - 1].is_ascii_alphanumeric()
                || matches!(bytes[cursor - 1], b'_' | b'+' | b'-' | b'.' | b'/' | b':'))
        {
            cursor += prefix.len();
            continue;
        }
        let enclosing = cursor.checked_sub(1).map(|index| bytes[index]);
        let quoted = enclosing == Some(b'\'');
        let start = cursor;
        cursor += prefix.len();
        let mut parentheses = 0usize;
        let mut brackets = 0usize;
        let mut userinfo_ended = false;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if byte.is_ascii_whitespace()
                || (!byte.is_ascii()
                    && text
                        .get(cursor..)
                        .and_then(|suffix| suffix.chars().next())
                        .is_some_and(char::is_whitespace))
                || matches!(byte, b'"' | b'<' | b'>' | b'`' | b'{' | b'}')
                || (quoted && byte == b'\'')
            {
                break;
            }
            // Closing prose wrappers are excluded; brackets inside IPv6 literals
            // and balanced parentheses in URI components remain part of the span.
            match byte {
                b'(' => parentheses += 1,
                b')' if parentheses == 0 && userinfo_ended && enclosing == Some(b'(') => break,
                b')' => parentheses = parentheses.saturating_sub(1),
                b'[' => brackets += 1,
                b']' if brackets == 0 && userinfo_ended && enclosing == Some(b'[') => break,
                b']' => brackets = brackets.saturating_sub(1),
                b'@' => userinfo_ended = true,
                _ => {}
            }
            cursor += 1;
        }
        let uri = &bytes[start + prefix.len()..cursor];
        if valid(uri) {
            candidates.push(Candidate {
                label: Label::CredentialUri,
                start_byte: start,
                end_byte: cursor,
            });
        }
    }
}

fn valid(uri: &[u8]) -> bool {
    let authority_end = uri
        .iter()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
        .unwrap_or(uri.len());
    let authority = &uri[..authority_end];
    let Some(at) = authority.iter().position(|byte| *byte == b'@') else {
        return false;
    };
    let userinfo = &authority[..at];
    let Some(colon) = userinfo.iter().position(|byte| *byte == b':') else {
        return false;
    };
    let password = &userinfo[colon + 1..];
    if password.is_empty()
        || !component(userinfo, |byte| {
            unreserved(byte) || sub_delimiter(byte) || byte == b':'
        })
        || !host_and_port(&authority[at + 1..])
    {
        return false;
    }
    let suffix = &uri[authority_end..];
    // Fragments are not part of libpq connection URI syntax. Invalid suffixes
    // reject the whole token instead of exposing a misleading truncated finding.
    if suffix.contains(&b'#') {
        return false;
    }
    let (path, query) = match suffix.iter().position(|byte| *byte == b'?') {
        Some(index) => (&suffix[..index], Some(&suffix[index + 1..])),
        None => (suffix, None),
    };
    component(path, |byte| pchar(byte) || byte == b'/')
        && query
            .is_none_or(|query| component(query, |byte| pchar(byte) || matches!(byte, b'/' | b'?')))
}

fn host_and_port(authority: &[u8]) -> bool {
    let (host, port) = if authority.first() == Some(&b'[') {
        let Some(close) = authority.iter().position(|byte| *byte == b']') else {
            return false;
        };
        if std::str::from_utf8(&authority[1..close])
            .ok()
            .and_then(|host| host.parse::<Ipv6Addr>().ok())
            .is_none()
        {
            return false;
        }
        let suffix = &authority[close + 1..];
        return suffix.is_empty() || suffix.strip_prefix(b":").is_some_and(valid_port);
    } else if let Some(colon) = authority.iter().position(|byte| *byte == b':') {
        (&authority[..colon], Some(&authority[colon + 1..]))
    } else {
        (authority, None)
    };
    // This initial scope supports single network hosts and libpq's omitted-host
    // default, excluding host lists and percent-encoded Unix socket directories.
    let host_valid = if host
        .iter()
        .all(|byte| byte.is_ascii_digit() || *byte == b'.')
        && host.contains(&b'.')
    {
        std::str::from_utf8(host)
            .ok()
            .and_then(|host| host.parse::<Ipv4Addr>().ok())
            .is_some()
    } else {
        host.is_empty()
            || host
                .strip_suffix(b".")
                .unwrap_or(host)
                .split(|byte| *byte == b'.')
                .all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label.first() != Some(&b'-')
                        && label.last() != Some(&b'-')
                        && label
                            .iter()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                })
    };
    host_valid && host.len() <= 253 && port.is_none_or(valid_port)
}

fn valid_port(port: &[u8]) -> bool {
    !port.is_empty() && port.iter().all(u8::is_ascii_digit)
}

fn component(bytes: &[u8], allowed: impl Fn(u8) -> bool) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(encoded) = bytes.get(index + 1..index + 3) else {
                return false;
            };
            if !encoded.iter().all(u8::is_ascii_hexdigit) {
                return false;
            }
            index += 3;
        } else if allowed(bytes[index]) {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn sub_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'\'' | b'!' | b'$' | b'&' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn pchar(byte: u8) -> bool {
    unreserved(byte) || sub_delimiter(byte) || matches!(byte, b':' | b'@')
}
