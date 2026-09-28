//! Provider-prefixed credential shapes, without network or cryptographic validation.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

// Include Unicode identifier characters and unsupported token punctuation so a
// plausible prefix inside a longer identifier cannot produce a partial finding.
static TOKEN_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\w.+/\-~]+").expect("static API key token regex must compile"));

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    for matched in TOKEN_RUN.find_iter(text) {
        if text.as_bytes().get(matched.end()) == Some(&b'=') {
            continue;
        }
        // Preserve ambiguous punctuation when it belongs to a supported opaque
        // shape. Only remove a prose period if the full token is not supported.
        let token = matched.as_str();
        let token = if valid(token) {
            Some(token)
        } else {
            token.strip_suffix('.').filter(|token| valid(token))
        };
        if let Some(token) = token {
            candidates.push(Candidate {
                label: Label::ApiKey,
                start_byte: matched.start(),
                end_byte: matched.start() + token.len(),
            });
        }
    }
}

fn alphanumeric(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn valid(token: &str) -> bool {
    for prefix in ["ghp_", "gho_", "ghu_"] {
        if let Some(body) = token.strip_prefix(prefix) {
            return body.len() == 36 && alphanumeric(body);
        }
    }
    if let Some(body) = token.strip_prefix("ghr_") {
        return body.len() == 76 && alphanumeric(body);
    }
    if let Some(body) = token.strip_prefix("github_pat_") {
        return body.split_once('_').is_some_and(|(id, secret)| {
            id.len() == 22 && secret.len() == 59 && alphanumeric(id) && alphanumeric(secret)
        });
    }
    if let Some(body) = token.strip_prefix("ghs_") {
        // GitHub's May 2026 guidance treats these as opaque, variable-length
        // tokens; new installation tokens contain underscores, hyphens and dots.
        return body.len() >= 36
            && body
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    }
    for prefix in ["sk_test_", "sk_live_", "rk_test_", "rk_live_"] {
        if let Some(body) = token.strip_prefix(prefix) {
            return body.len() >= 24 && alphanumeric(body);
        }
    }
    false
}
