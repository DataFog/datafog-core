//! Explicit literal matching, shared by every SDK.
use super::*;
use regex::RegexBuilder;

const ENTITY: &str = "DENYLIST";
const DETECTOR: &str = "datafog-core/denylist";

/// Conditions on source characters immediately outside a literal phrase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DenyBoundary {
    /// Both neighbors must be absent or neither a Unicode letter/number nor `_`.
    #[default]
    Word,
    /// Match inside other words too.
    Substring,
}

/// A validated, compiled literal rule. No regex syntax or text normalization.
#[derive(Debug, Clone)]
pub struct LiteralDenyRule {
    phrase: String,
    case_sensitive: bool,
    boundary: DenyBoundary,
    pattern: Regex,
}

impl PartialEq for LiteralDenyRule {
    fn eq(&self, other: &Self) -> bool {
        self.phrase == other.phrase
            && self.case_sensitive == other.case_sensitive
            && self.boundary == other.boundary
    }
}
impl Eq for LiteralDenyRule {}

impl LiteralDenyRule {
    /// Compile a literal phrase. Case-insensitive matching uses Unicode simple
    /// case folding, without multi-character expansions (for example ß to SS).
    pub fn new(
        phrase: impl Into<String>,
        case_sensitive: bool,
        boundary: DenyBoundary,
    ) -> Result<Self, PrivacyError> {
        Self::at_path(phrase.into(), case_sensitive, boundary, "/deny/0/phrase")
    }

    fn at_path(
        phrase: String,
        case_sensitive: bool,
        boundary: DenyBoundary,
        path: &str,
    ) -> Result<Self, PrivacyError> {
        if phrase.trim().is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                path,
                "deny phrase must not be empty or whitespace-only",
            ));
        }
        let pattern = RegexBuilder::new(&regex::escape(&phrase))
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|_| {
                PrivacyError::invalid_configuration(
                    PrivacyErrorReason::InvalidValue,
                    path,
                    "deny phrase exceeds matcher compilation limits",
                )
            })?;
        Ok(Self {
            phrase,
            case_sensitive,
            boundary,
            pattern,
        })
    }

    pub fn phrase(&self) -> &str {
        &self.phrase
    }
    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }
    pub fn boundary(&self) -> DenyBoundary {
        self.boundary
    }
}

pub(super) fn parse_rules(value: &serde_json::Value) -> Result<Vec<LiteralDenyRule>, PrivacyError> {
    let rules = require_array(value, "/deny", "deny must be an array")?;
    rules
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("/deny/{index}");
            let object = require_object(value, &path, "deny rule must be an object")?;
            reject_unknown_fields(object, &["phrase", "case_sensitive", "boundary"], &path)?;
            let phrase_path = format!("{path}/phrase");
            let phrase = object.get("phrase").ok_or_else(|| {
                PrivacyError::invalid_configuration(
                    PrivacyErrorReason::MissingField,
                    &phrase_path,
                    "deny rule requires a phrase",
                )
            })?;
            let phrase = require_string(phrase, &phrase_path, "deny phrase must be a string")?;
            let case_sensitive = match object.get("case_sensitive") {
                None => true,
                Some(value) => value.as_bool().ok_or_else(|| {
                    PrivacyError::invalid_configuration(
                        PrivacyErrorReason::InvalidType,
                        format!("{path}/case_sensitive"),
                        "case_sensitive must be a boolean",
                    )
                })?,
            };
            let boundary = match object.get("boundary") {
                None => DenyBoundary::Word,
                Some(value) => {
                    let boundary_path = format!("{path}/boundary");
                    match require_string(value, &boundary_path, "deny boundary must be a string")?
                        .as_str()
                    {
                        "word" => DenyBoundary::Word,
                        "substring" => DenyBoundary::Substring,
                        _ => {
                            return Err(PrivacyError::invalid_configuration(
                                PrivacyErrorReason::InvalidValue,
                                boundary_path,
                                "deny boundary must be word or substring",
                            ));
                        }
                    }
                }
            };
            LiteralDenyRule::at_path(phrase, case_sensitive, boundary, &phrase_path)
        })
        .collect()
}

pub(crate) fn is_explicit_deny(finding: &Finding) -> bool {
    finding.entity_type == ENTITY && finding.detector_name == DETECTOR
}

fn word_character(character: char) -> bool {
    // Rust's is_alphabetic also includes some combining marks; the public
    // boundary contract deliberately uses general categories Letter/Number.
    static WORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_]$").expect("constant word-character regex"));
    WORD.is_match(character.encode_utf8(&mut [0; 4]))
}

pub(super) fn append_findings(text: &str, rules: &[LiteralDenyRule], findings: &mut Vec<Finding>) {
    if rules.is_empty() {
        return;
    }
    let mut index = TextIndex::new(text);
    let mut spans = BTreeSet::new();
    for rule in rules {
        // Restart at the next source character, rather than the end of a match,
        // so overlapping occurrences ("aba" in "ababa") retain full coverage.
        let mut offset = 0;
        while let Some(found) = rule.pattern.find_at(text, offset) {
            let start = found.start();
            let end = found.end();
            if rule.boundary == DenyBoundary::Substring
                || (text[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !word_character(c))
                    && text[end..]
                        .chars()
                        .next()
                        .is_none_or(|c| !word_character(c)))
            {
                spans.insert((start, end));
            }
            offset = start + text[start..].chars().next().map_or(0, char::len_utf8);
            if offset >= text.len() {
                break;
            }
        }
    }
    for (start, end) in spans {
        findings.push(Finding {
            entity_type: ENTITY.into(),
            matched_text: text[start..end].into(),
            byte_range: TextRange { start, end },
            codepoint_range: TextRange {
                start: index.codepoint_offset(start),
                end: index.codepoint_offset(end),
            },
            confidence: None,
            detector_name: DETECTOR.into(),
            detector_version: Some(env!("CARGO_PKG_VERSION").into()),
        });
    }
    findings.sort_by(|a, b| {
        (
            a.byte_range.start,
            std::cmp::Reverse(a.byte_range.end),
            &a.entity_type,
        )
            .cmp(&(
                b.byte_range.start,
                std::cmp::Reverse(b.byte_range.end),
                &b.entity_type,
            ))
    });
}
