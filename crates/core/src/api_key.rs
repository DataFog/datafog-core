//! Provider-prefixed credential shapes, without network or cryptographic validation.
use crate::{Candidate, Label};
use regex::Regex;
use std::sync::LazyLock;

// Include Unicode identifier characters and unsupported token punctuation so a
// plausible prefix inside a longer identifier cannot produce a partial finding.
static TOKEN_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\w.+/\-~]+").expect("static API key token regex must compile"));

// Every supported token starts with one of these case-sensitive prefixes, so a
// literal search replaces a walk over every word of the input.
static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"gh[pours]_|github_pat_|[sr]k_(?:test|live)_")
        .expect("static API key prefix regex must compile")
});

pub(super) fn detect(text: &str, candidates: &mut Vec<Candidate>) {
    for prefix in PREFIX.find_iter(text) {
        let start = prefix.start();
        // A token must begin its maximal run; a prefix inside a longer run is
        // never a supported shape because validation strips the prefix itself.
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(|previous| TOKEN_RUN.is_match(previous.encode_utf8(&mut [0; 4])))
        {
            continue;
        }
        let Some(matched) = TOKEN_RUN.find_at(text, start) else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    // The original whole-text walk, kept as the reference for the prefix search.
    fn reference(text: &str) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        for matched in TOKEN_RUN.find_iter(text) {
            if text.as_bytes().get(matched.end()) == Some(&b'=') {
                continue;
            }
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
        candidates
    }

    #[test]
    fn prefix_search_matches_the_whole_text_walk() {
        let body = "abcdefghijklmnopqrstuvwxyz0123456789";
        let tokens = [
            format!("ghp_{body}"),
            format!("ghs_{body}.x-y_z"),
            format!("ghr_{body}{body}abcd"),
            format!("github_pat_{}_{}", "a".repeat(22), "b".repeat(59)),
            format!("sk_live_{}", "c".repeat(24)),
            format!("rk_test_{}", "d".repeat(30)),
        ];
        let wrappers = [
            ("", ""),
            ("x", ""),
            ("é", ""),
            ("中", ""),
            ("_", ""),
            ("-", ""),
            (".", ""),
            ("~", ""),
            ("+", ""),
            ("/", ""),
            (" ", "."),
            ("(", ")"),
            ("\"", "\""),
            ("key=", ""),
            ("", "="),
            ("", ".."),
            ("", "é"),
            ("", "x"),
            ("\u{0301}", ""),
            ("ghp_", ""),
            ("sk_test_", ""),
        ];
        for token in &tokens {
            for (before, after) in wrappers {
                let text = format!("see {before}{token}{after} and {token}");
                assert_eq!(
                    {
                        let mut found = Vec::new();
                        detect(&text, &mut found);
                        found
                    },
                    reference(&text),
                    "{text}"
                );
            }
        }
        let noisy = format!(
            "ghp_ gh_ ghp_x github_pat_ sk_test_ rk_live_short {} ghp_{body}ghp_{body}",
            "word ".repeat(50)
        );
        let mut found = Vec::new();
        detect(&noisy, &mut found);
        assert_eq!(found, reference(&noisy));
    }
}
