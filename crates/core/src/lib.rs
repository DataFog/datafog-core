//! Core PII scanning API for DataFog.
mod offsets;
pub mod structured;
use base64::Engine;
use hmac::{Hmac, Mac};
pub use offsets::TextIndex;
use regex::{Regex, RegexSet, RegexSetBuilder};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::LazyLock;
use zeroize::Zeroize;

/// A zero-based, end-exclusive range in the coordinate system named by its
/// containing field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Inclusive start offset.
    pub start: usize,
    /// Exclusive end offset.
    pub end: usize,
}

/// Returned when a UTF-8 byte range is invalid for the supplied text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf16RangeError;

impl std::fmt::Display for Utf16RangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("UTF-8 byte range is invalid for the supplied text")
    }
}

impl std::error::Error for Utf16RangeError {}

/// Convert a UTF-8 byte range into zero-based, end-exclusive UTF-16 code-unit
/// offsets for JavaScript consumers.
pub fn utf16_range(text: &str, byte_range: TextRange) -> Result<TextRange, Utf16RangeError> {
    validate_utf8_range(text, byte_range)?;
    Ok(TextRange {
        start: text[..byte_range.start].encode_utf16().count(),
        end: text[..byte_range.end].encode_utf16().count(),
    })
}

fn validate_utf8_range(text: &str, byte_range: TextRange) -> Result<(), Utf16RangeError> {
    if byte_range.start > byte_range.end {
        return Err(Utf16RangeError);
    }
    if byte_range.end > text.len() {
        return Err(Utf16RangeError);
    }
    if !text.is_char_boundary(byte_range.start) || !text.is_char_boundary(byte_range.end) {
        return Err(Utf16RangeError);
    }

    Ok(())
}

/// A piece of potentially sensitive content detected in an input string.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Canonical PII type, such as `EMAIL` or `SSN`.
    pub entity_type: String,
    /// Exact substring matched in the original input.
    pub matched_text: String,
    /// Range in UTF-8 bytes.
    pub byte_range: TextRange,
    /// Range in Unicode code points.
    pub codepoint_range: TextRange,
    /// Detection confidence in `0.0..=1.0`, when the detector produces one.
    pub confidence: Option<f32>,
    /// Stable name of the detector that produced this finding.
    pub detector_name: String,
    /// Version of the detector implementation, when available.
    pub detector_version: Option<String>,
}

/// A privacy transformation applied to a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformationStrategy {
    /// Replace the finding with its unnumbered entity-type placeholder.
    Redact,
    /// Delete the exact finding span.
    Remove,
    /// Replace non-revealed code points with a configured character.
    Mask(MaskConfig),
    /// Replace the exact finding value with a deterministic keyed pseudonym.
    Pseudonymize(PseudonymizeConfig),
    /// Replace the exact finding value with an opaque reversible token.
    Tokenize(TokenizeConfig),
}

/// Provider-owned token profile selected by a tokenization request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TokenizeConfig {
    token_ref: String,
}

impl TokenizeConfig {
    /// Create a validated token profile selector.
    pub fn new(token_ref: impl Into<String>) -> Result<Self, PrivacyError> {
        let token_ref = token_ref.into();
        if token_ref.trim().is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                "/token_ref",
                "token_ref must not be empty or whitespace-only",
            ));
        }
        Ok(Self { token_ref })
    }

    /// Provider-defined token profile reference.
    pub fn token_ref(&self) -> &str {
        &self.token_ref
    }
}

/// Non-secret request context used for authorization by token providers.
#[derive(Clone, PartialEq, Eq)]
pub struct PrivacyContext {
    scope: String,
}

impl std::fmt::Debug for PrivacyContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivacyContext")
            .field("scope", &"[REDACTED]")
            .finish()
    }
}

impl PrivacyContext {
    /// Create exact, case-sensitive request context without normalization.
    pub fn new(scope: impl Into<String>) -> Result<Self, PrivacyError> {
        let scope = scope.into();
        if scope.trim().is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                "/context/scope",
                "scope must not be empty or whitespace-only",
            ));
        }
        Ok(Self { scope })
    }

    /// Exact provider authorization scope.
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

/// Key selector for deterministic one-way pseudonymization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PseudonymizeConfig {
    key_ref: String,
    key_version: Option<String>,
}

/// Reason a pseudonymization configuration is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudonymizeConfigError {
    /// The key reference is empty or contains only whitespace.
    EmptyKeyRef,
    /// The supplied key version is empty or contains only whitespace.
    EmptyKeyVersion,
}

impl std::fmt::Display for PseudonymizeConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyKeyRef => formatter.write_str("key reference must not be empty"),
            Self::EmptyKeyVersion => formatter.write_str("key version must not be empty"),
        }
    }
}

impl std::error::Error for PseudonymizeConfigError {}

impl PseudonymizeConfig {
    /// Create a validated pseudonymization key selector.
    pub fn new(
        key_ref: impl Into<String>,
        key_version: Option<String>,
    ) -> Result<Self, PseudonymizeConfigError> {
        let key_ref = key_ref.into();
        if key_ref.trim().is_empty() {
            return Err(PseudonymizeConfigError::EmptyKeyRef);
        }
        if key_version
            .as_ref()
            .is_some_and(|version| version.trim().is_empty())
        {
            return Err(PseudonymizeConfigError::EmptyKeyVersion);
        }
        Ok(Self {
            key_ref,
            key_version,
        })
    }

    /// Provider-specific key reference.
    pub fn key_ref(&self) -> &str {
        &self.key_ref
    }

    /// Requested key version or alias, when supplied.
    pub fn key_version(&self) -> Option<&str> {
        self.key_version.as_deref()
    }
}

const MAX_REGEX_RULES: usize = 100;
const MAX_REGEX_PATTERN_BYTES: usize = 1024;
const MAX_REGEX_SOURCE_BYTES: usize = 10 * 1024;
const MAX_COMPILED_REGEX_GROUP_BYTES: usize = 1024 * 1024;

/// One entity-scoped full-match regex allowlist rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegexAllowRule {
    pattern: String,
    case_sensitive: bool,
}

impl RegexAllowRule {
    /// Create a rule. Complete validation occurs when the rule is added to a
    /// transformation configuration.
    pub fn new(pattern: impl Into<String>, case_sensitive: bool) -> Self {
        Self {
            pattern: pattern.into(),
            case_sensitive,
        }
    }

    /// Regex source supplied by the caller.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Whether matching preserves case distinctions.
    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }
}

#[derive(Debug, Clone)]
struct CompiledRegexGroup {
    patterns: RegexSet,
}

/// Validated configuration for transforming caller-supplied findings.
#[derive(Debug, Clone)]
pub struct TransformationConfig {
    default: TransformationStrategy,
    entities: Option<BTreeSet<String>>,
    overrides: BTreeMap<String, TransformationStrategy>,
    exact_allowlists: BTreeMap<String, BTreeSet<String>>,
    regex_allowlists: BTreeMap<String, Vec<RegexAllowRule>>,
    compiled_regex_allowlists: BTreeMap<String, Vec<CompiledRegexGroup>>,
}

impl TransformationConfig {
    /// Create a configuration which applies one default strategy to all
    /// supplied findings.
    pub fn new(default: TransformationStrategy) -> Self {
        Self {
            default,
            entities: None,
            overrides: BTreeMap::new(),
            exact_allowlists: BTreeMap::new(),
            regex_allowlists: BTreeMap::new(),
            compiled_regex_allowlists: BTreeMap::new(),
        }
    }

    /// Restrict transformation to a non-empty set of exact entity types.
    pub fn with_entities(mut self, entities: Vec<String>) -> Result<Self, PrivacyError> {
        if entities.is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                "/entities",
                "entities must contain at least one entity type",
            ));
        }
        let mut selected = BTreeSet::new();
        for (index, entity) in entities.into_iter().enumerate() {
            validate_entity_name(&entity, &format!("/entities/{index}"))?;
            if !selected.insert(entity) {
                return Err(PrivacyError::invalid_configuration(
                    PrivacyErrorReason::DuplicateValue,
                    format!("/entities/{index}"),
                    "entity selection contains a duplicate entity type",
                ));
            }
        }
        self.entities = Some(selected);
        Ok(self)
    }

    /// Add an exact, case-sensitive entity strategy override.
    pub fn with_override(
        mut self,
        entity_type: impl Into<String>,
        strategy: TransformationStrategy,
    ) -> Result<Self, PrivacyError> {
        let entity_type = entity_type.into();
        let path = format!("/overrides/{}", json_pointer_segment(&entity_type));
        validate_entity_name(&entity_type, &path)?;
        if self.overrides.insert(entity_type, strategy).is_some() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::DuplicateValue,
                path,
                "entity type has more than one strategy override",
            ));
        }
        Ok(self)
    }

    /// Add exact, case-sensitive allowlist values for one entity type.
    pub fn with_exact_allowlist(
        mut self,
        entity_type: impl Into<String>,
        values: Vec<String>,
    ) -> Result<Self, PrivacyError> {
        let entity_type = entity_type.into();
        let path = format!("/allow/exact/{}", json_pointer_segment(&entity_type));
        validate_entity_name(&entity_type, &path)?;
        if values.is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                path,
                "exact allowlist must contain at least one value",
            ));
        }
        if self.exact_allowlists.contains_key(&entity_type) {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::DuplicateValue,
                path,
                "entity type has more than one exact allowlist",
            ));
        }
        let mut deduplicated = BTreeSet::new();
        for (index, value) in values.into_iter().enumerate() {
            if value.is_empty() {
                return Err(PrivacyError::invalid_configuration(
                    PrivacyErrorReason::EmptyValue,
                    format!("{path}/{index}"),
                    "exact allowlist values must not be empty",
                ));
            }
            deduplicated.insert(value);
        }
        self.exact_allowlists.insert(entity_type, deduplicated);
        Ok(self)
    }

    /// Add full-match regex allowlist rules for one entity type.
    pub fn with_regex_allowlist(
        mut self,
        entity_type: impl Into<String>,
        rules: Vec<RegexAllowRule>,
    ) -> Result<Self, PrivacyError> {
        let entity_type = entity_type.into();
        let path = format!("/allow/regex/{}", json_pointer_segment(&entity_type));
        validate_entity_name(&entity_type, &path)?;
        if rules.is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                path,
                "regex allowlist must contain at least one rule",
            ));
        }
        if self.regex_allowlists.contains_key(&entity_type) {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::DuplicateValue,
                path,
                "entity type has more than one regex allowlist",
            ));
        }
        let rules = rules
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.regex_allowlists.insert(entity_type, rules);
        self.compile_regex_allowlists()?;
        Ok(self)
    }

    fn includes(&self, finding: &Finding) -> bool {
        self.entities
            .as_ref()
            .is_none_or(|entities| entities.contains(&finding.entity_type))
    }

    fn allows(&self, finding: &Finding) -> bool {
        self.exact_allowlists
            .get(&finding.entity_type)
            .is_some_and(|values| values.contains(&finding.matched_text))
            || self
                .compiled_regex_allowlists
                .get(&finding.entity_type)
                .is_some_and(|groups| {
                    groups
                        .iter()
                        .any(|group| group.patterns.is_match(&finding.matched_text))
                })
    }

    fn strategy_for(&self, finding: &Finding) -> &TransformationStrategy {
        self.overrides
            .get(&finding.entity_type)
            .unwrap_or(&self.default)
    }

    fn strategy_path_for(&self, finding: &Finding) -> String {
        if self.overrides.contains_key(&finding.entity_type) {
            format!(
                "/overrides/{}/key_ref",
                json_pointer_segment(&finding.entity_type)
            )
        } else {
            "/default/key_ref".to_owned()
        }
    }

    fn compile_regex_allowlists(&mut self) -> Result<(), PrivacyError> {
        let rule_count: usize = self.regex_allowlists.values().map(Vec::len).sum();
        if rule_count > MAX_REGEX_RULES {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::LimitExceeded,
                "/allow/regex",
                "regex allowlists exceed the maximum of 100 deduplicated rules",
            ));
        }

        let mut source_bytes = 0;
        for (entity_type, rules) in &self.regex_allowlists {
            let entity_path = format!("/allow/regex/{}", json_pointer_segment(entity_type));
            for (index, rule) in rules.iter().enumerate() {
                let pattern_path = format!("{entity_path}/{index}/pattern");
                if rule.pattern.is_empty() {
                    return Err(PrivacyError::invalid_configuration(
                        PrivacyErrorReason::EmptyValue,
                        pattern_path,
                        "regex pattern must not be empty",
                    ));
                }
                if rule.pattern.len() > MAX_REGEX_PATTERN_BYTES {
                    return Err(PrivacyError::invalid_configuration(
                        PrivacyErrorReason::LimitExceeded,
                        pattern_path,
                        "regex pattern exceeds the 1 KiB source limit",
                    ));
                }
                source_bytes += rule.pattern.len();
            }
        }
        if source_bytes > MAX_REGEX_SOURCE_BYTES {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::LimitExceeded,
                "/allow/regex",
                "regex allowlists exceed the 10 KiB aggregate source limit",
            ));
        }

        let mut compiled = BTreeMap::new();
        for (entity_type, rules) in &self.regex_allowlists {
            let entity_path = format!("/allow/regex/{}", json_pointer_segment(entity_type));
            let mut by_case = BTreeMap::<bool, Vec<String>>::new();
            for rule in rules {
                by_case
                    .entry(rule.case_sensitive)
                    .or_default()
                    .push(format!(r"\A(?:{})\z", rule.pattern));
            }

            let mut groups = Vec::new();
            for (case_sensitive, patterns) in by_case {
                let set = RegexSetBuilder::new(patterns)
                    .case_insensitive(!case_sensitive)
                    .size_limit(MAX_COMPILED_REGEX_GROUP_BYTES)
                    .dfa_size_limit(MAX_COMPILED_REGEX_GROUP_BYTES)
                    .build()
                    .map_err(|error| {
                        let reason = match error {
                            regex::Error::CompiledTooBig(_) => PrivacyErrorReason::LimitExceeded,
                            _ => PrivacyErrorReason::InvalidRegex,
                        };
                        PrivacyError::invalid_configuration(
                            reason,
                            &entity_path,
                            "regex allowlist contains an invalid or over-limit pattern",
                        )
                    })?;
                groups.push(CompiledRegexGroup { patterns: set });
            }
            compiled.insert(entity_type.clone(), groups);
        }
        self.compiled_regex_allowlists = compiled;
        Ok(())
    }
}

fn validate_entity_name(entity_type: &str, path: &str) -> Result<(), PrivacyError> {
    if entity_type.trim().is_empty() {
        return Err(PrivacyError::invalid_configuration(
            PrivacyErrorReason::EmptyValue,
            path,
            "entity type must not be empty or whitespace-only",
        ));
    }
    Ok(())
}

fn json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Scanner configuration. Current built-in detectors share the same execution
/// path; locale is retained for detector-specific routing as coverage expands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanConfig {
    locale: Option<String>,
}

impl ScanConfig {
    /// Create scanner configuration using detector defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a non-empty locale identifier.
    pub fn with_locale(mut self, locale: impl Into<String>) -> Result<Self, PrivacyError> {
        let locale = locale.into();
        if locale.trim().is_empty() {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::EmptyValue,
                "/locale",
                "scan locale must not be empty or whitespace-only",
            ));
        }
        self.locale = Some(locale);
        Ok(self)
    }

    /// Configured locale, when explicitly supplied.
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }
}

/// Configuration for the explicit scan-then-transform convenience operation.
#[derive(Debug, Clone)]
pub struct ScanAndTransformConfig {
    scan: ScanConfig,
    transform: TransformationConfig,
}

impl ScanAndTransformConfig {
    /// Use scanner defaults with a required transformation configuration.
    pub fn new(transform: TransformationConfig) -> Self {
        Self {
            scan: ScanConfig::default(),
            transform,
        }
    }

    /// Supply scanner configuration.
    pub fn with_scan(mut self, scan: ScanConfig) -> Self {
        self.scan = scan;
        self
    }

    /// Scanner settings.
    pub fn scan_config(&self) -> &ScanConfig {
        &self.scan
    }

    /// Transformation settings.
    pub fn transformation_config(&self) -> &TransformationConfig {
        &self.transform
    }
}

/// Parse the canonical serialized transformation envelope.
pub fn parse_transformation_config(
    value: &serde_json::Value,
) -> Result<TransformationConfig, PrivacyError> {
    let object = require_object(value, "", "transformation configuration must be an object")?;
    reject_unknown_fields(object, &["default", "entities", "overrides", "allow"], "")?;
    let default = object.get("default").ok_or_else(|| {
        PrivacyError::invalid_configuration(
            PrivacyErrorReason::MissingField,
            "/default",
            "transformation configuration requires default",
        )
    })?;
    let mut config = TransformationConfig::new(parse_strategy_config(default, "/default")?);

    if let Some(entities) = object.get("entities") {
        let entities = require_array(entities, "/entities", "entities must be an array")?;
        let mut parsed = Vec::with_capacity(entities.len());
        for (index, entity) in entities.iter().enumerate() {
            parsed.push(require_string(
                entity,
                &format!("/entities/{index}"),
                "entity type must be a string",
            )?);
        }
        config = config.with_entities(parsed)?;
    }

    if let Some(overrides) = object.get("overrides") {
        let overrides = require_object(
            overrides,
            "/overrides",
            "strategy overrides must be an object",
        )?;
        for (entity_type, strategy) in overrides {
            let path = format!("/overrides/{}", json_pointer_segment(entity_type));
            config = config.with_override(entity_type, parse_strategy_config(strategy, &path)?)?;
        }
    }

    if let Some(allow) = object.get("allow") {
        let allow = require_object(allow, "/allow", "allow must be an object")?;
        reject_unknown_fields(allow, &["exact", "regex"], "/allow")?;
        if let Some(exact) = allow.get("exact") {
            let exact =
                require_object(exact, "/allow/exact", "exact allowlists must be an object")?;
            for (entity_type, values) in exact {
                let entity_path = format!("/allow/exact/{}", json_pointer_segment(entity_type));
                let values =
                    require_array(values, &entity_path, "exact allowlist must be an array")?;
                let mut parsed = Vec::with_capacity(values.len());
                for (index, value) in values.iter().enumerate() {
                    parsed.push(require_string(
                        value,
                        &format!("{entity_path}/{index}"),
                        "exact allowlist value must be a string",
                    )?);
                }
                config = config.with_exact_allowlist(entity_type, parsed)?;
            }
        }
        if let Some(regex) = allow.get("regex") {
            let regex =
                require_object(regex, "/allow/regex", "regex allowlists must be an object")?;
            for (entity_type, rules) in regex {
                let entity_path = format!("/allow/regex/{}", json_pointer_segment(entity_type));
                let rules = require_array(rules, &entity_path, "regex allowlist must be an array")?;
                let mut parsed = Vec::with_capacity(rules.len());
                for (index, rule) in rules.iter().enumerate() {
                    let path = format!("{entity_path}/{index}");
                    let rule = require_object(rule, &path, "regex rule must be an object")?;
                    reject_unknown_fields(rule, &["pattern", "case_sensitive"], &path)?;
                    let pattern = rule.get("pattern").ok_or_else(|| {
                        PrivacyError::invalid_configuration(
                            PrivacyErrorReason::MissingField,
                            format!("{path}/pattern"),
                            "regex rule requires pattern",
                        )
                    })?;
                    let pattern = require_string(
                        pattern,
                        &format!("{path}/pattern"),
                        "regex pattern must be a string",
                    )?;
                    let case_sensitive = match rule.get("case_sensitive") {
                        None => true,
                        Some(serde_json::Value::Bool(value)) => *value,
                        Some(_) => {
                            return Err(PrivacyError::invalid_configuration(
                                PrivacyErrorReason::InvalidType,
                                format!("{path}/case_sensitive"),
                                "case_sensitive must be a boolean",
                            ));
                        }
                    };
                    parsed.push(RegexAllowRule::new(pattern, case_sensitive));
                }
                config = config.with_regex_allowlist(entity_type, parsed)?;
            }
        }
    }
    Ok(config)
}

/// Parse the canonical divided scan-and-transform envelope.
pub fn parse_scan_and_transform_config(
    value: &serde_json::Value,
) -> Result<ScanAndTransformConfig, PrivacyError> {
    let object = require_object(
        value,
        "",
        "scan-and-transform configuration must be an object",
    )?;
    reject_unknown_fields(object, &["scan", "transform"], "")?;
    let transform = object.get("transform").ok_or_else(|| {
        PrivacyError::invalid_configuration(
            PrivacyErrorReason::MissingField,
            "/transform",
            "scan-and-transform configuration requires transform",
        )
    })?;
    let transform =
        parse_transformation_config(transform).map_err(|error| error.prefixed("/transform"))?;
    let mut combined = ScanAndTransformConfig::new(transform);
    if let Some(scan) = object.get("scan") {
        let scan_config = parse_scan_config(scan).map_err(|error| error.prefixed("/scan"))?;
        combined = combined.with_scan(scan_config);
    }
    Ok(combined)
}

/// Parse canonical scanner configuration.
pub fn parse_scan_config(value: &serde_json::Value) -> Result<ScanConfig, PrivacyError> {
    let object = require_object(value, "", "scan configuration must be an object")?;
    reject_unknown_fields(object, &["locale"], "")?;
    let mut config = ScanConfig::new();
    if let Some(locale) = object.get("locale") {
        let locale = require_string(locale, "/locale", "scan locale must be a string")?;
        config = config.with_locale(locale)?;
    }
    Ok(config)
}

/// Parse the canonical request-level authorization context.
pub fn parse_privacy_context(value: &serde_json::Value) -> Result<PrivacyContext, PrivacyError> {
    let object = require_object(value, "/context", "context must be an object")?;
    reject_unknown_fields(object, &["scope"], "/context")?;
    let scope = object.get("scope").ok_or_else(|| {
        PrivacyError::invalid_configuration(
            PrivacyErrorReason::MissingField,
            "/context/scope",
            "token operations require scope",
        )
    })?;
    let scope = require_string(scope, "/context/scope", "scope must be a string")?;
    PrivacyContext::new(scope)
}

fn parse_strategy_config(
    value: &serde_json::Value,
    path: &str,
) -> Result<TransformationStrategy, PrivacyError> {
    let object = require_object(value, path, "strategy configuration must be an object")?;
    let strategy_path = format!("{path}/strategy");
    let strategy = object.get("strategy").ok_or_else(|| {
        PrivacyError::invalid_configuration(
            PrivacyErrorReason::MissingField,
            &strategy_path,
            "strategy configuration requires strategy",
        )
    })?;
    let strategy = require_string(strategy, &strategy_path, "strategy must be a string")?;
    match strategy.as_str() {
        "redact" => {
            reject_unknown_fields(object, &["strategy"], path)?;
            Ok(TransformationStrategy::Redact)
        }
        "remove" => {
            reject_unknown_fields(object, &["strategy"], path)?;
            Ok(TransformationStrategy::Remove)
        }
        "mask" => {
            reject_unknown_fields(object, &["strategy", "character", "reveal"], path)?;
            let character = match object.get("character") {
                None => '*',
                Some(value) => {
                    let value = require_string(
                        value,
                        &format!("{path}/character"),
                        "mask character must be a string",
                    )?;
                    let mut characters = value.chars();
                    characters
                        .next()
                        .filter(|_| characters.next().is_none())
                        .ok_or_else(|| {
                            PrivacyError::invalid_configuration(
                                PrivacyErrorReason::InvalidValue,
                                format!("{path}/character"),
                                "mask character must contain exactly one code point",
                            )
                        })?
                }
            };
            let reveal = match object.get("reveal") {
                None => MaskReveal::None,
                Some(value) => {
                    let reveal_path = format!("{path}/reveal");
                    let reveal =
                        require_object(value, &reveal_path, "mask reveal must be an object")?;
                    reject_unknown_fields(reveal, &["direction", "count"], &reveal_path)?;
                    let direction = reveal.get("direction").ok_or_else(|| {
                        PrivacyError::invalid_configuration(
                            PrivacyErrorReason::MissingField,
                            format!("{reveal_path}/direction"),
                            "mask reveal requires direction",
                        )
                    })?;
                    let direction = require_string(
                        direction,
                        &format!("{reveal_path}/direction"),
                        "mask reveal direction must be a string",
                    )?;
                    let count = reveal.get("count").ok_or_else(|| {
                        PrivacyError::invalid_configuration(
                            PrivacyErrorReason::MissingField,
                            format!("{reveal_path}/count"),
                            "mask reveal requires count",
                        )
                    })?;
                    let count = count
                        .as_u64()
                        .and_then(|count| usize::try_from(count).ok())
                        .ok_or_else(|| {
                            PrivacyError::invalid_configuration(
                                PrivacyErrorReason::InvalidValue,
                                format!("{reveal_path}/count"),
                                "mask reveal count must be a non-negative integer",
                            )
                        })?;
                    match direction.as_str() {
                        "first" => MaskReveal::First(count),
                        "last" => MaskReveal::Last(count),
                        _ => {
                            return Err(PrivacyError::invalid_configuration(
                                PrivacyErrorReason::InvalidValue,
                                format!("{reveal_path}/direction"),
                                "mask reveal direction must be first or last",
                            ));
                        }
                    }
                }
            };
            MaskConfig::new(character, reveal)
                .map(TransformationStrategy::Mask)
                .map_err(|_| {
                    PrivacyError::invalid_configuration(
                        PrivacyErrorReason::InvalidValue,
                        format!("{path}/character"),
                        "mask character must not be whitespace or a control character",
                    )
                })
        }
        "pseudonymize" => {
            reject_unknown_fields(object, &["strategy", "key_ref", "key_version"], path)?;
            let key_ref_path = format!("{path}/key_ref");
            let key_ref = object.get("key_ref").ok_or_else(|| {
                PrivacyError::invalid_configuration(
                    PrivacyErrorReason::MissingField,
                    &key_ref_path,
                    "pseudonymization requires key_ref",
                )
            })?;
            let key_ref = require_string(key_ref, &key_ref_path, "key_ref must be a string")?;
            let key_version = object
                .get("key_version")
                .map(|value| {
                    require_string(
                        value,
                        &format!("{path}/key_version"),
                        "key_version must be a string",
                    )
                })
                .transpose()?;
            PseudonymizeConfig::new(key_ref, key_version)
                .map(TransformationStrategy::Pseudonymize)
                .map_err(|error| match error {
                    PseudonymizeConfigError::EmptyKeyRef => PrivacyError::invalid_configuration(
                        PrivacyErrorReason::EmptyValue,
                        key_ref_path,
                        "key_ref must not be empty or whitespace-only",
                    ),
                    PseudonymizeConfigError::EmptyKeyVersion => {
                        PrivacyError::invalid_configuration(
                            PrivacyErrorReason::EmptyValue,
                            format!("{path}/key_version"),
                            "key_version must not be empty or whitespace-only",
                        )
                    }
                })
        }
        "tokenize" => {
            reject_unknown_fields(object, &["strategy", "token_ref"], path)?;
            let token_ref_path = format!("{path}/token_ref");
            let token_ref = object.get("token_ref").ok_or_else(|| {
                PrivacyError::invalid_configuration(
                    PrivacyErrorReason::MissingField,
                    &token_ref_path,
                    "tokenization requires token_ref",
                )
            })?;
            let token_ref =
                require_string(token_ref, &token_ref_path, "token_ref must be a string")?;
            TokenizeConfig::new(token_ref)
                .map(TransformationStrategy::Tokenize)
                .map_err(|_| {
                    PrivacyError::invalid_configuration(
                        PrivacyErrorReason::EmptyValue,
                        token_ref_path,
                        "token_ref must not be empty or whitespace-only",
                    )
                })
        }
        _ => Err(PrivacyError::invalid_configuration(
            PrivacyErrorReason::InvalidValue,
            strategy_path,
            "strategy must be redact, mask, remove, pseudonymize, or tokenize",
        )),
    }
}

fn require_object<'a>(
    value: &'a serde_json::Value,
    path: &str,
    message: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, PrivacyError> {
    value.as_object().ok_or_else(|| {
        PrivacyError::invalid_configuration(PrivacyErrorReason::InvalidType, path, message)
    })
}

fn require_array<'a>(
    value: &'a serde_json::Value,
    path: &str,
    message: &str,
) -> Result<&'a [serde_json::Value], PrivacyError> {
    value.as_array().map(Vec::as_slice).ok_or_else(|| {
        PrivacyError::invalid_configuration(PrivacyErrorReason::InvalidType, path, message)
    })
}

fn require_string(
    value: &serde_json::Value,
    path: &str,
    message: &str,
) -> Result<String, PrivacyError> {
    value.as_str().map(str::to_owned).ok_or_else(|| {
        PrivacyError::invalid_configuration(PrivacyErrorReason::InvalidType, path, message)
    })
}

fn reject_unknown_fields(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    path: &str,
) -> Result<(), PrivacyError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(PrivacyError::invalid_configuration(
                PrivacyErrorReason::UnknownField,
                format!("{path}/{}", json_pointer_segment(key)),
                "configuration contains an unknown field",
            ));
        }
    }
    Ok(())
}

/// Configuration for character masking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskConfig {
    character: char,
    reveal: MaskReveal,
}

/// Portion of a finding preserved by character masking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskReveal {
    /// Reveal no source code points.
    None,
    /// Reveal the requested number of leading code points.
    First(usize),
    /// Reveal the requested number of trailing code points.
    Last(usize),
}

/// Reason a masking configuration is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskConfigError {
    /// The masking character is whitespace or a control character.
    InvalidCharacter,
}

impl std::fmt::Display for MaskConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCharacter => {
                formatter.write_str("mask character must not be whitespace or a control character")
            }
        }
    }
}

impl std::error::Error for MaskConfigError {}

impl MaskConfig {
    /// Create a validated masking configuration.
    pub fn new(character: char, reveal: MaskReveal) -> Result<Self, MaskConfigError> {
        if character.is_whitespace() || character.is_control() {
            return Err(MaskConfigError::InvalidCharacter);
        }
        Ok(Self { character, reveal })
    }

    /// Character used to replace hidden source code points.
    pub fn character(self) -> char {
        self.character
    }

    /// Portion of the source finding that remains visible.
    pub fn reveal(self) -> MaskReveal {
        self.reveal
    }
}

impl Default for MaskConfig {
    fn default() -> Self {
        Self {
            character: '*',
            reveal: MaskReveal::None,
        }
    }
}

/// One provider key requested by a prepared transformation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeySelector {
    config: PseudonymizeConfig,
    path: String,
}

impl KeySelector {
    /// Provider-specific key reference.
    pub fn key_ref(&self) -> &str {
        self.config.key_ref()
    }

    /// Requested key version or alias, when supplied.
    pub fn key_version(&self) -> Option<&str> {
        self.config.key_version()
    }

    /// Configuration path used for sanitized provider errors.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Resolved provider response containing short-lived secret key material.
pub struct ResolvedKey {
    key: Vec<u8>,
    resolved_version: String,
}

impl ResolvedKey {
    /// Create a provider response. The manager validates its contents before
    /// any transformation is applied.
    pub fn new(key: Vec<u8>, resolved_version: impl Into<String>) -> Self {
        Self {
            key,
            resolved_version: resolved_version.into(),
        }
    }

    fn key(&self) -> &[u8] {
        &self.key
    }

    /// Concrete provider version used for this response.
    pub fn resolved_version(&self) -> &str {
        &self.resolved_version
    }
}

impl std::fmt::Debug for ResolvedKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedKey")
            .field("key", &"[REDACTED]")
            .field("resolved_version", &self.resolved_version)
            .finish()
    }
}

impl Drop for ResolvedKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Provider failure category independent of any cloud SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProviderErrorKind {
    NotFound,
    AccessDenied,
    Unavailable,
    ProviderError,
}

/// Sanitized failure returned by a key provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyProviderError {
    kind: KeyProviderErrorKind,
}

impl KeyProviderError {
    /// Create a sanitized provider failure.
    pub fn new(kind: KeyProviderErrorKind) -> Self {
        Self { kind }
    }

    /// Stable provider failure category.
    pub fn kind(self) -> KeyProviderErrorKind {
        self.kind
    }
}

impl std::fmt::Display for KeyProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("key provider could not resolve the requested key")
    }
}

impl std::error::Error for KeyProviderError {}

/// Future returned by a vendor-neutral asynchronous key provider.
pub type KeyProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ResolvedKey, KeyProviderError>> + Send + 'a>>;

/// Runtime boundary for resolving pseudonymization key references.
pub trait KeyProvider: Send + Sync {
    /// Whether this value represents an available provider capability.
    fn is_configured(&self) -> bool {
        true
    }

    /// Resolve one key selector. Provider implementations own retries,
    /// timeouts, authentication, decoding, and optional caching.
    fn resolve_key(&self, selector: KeySelector) -> KeyProviderFuture<'_>;
}

/// One source value submitted to a token provider. Equal values remain
/// separate items so providers may issue fresh tokens.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenizeItem {
    id: String,
    exact_value: String,
    token_ref: String,
}

impl TokenizeItem {
    /// Request-local correlation identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Exact selected source value.
    pub fn exact_value(&self) -> &str {
        &self.exact_value
    }

    /// Provider-defined profile reference.
    pub fn token_ref(&self) -> &str {
        &self.token_ref
    }
}

impl std::fmt::Debug for TokenizeItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenizeItem")
            .field("id", &self.id)
            .field("exact_value", &"[REDACTED]")
            .field("token_ref", &self.token_ref)
            .finish()
    }
}

/// Opaque token material returned for one tokenization item.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenizeResult {
    id: String,
    payload: Vec<u8>,
    resolved_version: String,
}

impl TokenizeResult {
    /// Construct an untrusted provider response for validation by the core.
    pub fn new(
        id: impl Into<String>,
        payload: Vec<u8>,
        resolved_version: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            payload,
            resolved_version: resolved_version.into(),
        }
    }

    /// Request-local correlation identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Debug for TokenizeResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenizeResult")
            .field("id", &self.id)
            .field("payload", &"[REDACTED]")
            .field("resolved_version", &self.resolved_version)
            .finish()
    }
}

/// One deduplicated canonical token submitted for restoration.
#[derive(Clone, PartialEq, Eq)]
pub struct RestoreItem {
    id: String,
    token_ref: String,
    resolved_version: String,
    payload: Vec<u8>,
}

impl RestoreItem {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn token_ref(&self) -> &str {
        &self.token_ref
    }
    pub fn resolved_version(&self) -> &str {
        &self.resolved_version
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl std::fmt::Debug for RestoreItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RestoreItem")
            .field("id", &self.id)
            .field("token_ref", &self.token_ref)
            .field("resolved_version", &self.resolved_version)
            .field("payload", &"[REDACTED]")
            .finish()
    }
}

/// Restored plaintext returned for one provider request item.
#[derive(Clone, PartialEq, Eq)]
pub struct RestoredValue {
    id: String,
    value: String,
}

impl RestoredValue {
    pub fn new(id: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Debug for RestoredValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RestoredValue")
            .field("id", &self.id)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenProviderErrorKind {
    NotFound,
    Expired,
    AccessDenied,
    Unavailable,
    ProviderError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenProviderError {
    kind: TokenProviderErrorKind,
}

impl TokenProviderError {
    pub fn new(kind: TokenProviderErrorKind) -> Self {
        Self { kind }
    }
    pub fn kind(self) -> TokenProviderErrorKind {
        self.kind
    }
}

impl std::fmt::Display for TokenProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("token provider could not complete the request")
    }
}

impl std::error::Error for TokenProviderError {}

pub type TokenizeProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<TokenizeResult>, TokenProviderError>> + Send + 'a>>;
pub type RestoreProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<RestoredValue>, TokenProviderError>> + Send + 'a>>;

/// Vendor-neutral asynchronous boundary for reversible token operations.
pub trait TokenProvider: Send + Sync {
    /// Whether this value represents an available provider capability.
    fn is_configured(&self) -> bool {
        true
    }

    fn tokenize_batch(&self, scope: &str, items: Vec<TokenizeItem>) -> TokenizeProviderFuture<'_>;
    fn restore_batch(&self, scope: &str, items: Vec<RestoreItem>) -> RestoreProviderFuture<'_>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoTokenProvider;

#[derive(Debug, Clone, Copy, Default)]
pub struct NoKeyProvider;

impl KeyProvider for NoKeyProvider {
    fn is_configured(&self) -> bool {
        false
    }

    fn resolve_key(&self, _selector: KeySelector) -> KeyProviderFuture<'_> {
        Box::pin(async { Err(KeyProviderError::new(KeyProviderErrorKind::ProviderError)) })
    }
}

impl TokenProvider for NoTokenProvider {
    fn is_configured(&self) -> bool {
        false
    }

    fn tokenize_batch(
        &self,
        _scope: &str,
        _items: Vec<TokenizeItem>,
    ) -> TokenizeProviderFuture<'_> {
        Box::pin(async {
            Err(TokenProviderError::new(
                TokenProviderErrorKind::ProviderError,
            ))
        })
    }

    fn restore_batch(&self, _scope: &str, _items: Vec<RestoreItem>) -> RestoreProviderFuture<'_> {
        Box::pin(async {
            Err(TokenProviderError::new(
                TokenProviderErrorKind::ProviderError,
            ))
        })
    }
}

/// Provider-backed privacy operation manager.
#[derive(Debug)]
pub struct PrivacyManager<P, T = NoTokenProvider> {
    provider: P,
    token_provider: T,
}

impl<P> PrivacyManager<P> {
    /// Create a manager with one runtime provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            token_provider: NoTokenProvider,
        }
    }

    /// Borrow the configured provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Add reversible token capability without changing the key provider.
    pub fn with_token_provider<T>(self, token_provider: T) -> PrivacyManager<P, T> {
        PrivacyManager {
            provider: self.provider,
            token_provider,
        }
    }
}

impl<T> PrivacyManager<NoKeyProvider, T> {
    /// Create a manager that supports token operations but no pseudonymization.
    pub fn token_provider_only(token_provider: T) -> Self {
        Self {
            provider: NoKeyProvider,
            token_provider,
        }
    }
}

impl<P, T> PrivacyManager<P, T> {
    /// Borrow the configured token provider or marker capability.
    pub fn token_provider(&self) -> &T {
        &self.token_provider
    }
}

/// One transformation applied to the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Transformation {
    /// Canonical PII type of the source finding.
    pub entity_type: String,
    /// Source range in UTF-8 bytes.
    pub source_byte_range: TextRange,
    /// Source range in Unicode code points.
    pub source_codepoint_range: TextRange,
    /// Detector confidence, when available.
    pub confidence: Option<f32>,
    /// Stable detector name.
    pub detector_name: String,
    /// Detector implementation version, when available.
    pub detector_version: Option<String>,
    /// Strategy applied to the finding.
    pub strategy: TransformationStrategy,
    /// Exact replacement inserted into the output text.
    pub replacement: String,
    /// Range of the replacement in UTF-8 bytes in the output text.
    pub output_byte_range: TextRange,
    /// Range of the replacement in Unicode code points in the output text.
    pub output_codepoint_range: TextRange,
    /// Provider-specific key reference for pseudonymization only.
    pub key_ref: Option<String>,
    /// Concrete provider version for pseudonymization only.
    pub resolved_key_version: Option<String>,
    /// Provider profile reference for tokenization only.
    pub token_ref: Option<String>,
    /// Concrete provider profile version for tokenization only.
    pub resolved_token_version: Option<String>,
}

/// Text and audit records produced by a transformation.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformResult {
    /// Transformed text.
    pub text: String,
    /// Applied transformations in source document order.
    pub transformations: Vec<Transformation>,
}

/// One token replacement applied while restoring a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restoration {
    pub source_byte_range: TextRange,
    pub source_codepoint_range: TextRange,
    pub output_byte_range: TextRange,
    pub output_codepoint_range: TextRange,
    pub token_ref: String,
    pub resolved_token_version: String,
}

/// Restored text and non-secret range metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct RestoreResult {
    pub text: String,
    pub restorations: Vec<Restoration>,
}

impl std::fmt::Debug for RestoreResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RestoreResult")
            .field("text", &"[REDACTED]")
            .field("restorations", &self.restorations)
            .finish()
    }
}

/// Reason a caller-supplied finding is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingValidationError {
    /// The UTF-8 byte range is empty or reversed.
    EmptyOrReversedByteRange,
    /// The UTF-8 byte range extends beyond the source text.
    ByteRangeOutOfBounds,
    /// A UTF-8 byte offset does not fall on a character boundary.
    InvalidUtf8Boundary,
    /// The Unicode code-point range is empty or reversed.
    EmptyOrReversedCodepointRange,
    /// The Unicode code-point range extends beyond the source text.
    CodepointRangeOutOfBounds,
    /// The byte and code-point ranges select different source spans.
    InconsistentRanges,
    /// `matched_text` differs from the source substring selected by the range.
    MatchedTextMismatch,
    /// Confidence is non-finite or outside `0.0..=1.0`.
    InvalidConfidence,
}

/// Stable top-level category for a privacy-operation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyErrorCode {
    /// Transformation or scanning configuration is invalid.
    InvalidConfiguration,
    /// One caller-supplied finding is invalid.
    InvalidFinding,
    /// Pseudonymization was selected without a runtime provider.
    KeyProviderRequired,
    /// The requested provider key does not exist.
    KeyNotFound,
    /// The provider denied access to the requested key.
    KeyAccessDenied,
    /// The provider is temporarily unavailable.
    KeyProviderUnavailable,
    /// The provider returned malformed or weak key material.
    InvalidKeyMaterial,
    /// The provider failed without a more specific safe category.
    KeyProviderError,
    /// Tokenization was selected without a runtime token provider.
    TokenProviderRequired,
    /// Input contains a malformed canonical token.
    InvalidToken,
    /// Input uses a canonical token version this core does not support.
    UnsupportedTokenVersion,
    /// The requested token does not exist.
    TokenNotFound,
    /// The requested token has expired.
    TokenExpired,
    /// The token provider denied restoration.
    TokenAccessDenied,
    /// The token provider returned malformed material.
    InvalidTokenMaterial,
    /// The token provider is temporarily unavailable.
    TokenProviderUnavailable,
    /// The token provider failed without a more specific safe category.
    TokenProviderError,
    /// The selected runtime intentionally cannot execute this strategy.
    UnsupportedStrategy,
    /// An unexpected non-caller-correctable failure occurred.
    InternalError,
}

impl PrivacyErrorCode {
    /// Stable serialized value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::InvalidFinding => "invalid_finding",
            Self::KeyProviderRequired => "key_provider_required",
            Self::KeyNotFound => "key_not_found",
            Self::KeyAccessDenied => "key_access_denied",
            Self::KeyProviderUnavailable => "key_provider_unavailable",
            Self::InvalidKeyMaterial => "invalid_key_material",
            Self::KeyProviderError => "key_provider_error",
            Self::TokenProviderRequired => "token_provider_required",
            Self::InvalidToken => "invalid_token",
            Self::UnsupportedTokenVersion => "unsupported_token_version",
            Self::TokenNotFound => "token_not_found",
            Self::TokenExpired => "token_expired",
            Self::TokenAccessDenied => "token_access_denied",
            Self::InvalidTokenMaterial => "invalid_token_material",
            Self::TokenProviderUnavailable => "token_provider_unavailable",
            Self::TokenProviderError => "token_provider_error",
            Self::UnsupportedStrategy => "unsupported_strategy",
            Self::InternalError => "internal_error",
        }
    }
}

/// Stable machine-readable reason for a caller-correctable error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyErrorReason {
    MissingField,
    UnknownField,
    InvalidType,
    InvalidValue,
    EmptyValue,
    DuplicateValue,
    InvalidRegex,
    LimitExceeded,
    MatchedTextMismatch,
    InconsistentRanges,
    OutOfBounds,
    InvalidBoundary,
    InvalidConfidence,
}

impl PrivacyErrorReason {
    /// Stable serialized value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingField => "missing_field",
            Self::UnknownField => "unknown_field",
            Self::InvalidType => "invalid_type",
            Self::InvalidValue => "invalid_value",
            Self::EmptyValue => "empty_value",
            Self::DuplicateValue => "duplicate_value",
            Self::InvalidRegex => "invalid_regex",
            Self::LimitExceeded => "limit_exceeded",
            Self::MatchedTextMismatch => "matched_text_mismatch",
            Self::InconsistentRanges => "inconsistent_ranges",
            Self::OutOfBounds => "out_of_bounds",
            Self::InvalidBoundary => "invalid_boundary",
            Self::InvalidConfidence => "invalid_confidence",
        }
    }
}

/// A privacy operation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyError {
    code: PrivacyErrorCode,
    reason: Option<PrivacyErrorReason>,
    path: Option<String>,
    finding_index: Option<usize>,
    message: String,
}

impl PrivacyError {
    fn invalid_configuration(
        reason: PrivacyErrorReason,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: PrivacyErrorCode::InvalidConfiguration,
            reason: Some(reason),
            path: Some(path.into()),
            finding_index: None,
            message: message.into(),
        }
    }

    fn invalid_finding(finding_index: usize, kind: FindingValidationError) -> Self {
        let (reason, suffix, message) = match kind {
            FindingValidationError::EmptyOrReversedByteRange => (
                PrivacyErrorReason::InvalidValue,
                "byte_range",
                "finding byte range must be non-empty and increasing",
            ),
            FindingValidationError::ByteRangeOutOfBounds => (
                PrivacyErrorReason::OutOfBounds,
                "byte_range",
                "finding byte range is outside the source text",
            ),
            FindingValidationError::InvalidUtf8Boundary => (
                PrivacyErrorReason::InvalidBoundary,
                "byte_range",
                "finding byte range does not use UTF-8 boundaries",
            ),
            FindingValidationError::EmptyOrReversedCodepointRange => (
                PrivacyErrorReason::InvalidValue,
                "codepoint_range",
                "finding code-point range must be non-empty and increasing",
            ),
            FindingValidationError::CodepointRangeOutOfBounds => (
                PrivacyErrorReason::OutOfBounds,
                "codepoint_range",
                "finding code-point range is outside the source text",
            ),
            FindingValidationError::InconsistentRanges => (
                PrivacyErrorReason::InconsistentRanges,
                "codepoint_range",
                "finding byte and code-point ranges select different text",
            ),
            FindingValidationError::MatchedTextMismatch => (
                PrivacyErrorReason::MatchedTextMismatch,
                "matched_text",
                "finding text does not match the selected source span",
            ),
            FindingValidationError::InvalidConfidence => (
                PrivacyErrorReason::InvalidConfidence,
                "confidence",
                "finding confidence must be finite and in 0.0..=1.0",
            ),
        };
        Self {
            code: PrivacyErrorCode::InvalidFinding,
            reason: Some(reason),
            path: Some(format!("/findings/{finding_index}/{suffix}")),
            finding_index: Some(finding_index),
            message: message.to_owned(),
        }
    }

    fn key_error(code: PrivacyErrorCode, path: impl Into<String>, message: &'static str) -> Self {
        Self {
            code,
            reason: None,
            path: Some(path.into()),
            finding_index: None,
            message: message.to_owned(),
        }
    }

    fn provider_required(path: impl Into<String>) -> Self {
        Self::key_error(
            PrivacyErrorCode::KeyProviderRequired,
            path,
            "pseudonymization requires a runtime key provider",
        )
    }

    fn invalid_key_material(path: impl Into<String>) -> Self {
        Self::key_error(
            PrivacyErrorCode::InvalidKeyMaterial,
            path,
            "key provider returned invalid key material",
        )
    }

    fn token_error(code: PrivacyErrorCode, message: &'static str) -> Self {
        Self {
            code,
            reason: None,
            path: None,
            finding_index: None,
            message: message.to_owned(),
        }
    }

    fn token_provider_required(path: impl Into<String>) -> Self {
        Self::key_error(
            PrivacyErrorCode::TokenProviderRequired,
            path,
            "tokenization requires a runtime token provider and request scope",
        )
    }

    fn invalid_token() -> Self {
        Self::token_error(
            PrivacyErrorCode::InvalidToken,
            "input contains an invalid token",
        )
    }

    fn unsupported_token_version() -> Self {
        Self::token_error(
            PrivacyErrorCode::UnsupportedTokenVersion,
            "input contains an unsupported token version",
        )
    }

    fn invalid_token_material() -> Self {
        Self::token_error(
            PrivacyErrorCode::InvalidTokenMaterial,
            "token provider returned invalid token material",
        )
    }

    fn from_token_provider_error(error: TokenProviderError) -> Self {
        let (code, message) = match error.kind() {
            TokenProviderErrorKind::NotFound => {
                (PrivacyErrorCode::TokenNotFound, "token was not found")
            }
            TokenProviderErrorKind::Expired => {
                (PrivacyErrorCode::TokenExpired, "token has expired")
            }
            TokenProviderErrorKind::AccessDenied => (
                PrivacyErrorCode::TokenAccessDenied,
                "token access was denied",
            ),
            TokenProviderErrorKind::Unavailable => (
                PrivacyErrorCode::TokenProviderUnavailable,
                "token provider is temporarily unavailable",
            ),
            TokenProviderErrorKind::ProviderError => (
                PrivacyErrorCode::TokenProviderError,
                "token provider could not complete the request",
            ),
        };
        Self::token_error(code, message)
    }

    fn internal(message: &'static str) -> Self {
        Self {
            code: PrivacyErrorCode::InternalError,
            reason: None,
            path: None,
            finding_index: None,
            message: message.to_owned(),
        }
    }

    fn from_provider_error(path: impl Into<String>, error: KeyProviderError) -> Self {
        let (code, message) = match error.kind() {
            KeyProviderErrorKind::NotFound => (
                PrivacyErrorCode::KeyNotFound,
                "key provider could not find the requested key",
            ),
            KeyProviderErrorKind::AccessDenied => (
                PrivacyErrorCode::KeyAccessDenied,
                "key provider denied access to the requested key",
            ),
            KeyProviderErrorKind::Unavailable => (
                PrivacyErrorCode::KeyProviderUnavailable,
                "key provider is temporarily unavailable",
            ),
            KeyProviderErrorKind::ProviderError => (
                PrivacyErrorCode::KeyProviderError,
                "key provider could not resolve the requested key",
            ),
        };
        Self::key_error(code, path, message)
    }

    /// Create a structured unsupported-strategy error for a binding that
    /// intentionally cannot execute a canonical strategy.
    pub fn unsupported_strategy(path: impl Into<String>) -> Self {
        Self::key_error(
            PrivacyErrorCode::UnsupportedStrategy,
            path,
            "the selected runtime does not support this provider-backed strategy",
        )
    }

    fn prefixed(mut self, prefix: &str) -> Self {
        if let Some(path) = &mut self.path {
            *path = format!("{prefix}{path}");
        }
        self
    }

    /// Stable top-level category.
    pub fn code(&self) -> PrivacyErrorCode {
        self.code
    }

    /// Stable caller-correctable reason, when applicable.
    pub fn reason(&self) -> Option<PrivacyErrorReason> {
        self.reason
    }

    /// RFC 6901 request path, when applicable.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Invalid finding index, when applicable.
    pub fn finding_index(&self) -> Option<usize> {
        self.finding_index
    }
}

impl std::fmt::Display for PrivacyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PrivacyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Label {
    Email,
    Phone,
    Ssn,
    CreditCard,
    IpAddress,
    Date,
    ZipCode,
}

/// define a helper function for Label
impl Label {
    fn as_str(self) -> &'static str {
        match self {
            Label::Email => "EMAIL",
            Label::Phone => "PHONE",
            Label::Ssn => "SSN",
            Label::CreditCard => "CREDIT_CARD",
            Label::IpAddress => "IP_ADDRESS",
            Label::Date => "DATE",
            Label::ZipCode => "ZIP_CODE",
        }
    }

    fn detector_name(self) -> &'static str {
        match self {
            Label::Email => "datafog-core/email",
            Label::Phone => "datafog-core/phone",
            Label::Ssn => "datafog-core/ssn",
            Label::CreditCard => "datafog-core/credit-card",
            Label::IpAddress => "datafog-core/ip-address",
            Label::Date => "datafog-core/date",
            Label::ZipCode => "datafog-core/zip-code",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    label: Label,
    start_byte: usize,
    end_byte: usize,
}

/// Scan text for supported PII findings.
pub fn scan(text: &str) -> Vec<Finding> {
    scan_with_config(text, &ScanConfig::default())
}

/// Scan text using explicit detector configuration.
pub fn scan_with_config(text: &str, _config: &ScanConfig) -> Vec<Finding> {
    let mut candidates: Vec<Candidate> = Vec::new();
    detect_email(text, &mut candidates);
    detect_phone(text, &mut candidates);
    detect_ssn(text, &mut candidates);
    detect_credit_card(text, &mut candidates);
    detect_date(text, &mut candidates);
    detect_zip_code(text, &mut candidates);
    detect_ip_address(text, &mut candidates);
    finalize(text, candidates)
}

/// Transform caller-supplied findings without scanning implicitly.
pub fn transform(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
) -> Result<TransformResult, PrivacyError> {
    let selected_findings = select_findings(text, findings, config)?;
    if let Some(selector) = key_selectors(config, &selected_findings).into_iter().next() {
        return Err(PrivacyError::provider_required(selector.path));
    }
    if let Some((_, path)) = tokenization_selections(config, &selected_findings)
        .into_iter()
        .next()
    {
        return Err(PrivacyError::token_provider_required(path));
    }
    apply_transformations(
        text,
        &selected_findings,
        config,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
}

/// Return the distinct provider keys required after validation, filtering,
/// allowlists, duplicate handling, and overlap resolution.
pub fn required_key_selectors(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
) -> Result<Vec<KeySelector>, PrivacyError> {
    let selected_findings = select_findings(text, findings, config)?;
    Ok(key_selectors(config, &selected_findings))
}

/// One resolved key associated with the selector that requested it.
pub struct ResolvedKeyBinding {
    selector: KeySelector,
    key: ResolvedKey,
}

impl ResolvedKeyBinding {
    /// Associate one provider response with its original selector.
    pub fn new(selector: KeySelector, key: ResolvedKey) -> Self {
        Self { selector, key }
    }
}

impl std::fmt::Debug for ResolvedKeyBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedKeyBinding")
            .field("selector", &self.selector)
            .field("key", &self.key)
            .finish()
    }
}

/// Apply a transformation using provider responses already resolved by a thin
/// language binding or another trusted orchestration layer.
pub fn transform_with_resolved_keys(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
    resolved_keys: Vec<ResolvedKeyBinding>,
) -> Result<TransformResult, PrivacyError> {
    let selected_findings = select_findings(text, findings, config)?;
    let selectors = key_selectors(config, &selected_findings);
    let resolved_keys = validate_resolved_keys(selectors, resolved_keys)?;
    if let Some((_, path)) = tokenization_selections(config, &selected_findings)
        .into_iter()
        .next()
    {
        return Err(PrivacyError::token_provider_required(path));
    }
    apply_transformations(
        text,
        &selected_findings,
        config,
        &resolved_keys,
        &BTreeMap::new(),
    )
}

fn tokenization_selections(
    config: &TransformationConfig,
    selected_findings: &[Finding],
) -> Vec<(usize, String)> {
    selected_findings
        .iter()
        .enumerate()
        .filter_map(|(index, finding)| match config.strategy_for(finding) {
            TransformationStrategy::Tokenize(_) => Some((index, config.strategy_path_for(finding))),
            _ => None,
        })
        .collect()
}

/// Return the stateful provider items required by a validated transformation.
pub fn required_tokenization_items(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
    context: Option<&PrivacyContext>,
) -> Result<Vec<TokenizeItem>, PrivacyError> {
    let selected = select_findings(text, findings, config)?;
    let selections = tokenization_selections(config, &selected);
    if selections.is_empty() {
        return Ok(Vec::new());
    }
    if context.is_none() {
        return Err(PrivacyError::token_provider_required(
            selections[0].1.clone(),
        ));
    }
    let existing_tokens = parse_tokens_lenient(text);
    let mut items = Vec::with_capacity(selections.len());
    for (index, path) in selections {
        let finding = &selected[index];
        if existing_tokens.iter().any(|token| {
            finding.byte_range.start < token.end && token.start < finding.byte_range.end
        }) {
            return Err(PrivacyError::key_error(
                PrivacyErrorCode::InvalidToken,
                path,
                "tokenization cannot select an existing canonical token",
            ));
        }
        let TransformationStrategy::Tokenize(tokenize) = config.strategy_for(finding) else {
            return Err(PrivacyError::internal(
                "tokenization selection changed unexpectedly",
            ));
        };
        items.push(TokenizeItem {
            id: index.to_string(),
            exact_value: finding.matched_text.clone(),
            token_ref: tokenize.token_ref.clone(),
        });
    }
    Ok(items)
}

fn validate_tokenize_results(
    items: &[TokenizeItem],
    results: Vec<TokenizeResult>,
) -> Result<BTreeMap<String, (String, String)>, PrivacyError> {
    let expected = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut validated = BTreeMap::new();
    for result in results {
        if !expected.contains(&result.id)
            || result.payload.is_empty()
            || result.resolved_version.trim().is_empty()
        {
            return Err(PrivacyError::invalid_token_material());
        }
        let item = items
            .iter()
            .find(|item| item.id == result.id)
            .ok_or_else(PrivacyError::invalid_token_material)?;
        let envelope = encode_token(&item.token_ref, &result.resolved_version, &result.payload);
        if validated
            .insert(result.id, (envelope, result.resolved_version))
            .is_some()
        {
            return Err(PrivacyError::invalid_token_material());
        }
    }
    if validated.len() != expected.len() {
        return Err(PrivacyError::invalid_token_material());
    }
    Ok(validated)
}

/// Complete a transformation using already resolved provider responses.
pub fn transform_with_provider_results(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
    context: Option<&PrivacyContext>,
    resolved_keys: Vec<ResolvedKeyBinding>,
    token_results: Vec<TokenizeResult>,
) -> Result<TransformResult, PrivacyError> {
    let selected = select_findings(text, findings, config)?;
    let keys = validate_resolved_keys(key_selectors(config, &selected), resolved_keys)?;
    let items = required_tokenization_items(text, findings, config, context)?;
    let tokens = validate_tokenize_results(&items, token_results)?;
    apply_transformations(text, &selected, config, &keys, &tokens)
}

const TOKEN_PREFIX: &str = "DFTOKENv";

#[derive(Clone)]
struct ParsedToken {
    start: usize,
    end: usize,
    token_ref: String,
    resolved_version: String,
    payload: Vec<u8>,
    envelope: String,
}

fn encode_token(token_ref: &str, version: &str, payload: &[u8]) -> String {
    let encoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let body = format!(
        "{}.{}.{}",
        encoder.encode(token_ref.as_bytes()),
        encoder.encode(version.as_bytes()),
        encoder.encode(payload)
    );
    format!("DFTOKENv1({}):{body}", body.len())
}

fn decode_canonical_component(value: &str) -> Result<Vec<u8>, PrivacyError> {
    if value.is_empty() {
        return Err(PrivacyError::invalid_token());
    }
    let decoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let decoded = decoder
        .decode(value)
        .map_err(|_| PrivacyError::invalid_token())?;
    if decoded.is_empty() || decoder.encode(&decoded) != value {
        return Err(PrivacyError::invalid_token());
    }
    Ok(decoded)
}

fn parse_token_at(text: &str, start: usize) -> Result<ParsedToken, PrivacyError> {
    let tail = &text[start + TOKEN_PREFIX.len()..];
    let open = tail.find('(').ok_or_else(PrivacyError::invalid_token)?;
    let version_text = &tail[..open];
    if version_text.is_empty() || !version_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PrivacyError::invalid_token());
    }
    if version_text != "1" {
        return Err(PrivacyError::unsupported_token_version());
    }
    let after_open = &tail[open + 1..];
    let close = after_open
        .find("):")
        .ok_or_else(PrivacyError::invalid_token)?;
    let length_text = &after_open[..close];
    if length_text.is_empty() || !length_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(PrivacyError::invalid_token());
    }
    if length_text.len() > 1 && length_text.starts_with('0') {
        return Err(PrivacyError::invalid_token());
    }
    let body_length = length_text
        .bytes()
        .try_fold(0usize, |length, digit| {
            length
                .checked_mul(10)?
                .checked_add(usize::from(digit - b'0'))
        })
        .ok_or_else(PrivacyError::invalid_token)?;
    let body_start = start + TOKEN_PREFIX.len() + open + 1 + close + 2;
    let body_end = body_start
        .checked_add(body_length)
        .ok_or_else(PrivacyError::invalid_token)?;
    if body_end > text.len() || !text.is_char_boundary(body_end) {
        return Err(PrivacyError::invalid_token());
    }
    let body = &text[body_start..body_end];
    let mut components = body.split('.');
    let token_ref_bytes = decode_canonical_component(components.next().unwrap_or_default())?;
    let version_bytes = decode_canonical_component(components.next().unwrap_or_default())?;
    let payload = decode_canonical_component(components.next().unwrap_or_default())?;
    if components.next().is_some() {
        return Err(PrivacyError::invalid_token());
    }
    let token_ref =
        String::from_utf8(token_ref_bytes).map_err(|_| PrivacyError::invalid_token())?;
    let resolved_version =
        String::from_utf8(version_bytes).map_err(|_| PrivacyError::invalid_token())?;
    if token_ref.trim().is_empty() || resolved_version.trim().is_empty() {
        return Err(PrivacyError::invalid_token());
    }
    Ok(ParsedToken {
        start,
        end: body_end,
        token_ref,
        resolved_version,
        payload,
        envelope: text[start..body_end].to_owned(),
    })
}

fn parse_tokens(text: &str) -> Result<Vec<ParsedToken>, PrivacyError> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find(TOKEN_PREFIX) {
        let start = cursor + relative;
        let token = parse_token_at(text, start)?;
        cursor = token.end;
        tokens.push(token);
    }
    Ok(tokens)
}

fn parse_tokens_lenient(text: &str) -> Vec<ParsedToken> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("DFTOKENv1(") {
        let start = cursor + relative;
        match parse_token_at(text, start) {
            Ok(token) => {
                cursor = token.end;
                tokens.push(token);
            }
            Err(_) => cursor = start + "DFTOKENv1(".len(),
        }
    }
    tokens
}

/// Parse and deduplicate all canonical tokens before a provider restore call.
pub fn required_restore_items(
    text: &str,
    _context: &PrivacyContext,
) -> Result<Vec<RestoreItem>, PrivacyError> {
    let tokens = parse_tokens(text)?;
    let mut ids = BTreeMap::<String, String>::new();
    let mut items = Vec::new();
    for token in tokens {
        if ids.contains_key(&token.envelope) {
            continue;
        }
        let id = items.len().to_string();
        ids.insert(token.envelope, id.clone());
        items.push(RestoreItem {
            id,
            token_ref: token.token_ref,
            resolved_version: token.resolved_version,
            payload: token.payload,
        });
    }
    Ok(items)
}

/// Apply complete, validated provider restoration results atomically.
pub fn restore_with_results(
    text: &str,
    context: &PrivacyContext,
    results: Vec<RestoredValue>,
) -> Result<RestoreResult, PrivacyError> {
    let tokens = parse_tokens(text)?;
    let items = required_restore_items(text, context)?;
    let expected = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut values = BTreeMap::new();
    for result in results {
        if !expected.contains(&result.id) || values.insert(result.id, result.value).is_some() {
            return Err(PrivacyError::invalid_token_material());
        }
    }
    if values.len() != expected.len() {
        return Err(PrivacyError::invalid_token_material());
    }
    let mut envelope_ids = BTreeMap::new();
    for token in &tokens {
        if !envelope_ids.contains_key(&token.envelope) {
            let id = envelope_ids.len().to_string();
            envelope_ids.insert(token.envelope.clone(), id);
        }
    }
    let mut output = String::with_capacity(text.len());
    let mut restorations = Vec::with_capacity(tokens.len());
    let mut cursor = 0;
    for token in tokens {
        output.push_str(&text[cursor..token.start]);
        let output_byte_start = output.len();
        let output_codepoint_start = output.chars().count();
        let id = envelope_ids
            .get(&token.envelope)
            .ok_or_else(PrivacyError::invalid_token_material)?;
        let value = values
            .get(id)
            .ok_or_else(PrivacyError::invalid_token_material)?;
        output.push_str(value);
        restorations.push(Restoration {
            source_byte_range: TextRange {
                start: token.start,
                end: token.end,
            },
            source_codepoint_range: TextRange {
                start: text[..token.start].chars().count(),
                end: text[..token.end].chars().count(),
            },
            output_byte_range: TextRange {
                start: output_byte_start,
                end: output.len(),
            },
            output_codepoint_range: TextRange {
                start: output_codepoint_start,
                end: output.chars().count(),
            },
            token_ref: token.token_ref,
            resolved_token_version: token.resolved_version,
        });
        cursor = token.end;
    }
    output.push_str(&text[cursor..]);
    Ok(RestoreResult {
        text: output,
        restorations,
    })
}

fn select_findings(
    text: &str,
    findings: &[Finding],
    config: &TransformationConfig,
) -> Result<Vec<Finding>, PrivacyError> {
    let mut offsets = TextIndex::new(text);
    for (finding_index, finding) in findings.iter().enumerate() {
        if let Err(kind) = validate_finding_with_index(text, finding, &mut offsets) {
            return Err(PrivacyError::invalid_finding(finding_index, kind));
        }
    }

    Ok(select_validated_findings(findings, config))
}

fn select_validated_findings(findings: &[Finding], config: &TransformationConfig) -> Vec<Finding> {
    let mut selected_findings: Vec<Finding> = Vec::with_capacity(findings.len());
    let mut duplicate_indices = BTreeMap::new();
    for finding in findings
        .iter()
        .filter(|finding| config.includes(finding) && !config.allows(finding))
    {
        // Validation against the same text makes the byte range determine both
        // the matched text and the code-point range. Preserve encounter order
        // within each group: mixed confidence is not a sortable preference.
        let identity = (
            finding.entity_type.as_str(),
            finding.byte_range.start,
            finding.byte_range.end,
        );
        match duplicate_indices.entry(identity) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                let existing = &mut selected_findings[*entry.get()];
                if duplicate_preference(finding, existing).is_lt() {
                    *existing = finding.clone();
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(selected_findings.len());
                selected_findings.push(finding.clone());
            }
        }
    }
    selected_findings.sort_by(|left, right| {
        (
            left.codepoint_range.start,
            left.codepoint_range.end,
            &left.entity_type,
        )
            .cmp(&(
                right.codepoint_range.start,
                right.codepoint_range.end,
                &right.entity_type,
            ))
    });
    resolve_overlaps(selected_findings)
}

fn key_selectors(config: &TransformationConfig, selected_findings: &[Finding]) -> Vec<KeySelector> {
    let mut selectors = BTreeMap::new();
    for finding in selected_findings {
        if let TransformationStrategy::Pseudonymize(pseudonymize) = config.strategy_for(finding) {
            selectors
                .entry(pseudonymize.clone())
                .or_insert_with(|| KeySelector {
                    config: pseudonymize.clone(),
                    path: config.strategy_path_for(finding),
                });
        }
    }
    selectors.into_values().collect()
}

fn validate_resolved_key(selector: &KeySelector, key: &ResolvedKey) -> Result<(), PrivacyError> {
    if key.key().len() != 32 || key.resolved_version().trim().is_empty() {
        return Err(PrivacyError::invalid_key_material(selector.path.clone()));
    }
    Ok(())
}

fn validate_resolved_keys(
    selectors: Vec<KeySelector>,
    bindings: Vec<ResolvedKeyBinding>,
) -> Result<BTreeMap<PseudonymizeConfig, ResolvedKey>, PrivacyError> {
    let expected = selectors
        .iter()
        .map(|selector| (selector.config.clone(), selector))
        .collect::<BTreeMap<_, _>>();
    let mut resolved = BTreeMap::new();
    for binding in bindings {
        let Some(selector) = expected.get(&binding.selector.config) else {
            return Err(PrivacyError::invalid_key_material(binding.selector.path));
        };
        validate_resolved_key(selector, &binding.key)?;
        if resolved
            .insert(binding.selector.config, binding.key)
            .is_some()
        {
            return Err(PrivacyError::invalid_key_material(selector.path.clone()));
        }
    }
    for selector in selectors {
        if !resolved.contains_key(&selector.config) {
            return Err(PrivacyError::provider_required(selector.path));
        }
    }
    Ok(resolved)
}

fn apply_transformations(
    text: &str,
    selected_findings: &[Finding],
    config: &TransformationConfig,
    resolved_keys: &BTreeMap<PseudonymizeConfig, ResolvedKey>,
    tokens: &BTreeMap<String, (String, String)>,
) -> Result<TransformResult, PrivacyError> {
    let mut output = String::with_capacity(text.len());
    let mut transformations = Vec::with_capacity(selected_findings.len());
    let mut source_byte_cursor = 0;
    let mut output_codepoints = 0;

    for (finding_index, finding) in selected_findings.iter().enumerate() {
        let unchanged = &text[source_byte_cursor..finding.byte_range.start];
        output.push_str(unchanged);
        output_codepoints += unchanged.chars().count();
        let output_byte_start = output.len();
        let output_codepoint_start = output_codepoints;
        let strategy = config.strategy_for(finding);
        let mut key_ref = None;
        let mut resolved_key_version = None;
        let mut token_ref = None;
        let mut resolved_token_version = None;
        let replacement = match strategy {
            TransformationStrategy::Redact => format!("[{}]", finding.entity_type),
            TransformationStrategy::Remove => String::new(),
            TransformationStrategy::Mask(config) => {
                let codepoint_count = finding.matched_text.chars().count();
                finding
                    .matched_text
                    .chars()
                    .enumerate()
                    .map(|(index, source)| {
                        let revealed = match config.reveal {
                            MaskReveal::None => false,
                            MaskReveal::First(count) => index < count,
                            MaskReveal::Last(count) => {
                                index >= codepoint_count.saturating_sub(count)
                            }
                        };
                        if revealed { source } else { config.character }
                    })
                    .collect()
            }
            TransformationStrategy::Pseudonymize(pseudonymize) => {
                let selector_path = config.strategy_path_for(finding);
                let resolved = resolved_keys
                    .get(pseudonymize)
                    .ok_or_else(|| PrivacyError::provider_required(selector_path))?;
                let mut mac = Hmac::<Sha256>::new_from_slice(resolved.key())
                    .map_err(|_| PrivacyError::internal("could not initialize HMAC-SHA-256"))?;
                mac.update(finding.matched_text.as_bytes());
                key_ref = Some(pseudonymize.key_ref.clone());
                resolved_key_version = Some(resolved.resolved_version.clone());
                base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
            }
            TransformationStrategy::Tokenize(tokenize) => {
                let (envelope, version) =
                    tokens.get(&finding_index.to_string()).ok_or_else(|| {
                        PrivacyError::token_provider_required(config.strategy_path_for(finding))
                    })?;
                token_ref = Some(tokenize.token_ref.clone());
                resolved_token_version = Some(version.clone());
                envelope.clone()
            }
        };
        output.push_str(&replacement);

        output_codepoints += replacement.chars().count();
        transformations.push(Transformation {
            entity_type: finding.entity_type.clone(),
            source_byte_range: finding.byte_range,
            source_codepoint_range: finding.codepoint_range,
            confidence: finding.confidence,
            detector_name: finding.detector_name.clone(),
            detector_version: finding.detector_version.clone(),
            strategy: strategy.clone(),
            replacement,
            output_byte_range: TextRange {
                start: output_byte_start,
                end: output.len(),
            },
            output_codepoint_range: TextRange {
                start: output_codepoint_start,
                end: output_codepoints,
            },
            key_ref,
            resolved_key_version,
            token_ref,
            resolved_token_version,
        });
        source_byte_cursor = finding.byte_range.end;
    }

    output.push_str(&text[source_byte_cursor..]);
    Ok(TransformResult {
        text: output,
        transformations,
    })
}

impl<P: KeyProvider, T> PrivacyManager<P, T> {
    /// Transform caller-supplied findings after atomically resolving every
    /// distinct key selected by the request.
    pub async fn transform(
        &self,
        text: &str,
        findings: &[Finding],
        config: &TransformationConfig,
    ) -> Result<TransformResult, PrivacyError> {
        let selected_findings = select_findings(text, findings, config)?;
        let selectors = key_selectors(config, &selected_findings);
        let mut resolved = BTreeMap::new();
        for selector in selectors {
            if !self.provider.is_configured() {
                return Err(PrivacyError::provider_required(selector.path));
            }
            let key = self
                .provider
                .resolve_key(selector.clone())
                .await
                .map_err(|error| PrivacyError::from_provider_error(selector.path.clone(), error))?;
            validate_resolved_key(&selector, &key)?;
            resolved.insert(selector.config, key);
        }
        let selections = tokenization_selections(config, &selected_findings);
        if let Some((_, path)) = selections.into_iter().next() {
            return Err(PrivacyError::token_provider_required(path));
        }
        apply_transformations(
            text,
            &selected_findings,
            config,
            &resolved,
            &BTreeMap::new(),
        )
    }

    /// Scan and transform after atomically resolving every selected key.
    pub async fn scan_and_transform(
        &self,
        text: &str,
        config: &ScanAndTransformConfig,
    ) -> Result<TransformResult, PrivacyError> {
        self.transform(
            text,
            &scan_with_config(text, config.scan_config()),
            config.transformation_config(),
        )
        .await
        .map_err(|error| error.prefixed("/transform"))
    }
}

impl<P: KeyProvider, T: TokenProvider> PrivacyManager<P, T> {
    /// Transform after resolving keys, then atomically creating every token.
    pub async fn transform_with_context(
        &self,
        text: &str,
        findings: &[Finding],
        config: &TransformationConfig,
        context: Option<&PrivacyContext>,
    ) -> Result<TransformResult, PrivacyError> {
        let selected = select_findings(text, findings, config)?;
        let mut keys = Vec::new();
        for selector in key_selectors(config, &selected) {
            if !self.provider.is_configured() {
                return Err(PrivacyError::provider_required(selector.path));
            }
            let key = self
                .provider
                .resolve_key(selector.clone())
                .await
                .map_err(|error| PrivacyError::from_provider_error(selector.path.clone(), error))?;
            validate_resolved_key(&selector, &key)?;
            keys.push(ResolvedKeyBinding::new(selector, key));
        }
        let items = required_tokenization_items(text, findings, config, context)?;
        let token_results = if items.is_empty() {
            Vec::new()
        } else {
            if !self.token_provider.is_configured() {
                let path = tokenization_selections(config, &selected)
                    .into_iter()
                    .next()
                    .map(|(_, path)| path)
                    .unwrap_or_else(|| "/context/scope".to_owned());
                return Err(PrivacyError::token_provider_required(path));
            }
            let scope = context
                .ok_or_else(|| PrivacyError::token_provider_required("/context/scope"))?
                .scope();
            self.token_provider
                .tokenize_batch(scope, items.clone())
                .await
                .map_err(PrivacyError::from_token_provider_error)?
        };
        transform_with_provider_results(text, findings, config, context, keys, token_results)
    }

    /// Scan and transform with request-level provider authorization context.
    pub async fn scan_and_transform_with_context(
        &self,
        text: &str,
        config: &ScanAndTransformConfig,
        context: Option<&PrivacyContext>,
    ) -> Result<TransformResult, PrivacyError> {
        self.transform_with_context(
            text,
            &scan_with_config(text, config.scan_config()),
            config.transformation_config(),
            context,
        )
        .await
        .map_err(|error| error.prefixed("/transform"))
    }

    /// Restore every canonical token in a document as one atomic operation.
    pub async fn restore(
        &self,
        text: &str,
        context: &PrivacyContext,
    ) -> Result<RestoreResult, PrivacyError> {
        let items = required_restore_items(text, context)?;
        if items.is_empty() {
            return Ok(RestoreResult {
                text: text.to_owned(),
                restorations: Vec::new(),
            });
        }
        if !self.token_provider.is_configured() {
            return Err(PrivacyError::token_provider_required("/restore"));
        }
        let results = self
            .token_provider
            .restore_batch(context.scope(), items)
            .await
            .map_err(PrivacyError::from_token_provider_error)?;
        restore_with_results(text, context, results)
    }
}

/// Scan text and transform the resulting findings in one explicit convenience operation.
pub fn scan_and_transform(
    text: &str,
    config: &ScanAndTransformConfig,
) -> Result<TransformResult, PrivacyError> {
    transform(
        text,
        &scan_with_config(text, config.scan_config()),
        config.transformation_config(),
    )
    .map_err(|error| error.prefixed("/transform"))
}

fn duplicate_preference(candidate: &Finding, existing: &Finding) -> std::cmp::Ordering {
    if let (Some(candidate_confidence), Some(existing_confidence)) =
        (candidate.confidence, existing.confidence)
    {
        let confidence_order = existing_confidence.total_cmp(&candidate_confidence);
        if !confidence_order.is_eq() {
            return confidence_order;
        }
    }

    (&candidate.detector_name, &candidate.detector_version)
        .cmp(&(&existing.detector_name, &existing.detector_version))
}

// Input is validated, deduplicated, and sorted in source order.
fn resolve_overlaps(mut findings: Vec<Finding>) -> Vec<Finding> {
    if findings
        .windows(2)
        .all(|pair| pair[0].byte_range.end <= pair[1].byte_range.start)
    {
        return findings;
    }

    // Containment agrees with descending code-point length on validated ranges.
    // Within one length, confidence is comparable only if all values are present
    // or all are absent. Mixing them can make the preference cyclic, so retain
    // the original pairwise selection in that case.
    let mut confidence_by_length = BTreeMap::new();
    for finding in &findings {
        let length = finding.codepoint_range.end - finding.codepoint_range.start;
        let has_confidence = finding.confidence.is_some();
        let previous = confidence_by_length.entry(length).or_insert(has_confidence);
        if *previous != has_confidence {
            return resolve_overlaps_pairwise(findings);
        }
    }

    findings.sort_by(overlap_preference);
    let mut selected: BTreeMap<usize, Finding> = BTreeMap::new();
    for finding in findings {
        let start = finding.byte_range.start;
        // Accepted intervals never overlap. The closest predecessor and
        // successor suffice, and tree insertion avoids shifting a sorted Vec.
        let overlaps_previous = selected
            .range(..=start)
            .next_back()
            .is_some_and(|(_, previous)| previous.byte_range.end > start);
        let overlaps_next = selected
            .range(start..)
            .next()
            .is_some_and(|(&next_start, _)| next_start < finding.byte_range.end);
        if !overlaps_previous && !overlaps_next {
            selected.insert(start, finding);
        }
    }
    selected.into_values().collect()
}

fn resolve_overlaps_pairwise(mut remaining: Vec<Finding>) -> Vec<Finding> {
    let mut selected = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let mut preferred_index = 0;
        for candidate_index in 1..remaining.len() {
            if overlap_preference(&remaining[candidate_index], &remaining[preferred_index]).is_lt()
            {
                preferred_index = candidate_index;
            }
        }

        let preferred = remaining.remove(preferred_index);
        remaining.retain(|finding| !findings_overlap(&preferred, finding));
        selected.push(preferred);
    }

    selected.sort_by(|left, right| {
        left.byte_range
            .start
            .cmp(&right.byte_range.start)
            .then_with(|| left.byte_range.end.cmp(&right.byte_range.end))
            .then_with(|| left.entity_type.cmp(&right.entity_type))
    });
    selected
}

fn findings_overlap(left: &Finding, right: &Finding) -> bool {
    left.byte_range.start < right.byte_range.end && right.byte_range.start < left.byte_range.end
}

fn overlap_preference(left: &Finding, right: &Finding) -> std::cmp::Ordering {
    let left_contains_right = left.byte_range.start <= right.byte_range.start
        && left.byte_range.end >= right.byte_range.end;
    let right_contains_left = right.byte_range.start <= left.byte_range.start
        && right.byte_range.end >= left.byte_range.end;
    match (left_contains_right, right_contains_left) {
        (true, false) => return std::cmp::Ordering::Less,
        (false, true) => return std::cmp::Ordering::Greater,
        _ => {}
    }

    let left_length = left.codepoint_range.end - left.codepoint_range.start;
    let right_length = right.codepoint_range.end - right.codepoint_range.start;
    let length_order = right_length.cmp(&left_length);
    if !length_order.is_eq() {
        return length_order;
    }

    if let (Some(left_confidence), Some(right_confidence)) = (left.confidence, right.confidence) {
        let confidence_order = right_confidence.total_cmp(&left_confidence);
        if !confidence_order.is_eq() {
            return confidence_order;
        }
    }

    left.codepoint_range
        .start
        .cmp(&right.codepoint_range.start)
        .then_with(|| left.entity_type.cmp(&right.entity_type))
        .then_with(|| left.detector_name.cmp(&right.detector_name))
        .then_with(|| left.detector_version.cmp(&right.detector_version))
}

#[cfg(test)]
fn validate_finding(text: &str, finding: &Finding) -> Result<(), FindingValidationError> {
    validate_finding_with_index(text, finding, &mut TextIndex::new(text))
}

fn validate_finding_with_index(
    text: &str,
    finding: &Finding,
    offsets: &mut TextIndex<'_>,
) -> Result<(), FindingValidationError> {
    if finding.byte_range.start >= finding.byte_range.end {
        return Err(FindingValidationError::EmptyOrReversedByteRange);
    }
    if finding.byte_range.end > text.len() {
        return Err(FindingValidationError::ByteRangeOutOfBounds);
    }
    if !text.is_char_boundary(finding.byte_range.start)
        || !text.is_char_boundary(finding.byte_range.end)
    {
        return Err(FindingValidationError::InvalidUtf8Boundary);
    }
    if finding.codepoint_range.start >= finding.codepoint_range.end {
        return Err(FindingValidationError::EmptyOrReversedCodepointRange);
    }

    let Some(codepoint_start_byte) =
        offsets.byte_offset_at_codepoint(finding.codepoint_range.start)
    else {
        return Err(FindingValidationError::CodepointRangeOutOfBounds);
    };
    let Some(codepoint_end_byte) = offsets.byte_offset_at_codepoint(finding.codepoint_range.end)
    else {
        return Err(FindingValidationError::CodepointRangeOutOfBounds);
    };
    if codepoint_start_byte != finding.byte_range.start
        || codepoint_end_byte != finding.byte_range.end
    {
        return Err(FindingValidationError::InconsistentRanges);
    }
    if text[finding.byte_range.start..finding.byte_range.end] != finding.matched_text {
        return Err(FindingValidationError::MatchedTextMismatch);
    }
    if finding
        .confidence
        .is_some_and(|confidence| !confidence.is_finite() || !(0.0..=1.0).contains(&confidence))
    {
        return Err(FindingValidationError::InvalidConfidence);
    }
    Ok(())
}

#[cfg(test)]
fn byte_offset_at_codepoint(text: &str, codepoint_offset: usize) -> Option<usize> {
    text.char_indices()
        .map(|(byte_offset, _)| byte_offset)
        .chain(std::iter::once(text.len()))
        .nth(codepoint_offset)
}

fn finalize(text: &str, mut candidates: Vec<Candidate>) -> Vec<Finding> {
    candidates.sort_by(|left, right| {
        left.start_byte
            .cmp(&right.start_byte)
            .then_with(|| right.end_byte.cmp(&left.end_byte))
            .then_with(|| left.label.cmp(&right.label))
    });

    candidates.dedup_by(|left, right| {
        left.label == right.label
            && left.start_byte == right.start_byte
            && left.end_byte == right.end_byte
    });

    let is_ascii = text.is_ascii();
    let mut offsets = TextIndex::new(text);

    candidates
        .into_iter()
        .map(|candidate| {
            debug_assert!(text.is_char_boundary(candidate.start_byte));
            debug_assert!(text.is_char_boundary(candidate.end_byte));

            let start = if is_ascii {
                candidate.start_byte
            } else {
                offsets.codepoint_offset(candidate.start_byte)
            };

            let end = if is_ascii {
                candidate.end_byte
            } else {
                offsets.codepoint_offset(candidate.end_byte)
            };

            Finding {
                entity_type: candidate.label.as_str().to_owned(),
                matched_text: text[candidate.start_byte..candidate.end_byte].to_owned(),
                byte_range: TextRange {
                    start: candidate.start_byte,
                    end: candidate.end_byte,
                },
                codepoint_range: TextRange { start, end },
                confidence: None,
                detector_name: candidate.label.detector_name().to_owned(),
                detector_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }
        })
        .collect()
}

static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?:
            (?:\+?1[\s.-]?)?
            (?:\(\d{3}\)|\d{3})
            [\s.-]?\d{3}[\s.-]?\d{4}
          |
            \+\d(?:[\s.-]?\d){6,14}
        )",
    )
    .expect("phone regex is valid")
});

fn has_phone_boundaries(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();

    let before_is_alphanumeric = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
    let after_is_alphanumeric = end < bytes.len() && bytes[end].is_ascii_alphanumeric();

    !before_is_alphanumeric && !after_is_alphanumeric
}

fn is_valid_phone(candidate: &str) -> bool {
    let digits: String = candidate
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .map(char::from)
        .collect();

    if candidate.starts_with('+') {
        (7..=15).contains(&digits.len())
    } else {
        digits.len() == 10 || (digits.len() == 11 && digits.starts_with('1'))
    }
}

fn detect_phone(text: &str, candidates: &mut Vec<Candidate>) {
    for matched in PHONE_RE.find_iter(text) {
        if !has_phone_boundaries(text, matched.start(), matched.end()) {
            continue;
        }

        if !is_valid_phone(matched.as_str()) {
            continue;
        }

        candidates.push(Candidate {
            label: Label::Phone,
            start_byte: matched.start(),
            end_byte: matched.end(),
        });
    }
}

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[A-Z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[A-Z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?\.)+[A-Z]{2,}",
    )
    .expect("email regex is valid")
});

fn detect_email(text: &str, candidates: &mut Vec<Candidate>) {
    for matched in EMAIL_RE.find_iter(text) {
        candidates.push(Candidate {
            label: Label::Email,
            start_byte: matched.start(),
            end_byte: matched.end(),
        });
    }
}

fn digits_value(digits: &[u8]) -> u16 {
    digits
        .iter()
        .fold(0, |value, digit| value * 10 + u16::from(*digit - b'0'))
}

fn digits_value_u32(digits: &[u8]) -> u32 {
    digits
        .iter()
        .fold(0, |value, digit| value * 10 + u32::from(*digit - b'0'))
}

fn is_valid_ssn(area: u16, group: u16, serial: u16) -> bool {
    area != 0 && area != 666 && area < 900 && group != 0 && serial != 0
}

fn ssn_parts_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u16, u16)> {
    if start + 11 <= bytes.len()
        && bytes[start + 3] == b'-'
        && bytes[start + 6] == b'-'
        && bytes[start..start + 3]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && bytes[start + 4..start + 6]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && bytes[start + 7..start + 11]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some((
            start + 11,
            digits_value(&bytes[start..start + 3]),
            digits_value(&bytes[start + 4..start + 6]),
            digits_value(&bytes[start + 7..start + 11]),
        ));
    }

    if start + 9 <= bytes.len()
        && bytes[start..start + 9]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return Some((
            start + 9,
            digits_value(&bytes[start..start + 3]),
            digits_value(&bytes[start + 3..start + 5]),
            digits_value(&bytes[start + 5..start + 9]),
        ));
    }

    None
}

fn detect_ssn(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some((end, area, group, serial)) = ssn_parts_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        if is_valid_ssn(area, group, serial) {
            candidates.push(Candidate {
                label: Label::Ssn,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
    }
}

fn passes_luhn(digits: &[u8]) -> bool {
    let mut sum = 0_u32;

    for (position, digit) in digits.iter().rev().enumerate() {
        let mut value = u32::from(*digit - b'0');

        if position % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }

        sum += value;
    }

    sum % 10 == 0
}

fn has_supported_card_prefix(digits: &[u8]) -> bool {
    match digits {
        [b'4', ..] => true,                                           // Visa
        [b'3', b'4' | b'7', ..] => true,                              // American Express
        [b'5', second, ..] if (b'1'..=b'5').contains(second) => true, // Mastercard
        [b'2', _, _, _, ..] => (2221..=2720).contains(&digits_value(&digits[..4])),
        [b'6', b'0', b'1', b'1', ..] => true, // Discover 6011
        [b'6', b'5', ..] => true,             // Discover 65
        [b'6', b'4', third, ..] if (b'4'..=b'9').contains(third) => true, // Discover 644–649
        [b'6', b'2', _, _, _, _, ..] => (622126..=622925).contains(&digits_value_u32(&digits[..6])),
        _ => false,
    }
}

fn card_parts_at(bytes: &[u8], start: usize) -> (usize, Vec<u8>) {
    let mut end = start;
    let mut last_digit_end = start;
    let mut digits = Vec::with_capacity(19);
    let mut previous_was_separator = false;

    while end < bytes.len() {
        let byte = bytes[end];

        if byte.is_ascii_digit() {
            if digits.len() == 19 {
                break;
            }

            digits.push(byte);
            end += 1;
            last_digit_end = end;
            previous_was_separator = false;
        } else if (byte == b' ' || byte == b'-') && !previous_was_separator {
            end += 1;
            previous_was_separator = true;
        } else {
            break;
        }
    }

    (last_digit_end, digits)
}

fn detect_credit_card(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let (end, digits) = card_parts_at(bytes, start);

        if end == start || (end < bytes.len() && bytes[end].is_ascii_alphanumeric()) {
            start += 1;
            continue;
        }

        if (13..=19).contains(&digits.len())
            && has_supported_card_prefix(&digits)
            && passes_luhn(&digits)
        {
            candidates.push(Candidate {
                label: Label::CreditCard,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
    }
}

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_valid_date(year: u16, month: u8, day: u8) -> bool {
    year >= 1000 && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

fn fixed_digits_at(bytes: &[u8], start: usize, width: usize) -> Option<u16> {
    let end = start.checked_add(width)?;

    if end <= bytes.len() && bytes[start..end].iter().all(|byte| byte.is_ascii_digit()) {
        Some(digits_value(&bytes[start..end]))
    } else {
        None
    }
}

fn numeric_date_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    if start + 10 <= bytes.len() && bytes[start + 4] == b'-' && bytes[start + 7] == b'-' {
        let year = fixed_digits_at(bytes, start, 4)?;
        let month = fixed_digits_at(bytes, start + 5, 2)? as u8;
        let day = fixed_digits_at(bytes, start + 8, 2)? as u8;

        return Some((start + 10, year, month, day));
    }

    if start + 10 <= bytes.len()
        && (bytes[start + 2] == b'/' || bytes[start + 2] == b'-')
        && bytes[start + 5] == bytes[start + 2]
    {
        let month = fixed_digits_at(bytes, start, 2)? as u8;
        let day = fixed_digits_at(bytes, start + 3, 2)? as u8;
        let year = fixed_digits_at(bytes, start + 6, 4)?;

        return Some((start + 10, year, month, day));
    }

    None
}

const MONTH_NAMES: [(&[u8], u8); 23] = [
    (b"january", 1),
    (b"jan", 1),
    (b"february", 2),
    (b"feb", 2),
    (b"march", 3),
    (b"mar", 3),
    (b"april", 4),
    (b"apr", 4),
    (b"may", 5),
    (b"june", 6),
    (b"jun", 6),
    (b"july", 7),
    (b"jul", 7),
    (b"august", 8),
    (b"aug", 8),
    (b"september", 9),
    (b"sep", 9),
    (b"october", 10),
    (b"oct", 10),
    (b"november", 11),
    (b"nov", 11),
    (b"december", 12),
    (b"dec", 12),
];

fn month_name_at(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    for (name, month) in MONTH_NAMES {
        let end = start + name.len();

        if end <= bytes.len()
            && bytes[start..end].eq_ignore_ascii_case(name)
            && (end == bytes.len() || !bytes[end].is_ascii_alphabetic())
        {
            return Some((end, month));
        }
    }

    None
}

fn named_date_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    let (mut cursor, month) = month_name_at(bytes, start)?;

    if bytes.get(cursor) != Some(&b' ') {
        return None;
    }
    cursor += 1;

    let day_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && cursor - day_start < 2 {
        cursor += 1;
    }

    if cursor == day_start || (cursor < bytes.len() && bytes[cursor].is_ascii_digit()) {
        return None;
    }

    let day = digits_value(&bytes[day_start..cursor]) as u8;

    if bytes.get(cursor) != Some(&b',') {
        return None;
    }
    cursor += 1;

    if bytes.get(cursor) != Some(&b' ') {
        return None;
    }
    cursor += 1;

    let year = fixed_digits_at(bytes, cursor, 4)?;
    Some((cursor + 4, year, month, day))
}

fn date_parts_at(bytes: &[u8], start: usize) -> Option<(usize, u16, u8, u8)> {
    numeric_date_at(bytes, start).or_else(|| named_date_at(bytes, start))
}

fn detect_date(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if (!bytes[start].is_ascii_digit() && !bytes[start].is_ascii_alphabetic())
            || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some((end, year, month, day)) = date_parts_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        if is_valid_date(year, month, day) {
            candidates.push(Candidate {
                label: Label::Date,
                start_byte: start,
                end_byte: end,
            });
            start = end;
        } else {
            start += 1;
        }
    }
}

fn zip_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    let five_digit_end = start + 5;

    if five_digit_end > bytes.len()
        || !bytes[start..five_digit_end]
            .iter()
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    if bytes.get(five_digit_end) == Some(&b'-') {
        let zip_plus_four_end = five_digit_end + 5;

        if zip_plus_four_end <= bytes.len()
            && bytes[five_digit_end + 1..zip_plus_four_end]
                .iter()
                .all(|byte| byte.is_ascii_digit())
        {
            return Some(zip_plus_four_end);
        }

        return None;
    }

    Some(five_digit_end)
}

fn detect_zip_code(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        let Some(end) = zip_end_at(bytes, start) else {
            start += 1;
            continue;
        };

        if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }

        candidates.push(Candidate {
            label: Label::ZipCode,
            start_byte: start,
            end_byte: end,
        });
        start = end;
    }
}

fn ipv4_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;

    for group in 0..4 {
        let group_start = end;

        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }

        if !(1..=3).contains(&(end - group_start)) {
            return None;
        }

        if group < 3 {
            if bytes.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
    }

    Some(end)
}

fn ipv6_end_at(bytes: &[u8], start: usize) -> Option<usize> {
    if !bytes[start].is_ascii_hexdigit() && bytes[start] != b':' {
        return None;
    }

    let mut end = start;
    let mut has_colon = false;

    while end < bytes.len() && (bytes[end].is_ascii_hexdigit() || bytes[end] == b':') {
        has_colon |= bytes[end] == b':';
        end += 1;
    }

    has_colon.then_some(end)
}

fn has_ip_boundaries(bytes: &[u8], start: usize, end: usize) -> bool {
    (start == 0 || !bytes[start - 1].is_ascii_alphanumeric())
        && (end == bytes.len() || !bytes[end].is_ascii_alphanumeric())
}

fn is_part_of_longer_ipv4(bytes: &[u8], start: usize, end: usize) -> bool {
    (start >= 2 && bytes[start - 1] == b'.' && bytes[start - 2].is_ascii_digit())
        || (bytes.get(end) == Some(&b'.')
            && bytes.get(end + 1).is_some_and(|byte| byte.is_ascii_digit()))
}

fn is_valid_ipv4(bytes: &[u8], start: usize, end: usize) -> bool {
    let Ok(candidate) = std::str::from_utf8(&bytes[start..end]) else {
        return false;
    };

    candidate.parse::<Ipv4Addr>().is_ok()
}

fn is_valid_ipv6(bytes: &[u8], start: usize, end: usize) -> bool {
    let Ok(candidate) = std::str::from_utf8(&bytes[start..end]) else {
        return false;
    };

    candidate.parse::<Ipv6Addr>().is_ok()
}

fn detect_ip_address(text: &str, candidates: &mut Vec<Candidate>) {
    let bytes = text.as_bytes();
    let mut start = 0;

    while start < bytes.len() {
        if (!bytes[start].is_ascii_hexdigit() && bytes[start] != b':')
            || (start > 0 && bytes[start - 1].is_ascii_alphanumeric())
        {
            start += 1;
            continue;
        }

        if let Some(end) = ipv4_end_at(bytes, start) {
            if has_ip_boundaries(bytes, start, end)
                && !is_part_of_longer_ipv4(bytes, start, end)
                && is_valid_ipv4(bytes, start, end)
            {
                candidates.push(Candidate {
                    label: Label::IpAddress,
                    start_byte: start,
                    end_byte: end,
                });
                start = end;
                continue;
            }
        }

        if let Some(end) = ipv6_end_at(bytes, start) {
            if has_ip_boundaries(bytes, start, end) && is_valid_ipv6(bytes, start, end) {
                candidates.push(Candidate {
                    label: Label::IpAddress,
                    start_byte: start,
                    end_byte: end,
                });
                start = end;
                continue;
            }
        }

        start += 1;
    }
}

#[cfg(test)]
mod selection_tests;

#[cfg(test)]
mod tests {
    use super::{
        Finding, FindingValidationError, KeyProvider, KeyProviderError, KeyProviderErrorKind,
        KeyProviderFuture, KeySelector, MAX_REGEX_PATTERN_BYTES, MAX_REGEX_RULES, MaskConfig,
        MaskConfigError, MaskReveal, NoKeyProvider, PrivacyContext, PrivacyError, PrivacyErrorCode,
        PrivacyErrorReason, PrivacyManager, RegexAllowRule, ResolvedKey, RestoreProviderFuture,
        RestoredValue, ScanAndTransformConfig, TextRange, TokenProvider, TokenProviderError,
        TokenProviderErrorKind, TokenizeProviderFuture, TokenizeResult, TransformationConfig,
        TransformationStrategy, parse_scan_and_transform_config, parse_transformation_config,
        required_restore_items, restore_with_results, scan, scan_and_transform, transform,
        utf16_range,
    };
    use futures::executor::block_on;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn utf16_ranges_match_javascript_string_offsets() {
        let text = "👋 jane@example.com";

        assert_eq!(
            utf16_range(text, TextRange { start: 5, end: 21 }),
            Ok(TextRange { start: 3, end: 19 })
        );
    }

    #[test]
    fn utf16_ranges_reject_invalid_byte_boundaries() {
        assert!(utf16_range("👋", TextRange { start: 1, end: 4 }).is_err());
    }

    #[derive(Default)]
    struct TestKeyProvider {
        responses: BTreeMap<(String, Option<String>), (Vec<u8>, String)>,
        failure: Option<KeyProviderErrorKind>,
        calls: Mutex<Vec<(String, Option<String>)>>,
        order: Option<Arc<Mutex<Vec<&'static str>>>>,
    }

    impl TestKeyProvider {
        fn with_key(
            mut self,
            key_ref: &str,
            requested_version: Option<&str>,
            key: Vec<u8>,
            resolved_version: &str,
        ) -> Self {
            self.responses.insert(
                (key_ref.to_owned(), requested_version.map(str::to_owned)),
                (key, resolved_version.to_owned()),
            );
            self
        }

        fn failing(kind: KeyProviderErrorKind) -> Self {
            Self {
                failure: Some(kind),
                ..Self::default()
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn with_order(mut self, order: Arc<Mutex<Vec<&'static str>>>) -> Self {
            self.order = Some(order);
            self
        }
    }

    impl KeyProvider for TestKeyProvider {
        fn resolve_key(&self, selector: KeySelector) -> KeyProviderFuture<'_> {
            if let Some(order) = &self.order {
                order.lock().unwrap().push("key");
            }
            let identity = (
                selector.key_ref().to_owned(),
                selector.key_version().map(str::to_owned),
            );
            self.calls.lock().unwrap().push(identity.clone());
            let response = self.responses.get(&identity).cloned();
            let failure = self.failure;
            Box::pin(async move {
                if let Some(kind) = failure {
                    return Err(KeyProviderError::new(kind));
                }
                let (key, version) = response
                    .ok_or_else(|| KeyProviderError::new(KeyProviderErrorKind::NotFound))?;
                Ok(ResolvedKey::new(key, version))
            })
        }
    }

    type TokenRecord = (String, String, String, String);

    #[derive(Clone, Default)]
    struct TestTokenProvider {
        next: Arc<AtomicUsize>,
        records: Arc<Mutex<BTreeMap<Vec<u8>, TokenRecord>>>,
        tokenize_calls: Arc<Mutex<Vec<(String, usize)>>>,
        order: Option<Arc<Mutex<Vec<&'static str>>>>,
    }

    impl TokenProvider for TestTokenProvider {
        fn tokenize_batch(
            &self,
            scope: &str,
            items: Vec<super::TokenizeItem>,
        ) -> TokenizeProviderFuture<'_> {
            if let Some(order) = &self.order {
                order.lock().unwrap().push("token");
            }
            let scope = scope.to_owned();
            let records = Arc::clone(&self.records);
            let next = Arc::clone(&self.next);
            self.tokenize_calls
                .lock()
                .unwrap()
                .push((scope.clone(), items.len()));
            Box::pin(async move {
                Ok(items
                    .into_iter()
                    .map(|item| {
                        let payload = next.fetch_add(1, Ordering::SeqCst).to_be_bytes().to_vec();
                        records.lock().unwrap().insert(
                            payload.clone(),
                            (
                                scope.clone(),
                                item.token_ref().to_owned(),
                                "active-7".to_owned(),
                                item.exact_value().to_owned(),
                            ),
                        );
                        TokenizeResult::new(item.id(), payload, "active-7")
                    })
                    .collect())
            })
        }

        fn restore_batch(
            &self,
            scope: &str,
            items: Vec<super::RestoreItem>,
        ) -> RestoreProviderFuture<'_> {
            let scope = scope.to_owned();
            let records = Arc::clone(&self.records);
            Box::pin(async move {
                let records = records.lock().unwrap();
                items
                    .into_iter()
                    .map(|item| {
                        let Some((bound_scope, token_ref, version, value)) =
                            records.get(item.payload())
                        else {
                            return Err(TokenProviderError::new(TokenProviderErrorKind::NotFound));
                        };
                        if bound_scope != &scope
                            || token_ref != item.token_ref()
                            || version != item.resolved_version()
                        {
                            return Err(TokenProviderError::new(
                                TokenProviderErrorKind::AccessDenied,
                            ));
                        }
                        Ok(RestoredValue::new(item.id(), value))
                    })
                    .collect()
            })
        }
    }

    impl TestTokenProvider {
        fn with_order(mut self, order: Arc<Mutex<Vec<&'static str>>>) -> Self {
            self.order = Some(order);
            self
        }
    }

    fn config(strategy: TransformationStrategy) -> TransformationConfig {
        TransformationConfig::new(strategy)
    }

    fn assert_transformation_source(transformation: &super::Transformation, finding: &Finding) {
        assert_eq!(transformation.entity_type, finding.entity_type);
        assert_eq!(transformation.source_byte_range, finding.byte_range);
        assert_eq!(
            transformation.source_codepoint_range,
            finding.codepoint_range
        );
        assert_eq!(transformation.confidence, finding.confidence);
        assert_eq!(transformation.detector_name, finding.detector_name);
        assert_eq!(transformation.detector_version, finding.detector_version);
    }

    fn expected_finding(
        entity_type: &str,
        matched_text: &str,
        byte_range: (usize, usize),
        codepoint_range: (usize, usize),
        detector_name: &str,
    ) -> Finding {
        Finding {
            entity_type: entity_type.to_owned(),
            matched_text: matched_text.to_owned(),
            byte_range: TextRange {
                start: byte_range.0,
                end: byte_range.1,
            },
            codepoint_range: TextRange {
                start: codepoint_range.0,
                end: codepoint_range.1,
            },
            confidence: None,
            detector_name: detector_name.to_owned(),
            detector_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    }

    fn expected_ascii_finding(
        entity_type: &str,
        matched_text: &str,
        start: usize,
        end: usize,
        detector_name: &str,
    ) -> Finding {
        expected_finding(
            entity_type,
            matched_text,
            (start, end),
            (start, end),
            detector_name,
        )
    }

    fn supplied_ascii_finding(
        text: &str,
        entity_type: &str,
        start: usize,
        end: usize,
        confidence: Option<f32>,
        detector_name: &str,
    ) -> Finding {
        Finding {
            entity_type: entity_type.to_owned(),
            matched_text: text[start..end].to_owned(),
            byte_range: TextRange { start, end },
            codepoint_range: TextRange { start, end },
            confidence,
            detector_name: detector_name.to_owned(),
            detector_version: Some("1".to_owned()),
        }
    }

    #[test]
    fn empty_input_has_no_entities() {
        assert!(scan("").is_empty());
    }

    #[test]
    fn detects_email_without_trailing_punctuation() {
        assert_eq!(
            scan("Contact jane@example.com."),
            vec![expected_ascii_finding(
                "EMAIL",
                "jane@example.com",
                8,
                24,
                "datafog-core/email",
            )]
        );
    }

    #[test]
    fn reports_email_byte_and_codepoint_ranges() {
        let text = "👋 jane@example.com";
        assert_eq!(
            scan(text),
            vec![expected_finding(
                "EMAIL",
                "jane@example.com",
                (5, 21),
                (2, 18),
                "datafog-core/email",
            )]
        );

        let finding = &scan(text)[0];
        assert_eq!(
            &text[finding.byte_range.start..finding.byte_range.end],
            finding.matched_text
        );
        assert_eq!(
            text.chars()
                .skip(finding.codepoint_range.start)
                .take(finding.codepoint_range.end - finding.codepoint_range.start)
                .collect::<String>(),
            finding.matched_text
        );
    }

    #[test]
    fn detects_north_american_phone_number() {
        assert_eq!(
            scan("Call (212) 555-0100."),
            vec![expected_ascii_finding(
                "PHONE",
                "(212) 555-0100",
                5,
                19,
                "datafog-core/phone",
            )]
        );
    }

    #[test]
    fn detects_international_phone_number() {
        assert_eq!(
            scan("Intl +44 20 7946 0958"),
            vec![expected_ascii_finding(
                "PHONE",
                "+44 20 7946 0958",
                5,
                21,
                "datafog-core/phone",
            )]
        );
    }

    #[test]
    fn rejects_short_or_embedded_phone_candidates() {
        assert!(scan("Call 555-0100").is_empty());
        assert!(scan("order212-555-0100x").is_empty());
    }

    #[test]
    fn detects_dashed_ssn() {
        assert_eq!(
            scan("SSN: 123-45-6789"),
            vec![expected_ascii_finding(
                "SSN",
                "123-45-6789",
                5,
                16,
                "datafog-core/ssn",
            )]
        );
    }

    #[test]
    fn detects_undashed_ssn() {
        assert_eq!(scan("123456789")[0].entity_type, "SSN");
    }

    #[test]
    fn rejects_invalid_or_embedded_ssn_candidates() {
        assert!(scan("000-12-3456").is_empty());
        assert!(scan("666-12-3456").is_empty());
        assert!(scan("900-12-3456").is_empty());
        assert!(scan("123-00-4567").is_empty());
        assert!(scan("123-45-0000").is_empty());
        assert!(scan("ref123-45-6789x").is_empty());
    }

    #[test]
    fn detects_formatted_credit_card() {
        assert_eq!(
            scan("Card: 4111-1111-1111-1111"),
            vec![expected_ascii_finding(
                "CREDIT_CARD",
                "4111-1111-1111-1111",
                6,
                25,
                "datafog-core/credit-card",
            )]
        );
    }

    #[test]
    fn detects_supported_card_issuers() {
        assert_eq!(scan("5555 5555 5555 4444")[0].entity_type, "CREDIT_CARD");
        assert_eq!(scan("378282246310005")[0].entity_type, "CREDIT_CARD");
        assert_eq!(scan("6011111111111117")[0].entity_type, "CREDIT_CARD");
    }

    #[test]
    fn rejects_invalid_credit_card_candidates() {
        assert!(scan("4111-1111-1111-1112").is_empty()); // bad Luhn checksum
        assert!(scan("1234-5678-9012-3452").is_empty()); // unsupported prefix
        assert!(scan("x4111-1111-1111-1111y").is_empty()); // embedded
    }

    #[test]
    fn detects_numeric_and_named_dates() {
        assert_eq!(
            scan("Date: 2024-02-29"),
            vec![expected_ascii_finding(
                "DATE",
                "2024-02-29",
                6,
                16,
                "datafog-core/date",
            )]
        );

        assert_eq!(scan("12/27/1988")[0].entity_type, "DATE");
        assert_eq!(scan("Jan 15, 2024")[0].entity_type, "DATE");
    }

    #[test]
    fn rejects_invalid_or_unsupported_dates() {
        assert!(scan("2023-02-29").is_empty());
        assert!(scan("2024-02-30").is_empty());
        assert!(scan("12/27/88").is_empty());
        assert!(scan("27/12/2024").is_empty());
        assert!(scan("version2024-02-29x").is_empty());
    }

    #[test]
    fn detects_five_digit_and_plus_four_zip_codes() {
        assert_eq!(
            scan("ZIP: 94105-1234"),
            vec![expected_ascii_finding(
                "ZIP_CODE",
                "94105-1234",
                5,
                15,
                "datafog-core/zip-code",
            )]
        );

        assert_eq!(scan("94105")[0].entity_type, "ZIP_CODE");
    }

    #[test]
    fn rejects_invalid_or_embedded_zip_codes() {
        assert!(scan("1234").is_empty());
        assert!(scan("123456").is_empty());
        assert!(scan("x94105").is_empty());
        assert!(scan("94105x").is_empty());
        assert!(scan("94105-123").is_empty());
    }

    #[test]
    fn detects_ipv4_and_ipv6_addresses() {
        assert_eq!(
            scan("IPv4: 192.168.1.10"),
            vec![expected_ascii_finding(
                "IP_ADDRESS",
                "192.168.1.10",
                6,
                18,
                "datafog-core/ip-address",
            )]
        );

        assert_eq!(scan("2001:db8::1")[0].entity_type, "IP_ADDRESS");
        assert_eq!(scan("[2001:db8::1]:443")[0].matched_text, "2001:db8::1");
        assert_eq!(scan("192.168.1.10:8080")[0].matched_text, "192.168.1.10");
    }

    #[test]
    fn rejects_invalid_or_embedded_ip_addresses() {
        assert!(scan("256.1.1.1").is_empty());
        assert!(scan("1.2.3.4.5").is_empty());
        assert!(scan("2001:db8::zzzz").is_empty());
        assert!(scan("host192.168.1.10name").is_empty());
    }

    #[test]
    fn redacts_explicit_findings_and_reports_output_ranges() {
        let text = "Contact jane@example.com";
        let findings = scan(text);

        let result = transform(text, &findings, &config(TransformationStrategy::Redact)).unwrap();

        assert_eq!(result.text, "Contact [EMAIL]");
        assert_eq!(result.transformations.len(), 1);
        let transformation = &result.transformations[0];
        assert_transformation_source(transformation, &findings[0]);
        assert_eq!(transformation.strategy, TransformationStrategy::Redact);
        assert_eq!(transformation.replacement, "[EMAIL]");
        assert_eq!(
            transformation.output_byte_range,
            TextRange { start: 8, end: 15 }
        );
        assert_eq!(
            transformation.output_codepoint_range,
            TextRange { start: 8, end: 15 }
        );
    }

    #[test]
    fn parses_pseudonymization_and_requires_a_runtime_provider() {
        let config = parse_transformation_config(&json!({
            "default": {
                "strategy": "pseudonymize",
                "key_ref": "customers/email",
                "key_version": "7"
            }
        }))
        .unwrap();
        let text = "Email jane@example.com";
        let error = transform(text, &scan(text), &config).unwrap_err();

        assert_eq!(error.code(), PrivacyErrorCode::KeyProviderRequired);
        assert_eq!(error.path(), Some("/default/key_ref"));

        for invalid in [
            json!({"default": {"strategy": "pseudonymize"}}),
            json!({"default": {"strategy": "pseudonymize", "key_ref": " "}}),
            json!({
                "default": {
                    "strategy": "pseudonymize",
                    "key_ref": "customers/email",
                    "key_version": ""
                }
            }),
            json!({
                "default": {
                    "strategy": "pseudonymize",
                    "key_ref": "customers/email",
                    "algorithm": "sha512"
                }
            }),
        ] {
            assert!(parse_transformation_config(&invalid).is_err());
        }
    }

    #[test]
    fn pseudonymizes_exact_utf8_with_full_padded_base64_hmac_sha256() {
        let config = parse_transformation_config(&json!({
            "default": {
                "strategy": "pseudonymize",
                "key_ref": "customers/email",
                "key_version": "7"
            }
        }))
        .unwrap();
        let manager = PrivacyManager::new(TestKeyProvider::default().with_key(
            "customers/email",
            Some("7"),
            (0_u8..32).collect(),
            "7",
        ));

        let result = block_on(manager.transform(
            "Email jane@example.com",
            &scan("Email jane@example.com"),
            &config,
        ))
        .unwrap();

        assert_eq!(
            result.text,
            "Email lIdYiXR1nTA9XURAF5GmA62F/aknbUP3Q2B31wnZ2hA="
        );
        let record = &result.transformations[0];
        assert_eq!(record.replacement.len(), 44);
        assert_eq!(record.key_ref.as_deref(), Some("customers/email"));
        assert_eq!(record.resolved_key_version.as_deref(), Some("7"));
        assert_eq!(record.entity_type, "EMAIL");
        assert_eq!(record.source_byte_range, TextRange { start: 6, end: 22 });
        assert_eq!(manager.provider().call_count(), 1);
    }

    #[test]
    fn the_key_alone_defines_linkage_scope_across_entity_types() {
        let text = "jane@example.com jane@example.com";
        let mut findings = scan(text);
        findings[1].entity_type = "CUSTOM_IDENTIFIER".to_owned();
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "pseudonymize", "key_ref": "shared"}
        }))
        .unwrap();
        let manager = PrivacyManager::new(TestKeyProvider::default().with_key(
            "shared",
            None,
            vec![42; 32],
            "2026-08-27",
        ));

        let result = block_on(manager.transform(text, &findings, &config)).unwrap();

        assert_eq!(result.transformations.len(), 2);
        assert_eq!(
            result.transformations[0].replacement,
            result.transformations[1].replacement
        );
        assert_eq!(manager.provider().call_count(), 1);
    }

    #[test]
    fn exact_input_and_key_material_changes_produce_different_pseudonyms() {
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "pseudonymize", "key_ref": "key"}
        }))
        .unwrap();
        let lower = "jane@example.com";
        let upper = "Jane@example.com";
        let lower_finding = supplied_ascii_finding(lower, "EMAIL", 0, lower.len(), None, "test");
        let upper_finding = supplied_ascii_finding(upper, "EMAIL", 0, upper.len(), None, "test");
        let manager_a =
            PrivacyManager::new(TestKeyProvider::default().with_key("key", None, vec![1; 32], "1"));
        let manager_b =
            PrivacyManager::new(TestKeyProvider::default().with_key("key", None, vec![2; 32], "2"));

        let lower_a = block_on(manager_a.transform(lower, &[lower_finding], &config)).unwrap();
        let upper_a =
            block_on(manager_a.transform(upper, &[upper_finding.clone()], &config)).unwrap();
        let upper_b = block_on(manager_b.transform(upper, &[upper_finding], &config)).unwrap();

        assert_ne!(
            lower_a.transformations[0].replacement,
            upper_a.transformations[0].replacement
        );
        assert_ne!(
            upper_a.transformations[0].replacement,
            upper_b.transformations[0].replacement
        );
    }

    #[test]
    fn multiple_selected_keys_are_deduplicated_and_resolved_before_application() {
        let text = "jane@example.com jane@example.com (212) 555-0100";
        let findings = scan(text);
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "pseudonymize", "key_ref": "email-key"},
            "overrides": {
                "PHONE": {"strategy": "pseudonymize", "key_ref": "phone-key", "key_version": "9"}
            }
        }))
        .unwrap();
        let manager = PrivacyManager::new(
            TestKeyProvider::default()
                .with_key("email-key", None, vec![3; 32], "4")
                .with_key("phone-key", Some("9"), vec![4; 32], "9"),
        );

        let result = block_on(manager.transform(text, &findings, &config)).unwrap();

        assert_eq!(result.transformations.len(), 3);
        assert_eq!(manager.provider().call_count(), 2);
        assert_eq!(
            result.transformations[0].resolved_key_version.as_deref(),
            Some("4")
        );
        assert_eq!(
            result.transformations[1].resolved_key_version.as_deref(),
            Some("4")
        );
        assert_eq!(
            result.transformations[2].resolved_key_version.as_deref(),
            Some("9")
        );
    }

    #[test]
    fn invalid_or_unavailable_keys_fail_closed_without_provider_retries() {
        let text = "Email jane@example.com";
        let findings = scan(text);
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "pseudonymize", "key_ref": "key"}
        }))
        .unwrap();
        let invalid_manager =
            PrivacyManager::new(TestKeyProvider::default().with_key("key", None, vec![0; 31], "1"));
        let unavailable_manager =
            PrivacyManager::new(TestKeyProvider::failing(KeyProviderErrorKind::Unavailable));

        let invalid = block_on(invalid_manager.transform(text, &findings, &config)).unwrap_err();
        let unavailable =
            block_on(unavailable_manager.transform(text, &findings, &config)).unwrap_err();

        assert_eq!(invalid.code(), PrivacyErrorCode::InvalidKeyMaterial);
        assert_eq!(unavailable.code(), PrivacyErrorCode::KeyProviderUnavailable);
        assert_eq!(unavailable_manager.provider().call_count(), 1);
    }

    #[test]
    fn unused_pseudonymization_keys_are_not_resolved() {
        let text = "Email support@example.com";
        let findings = scan(text);
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "pseudonymize", "key_ref": "key"},
            "allow": {"exact": {"EMAIL": ["support@example.com"]}}
        }))
        .unwrap();
        let manager = PrivacyManager::new(TestKeyProvider::failing(
            KeyProviderErrorKind::ProviderError,
        ));

        let result = block_on(manager.transform(text, &findings, &config)).unwrap();

        assert_eq!(result.text, text);
        assert!(result.transformations.is_empty());
        assert_eq!(manager.provider().call_count(), 0);
    }

    #[test]
    fn resolved_key_debug_output_redacts_material() {
        let key = ResolvedKey::new(vec![7; 32], "1");
        let debug = format!("{key:?}");

        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("7, 7"));
    }

    #[test]
    fn entity_override_replaces_the_default_strategy() {
        let text = "Email jane@example.com or call (212) 555-0100";
        let findings = scan(text);
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_override(
                "PHONE",
                TransformationStrategy::Mask(MaskConfig::new('*', MaskReveal::Last(4)).unwrap()),
            )
            .unwrap();

        let result = transform(text, &findings, &config).unwrap();

        assert_eq!(result.text, "Email [EMAIL] or call **********0100");
        assert_eq!(
            result.transformations[0].strategy,
            TransformationStrategy::Redact
        );
        assert!(matches!(
            result.transformations[1].strategy,
            TransformationStrategy::Mask(_)
        ));
    }

    #[test]
    fn entity_selection_transforms_only_exact_selected_types() {
        let text = "Email jane@example.com or call (212) 555-0100";
        let findings = scan(text);
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_entities(vec!["PHONE".to_owned()])
            .unwrap();

        let result = transform(text, &findings, &config).unwrap();

        assert_eq!(result.text, "Email jane@example.com or call [PHONE]");
        assert_eq!(result.transformations.len(), 1);
        assert_eq!(result.transformations[0].entity_type, "PHONE");
    }

    #[test]
    fn exact_allowlist_is_entity_scoped_and_applied_before_overlap_resolution() {
        let text = "212-555-0100@example.com";
        let email = supplied_ascii_finding(text, "EMAIL", 0, text.len(), Some(0.9), "email");
        let phone = supplied_ascii_finding(text, "PHONE", 0, 12, Some(0.8), "phone");
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_exact_allowlist("EMAIL", vec![text.to_owned(), text.to_owned()])
            .unwrap();

        let result = transform(text, &[email, phone], &config).unwrap();

        assert_eq!(result.text, "[PHONE]@example.com");
        assert_eq!(result.transformations.len(), 1);
        assert_eq!(result.transformations[0].entity_type, "PHONE");
    }

    #[test]
    fn entity_selection_happens_before_overlap_resolution() {
        let text = "Acme Corporation";
        let unselected_outer = supplied_ascii_finding(
            text,
            "ORGANIZATION",
            0,
            text.len(),
            Some(0.9),
            "organization",
        );
        let selected_inner = supplied_ascii_finding(text, "PERSON", 0, 4, Some(0.8), "person");
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_entities(vec!["PERSON".to_owned()])
            .unwrap();

        let result = transform(text, &[unselected_outer, selected_inner], &config).unwrap();

        assert_eq!(result.text, "[PERSON] Corporation");
    }

    #[test]
    fn regex_allowlists_use_full_match_and_explicit_case_sensitivity() {
        let text = "allowed@example.com ADMIN@EXAMPLE.COM";
        let lower = supplied_ascii_finding(text, "EMAIL", 0, 19, None, "test");
        let upper = supplied_ascii_finding(text, "EMAIL", 20, text.len(), None, "test");
        let sensitive = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("EMAIL", vec![RegexAllowRule::new(r".*@example\.com", true)])
            .unwrap();

        let result = transform(text, &[lower.clone(), upper.clone()], &sensitive).unwrap();
        assert_eq!(result.text, "allowed@example.com [EMAIL]");

        let insensitive = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist(
                "EMAIL",
                vec![
                    RegexAllowRule::new(r".*@example\.com", false),
                    RegexAllowRule::new(r".*@example\.com", false),
                ],
            )
            .unwrap();
        let explicit = transform(text, &[lower, upper], &insensitive).unwrap();
        let convenience =
            scan_and_transform(text, &ScanAndTransformConfig::new(insensitive)).unwrap();
        assert_eq!(explicit.text, text);
        assert_eq!(convenience, explicit);
    }

    #[test]
    fn configuration_errors_expose_stable_machine_readable_fields() {
        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_entities(Vec::new())
            .unwrap_err();

        assert_eq!(error.code(), PrivacyErrorCode::InvalidConfiguration);
        assert_eq!(error.reason(), Some(PrivacyErrorReason::EmptyValue));
        assert_eq!(error.path(), Some("/entities"));
        assert_eq!(error.finding_index(), None);
    }

    #[test]
    fn canonical_serialized_envelope_drives_selection_overrides_and_allowlists() {
        let text = "Email support@example.com or call (212) 555-0100";
        let findings = scan(text);
        let config = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "entities": ["EMAIL", "PHONE"],
            "overrides": {
                "PHONE": {
                    "strategy": "mask",
                    "reveal": { "direction": "last", "count": 4 }
                }
            },
            "allow": {
                "exact": { "EMAIL": ["support@example.com"] },
                "regex": {}
            }
        }))
        .unwrap();

        let result = transform(text, &findings, &config).unwrap();
        let convenience = scan_and_transform(text, &ScanAndTransformConfig::new(config)).unwrap();

        assert_eq!(
            result.text,
            "Email support@example.com or call **********0100"
        );
        assert_eq!(result.transformations.len(), 1);
        assert_eq!(result.transformations[0].entity_type, "PHONE");
        assert_eq!(convenience, result);
    }

    #[test]
    fn serialized_configuration_rejects_unknown_fields_and_null() {
        let unknown = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "overides": {}
        }))
        .unwrap_err();
        assert_eq!(unknown.reason(), Some(PrivacyErrorReason::UnknownField));
        assert_eq!(unknown.path(), Some("/overides"));

        let explicit_null = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "allow": null
        }))
        .unwrap_err();
        assert_eq!(
            explicit_null.reason(),
            Some(PrivacyErrorReason::InvalidType)
        );
        assert_eq!(explicit_null.path(), Some("/allow"));
    }

    #[test]
    fn serialized_configuration_distinguishes_empty_structure_from_empty_semantics() {
        let accepted = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "overrides": {},
            "allow": { "exact": {}, "regex": {} }
        }));
        assert!(accepted.is_ok());

        let duplicate_entity = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "entities": ["EMAIL", "EMAIL"]
        }))
        .unwrap_err();
        assert_eq!(
            duplicate_entity.reason(),
            Some(PrivacyErrorReason::DuplicateValue)
        );
        assert_eq!(duplicate_entity.path(), Some("/entities/1"));

        let empty_allowlist = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "allow": { "exact": { "EMAIL": [] } }
        }))
        .unwrap_err();
        assert_eq!(
            empty_allowlist.reason(),
            Some(PrivacyErrorReason::EmptyValue)
        );
        assert_eq!(empty_allowlist.path(), Some("/allow/exact/EMAIL"));
    }

    #[test]
    fn exact_allowlists_compare_unicode_values_without_normalizing_them() {
        let text = "Name José";
        let finding = Finding {
            entity_type: "PERSON".to_owned(),
            matched_text: "José".to_owned(),
            byte_range: TextRange { start: 5, end: 10 },
            codepoint_range: TextRange { start: 5, end: 9 },
            confidence: None,
            detector_name: "test".to_owned(),
            detector_version: None,
        };
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_exact_allowlist("PERSON", vec!["José".to_owned()])
            .unwrap();

        let result = transform(text, &[finding], &config).unwrap();

        assert_eq!(result.text, text);
        assert!(result.transformations.is_empty());
    }

    #[test]
    fn scan_and_transform_uses_the_divided_configuration_envelope() {
        let config = parse_scan_and_transform_config(&json!({
            "scan": { "locale": "en-US" },
            "transform": {
                "default": { "strategy": "redact" },
                "entities": ["EMAIL"]
            }
        }))
        .unwrap();

        assert_eq!(config.scan_config().locale(), Some("en-US"));
        assert_eq!(
            scan_and_transform("Email jane@example.com", &config)
                .unwrap()
                .text,
            "Email [EMAIL]"
        );

        let error = parse_scan_and_transform_config(&json!({
            "transform": { "default": { "strategy": "redact", "extra": true } }
        }))
        .unwrap_err();
        assert_eq!(error.path(), Some("/transform/default/extra"));
    }

    #[test]
    fn valid_configuration_for_unselected_entities_remains_dormant() {
        let config = parse_transformation_config(&json!({
            "default": { "strategy": "redact" },
            "entities": ["EMAIL"],
            "overrides": { "PHONE": { "strategy": "remove" } },
            "allow": {
                "exact": { "PERSON": ["Jane Example"] },
                "regex": {
                    "CUSTOM": [{ "pattern": "value-[0-9]+" }]
                }
            }
        }))
        .unwrap();

        let text = "Email jane@example.com or call (212) 555-0100";
        let result = transform(text, &scan(text), &config).unwrap();

        assert_eq!(result.text, "Email [EMAIL] or call (212) 555-0100");
        assert_eq!(result.transformations.len(), 1);
    }

    #[test]
    fn regex_allowlist_limits_reject_the_complete_configuration() {
        let too_many = (0..=MAX_REGEX_RULES)
            .map(|index| RegexAllowRule::new(format!("value-{index}"), true))
            .collect();
        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("CUSTOM", too_many)
            .unwrap_err();
        assert_eq!(error.reason(), Some(PrivacyErrorReason::LimitExceeded));
        assert_eq!(error.path(), Some("/allow/regex"));

        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist(
                "CUSTOM",
                vec![RegexAllowRule::new(
                    "x".repeat(MAX_REGEX_PATTERN_BYTES + 1),
                    true,
                )],
            )
            .unwrap_err();
        assert_eq!(error.reason(), Some(PrivacyErrorReason::LimitExceeded));

        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("CUSTOM", vec![RegexAllowRule::new("(", true)])
            .unwrap_err();
        assert_eq!(error.reason(), Some(PrivacyErrorReason::InvalidRegex));

        let aggregate = (0..11)
            .map(|index| RegexAllowRule::new(format!("{}-{index}", "x".repeat(950)), true))
            .collect();
        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("CUSTOM", aggregate)
            .unwrap_err();
        assert_eq!(error.reason(), Some(PrivacyErrorReason::LimitExceeded));
        assert_eq!(error.path(), Some("/allow/regex"));

        let error = TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("CUSTOM", vec![RegexAllowRule::new(r"\w{1000}", true)])
            .unwrap_err();
        assert_eq!(error.reason(), Some(PrivacyErrorReason::LimitExceeded));
    }

    #[test]
    fn fully_masks_every_codepoint_including_punctuation() {
        let text = "Email jane@example.com";
        let findings = scan(text);
        let strategy = TransformationStrategy::Mask(MaskConfig::default());

        let result = transform(text, &findings, &config(strategy.clone())).unwrap();

        assert_eq!(result.text, "Email ****************");
        assert_eq!(result.transformations[0].strategy, strategy);
        assert_eq!(result.transformations[0].replacement, "****************");
    }

    #[test]
    fn partial_masking_reveals_the_requested_edge() {
        let text = "Email jane@example.com";
        let findings = scan(text);

        let reveal_first =
            TransformationStrategy::Mask(MaskConfig::new('*', MaskReveal::First(4)).unwrap());
        let reveal_last =
            TransformationStrategy::Mask(MaskConfig::new('*', MaskReveal::Last(4)).unwrap());

        assert_eq!(
            transform(text, &findings, &config(reveal_first))
                .unwrap()
                .text,
            "Email jane************"
        );
        assert_eq!(
            transform(text, &findings, &config(reveal_last))
                .unwrap()
                .text,
            "Email ************.com"
        );
    }

    #[test]
    fn reveal_counts_handle_zero_and_the_finding_length() {
        let text = "Email jane@example.com";
        let findings = scan(text);
        let reveal_none =
            TransformationStrategy::Mask(MaskConfig::new('*', MaskReveal::First(0)).unwrap());
        let reveal_all = TransformationStrategy::Mask(
            MaskConfig::new('*', MaskReveal::Last(usize::MAX)).unwrap(),
        );

        assert_eq!(
            transform(text, &findings, &config(reveal_none))
                .unwrap()
                .text,
            "Email ****************"
        );
        assert_eq!(
            transform(text, &findings, &config(reveal_all))
                .unwrap()
                .text,
            text
        );
    }

    #[test]
    fn masking_rejects_whitespace_and_control_characters() {
        for character in [' ', '\n', '\0'] {
            assert_eq!(
                MaskConfig::new(character, MaskReveal::None),
                Err(MaskConfigError::InvalidCharacter)
            );
        }
        assert!(MaskConfig::new('•', MaskReveal::None).is_ok());
    }

    #[test]
    fn multibyte_mask_character_reports_exact_output_ranges() {
        let text = "A é👋 Z";
        let finding = Finding {
            entity_type: "CUSTOM".to_owned(),
            matched_text: "é👋".to_owned(),
            byte_range: TextRange { start: 2, end: 8 },
            codepoint_range: TextRange { start: 2, end: 4 },
            confidence: None,
            detector_name: "test".to_owned(),
            detector_version: None,
        };
        let strategy =
            TransformationStrategy::Mask(MaskConfig::new('•', MaskReveal::None).unwrap());

        let result = transform(text, &[finding], &config(strategy)).unwrap();

        assert_eq!(result.text, "A •• Z");
        assert_eq!(
            result.transformations[0].output_byte_range,
            TextRange { start: 2, end: 8 }
        );
        assert_eq!(
            result.transformations[0].output_codepoint_range,
            TextRange { start: 2, end: 4 }
        );
    }

    #[test]
    fn removal_deletes_only_the_finding_and_records_the_deletion_point() {
        let text = "Email jane@example.com today";
        let findings = scan(text);

        let result = transform(text, &findings, &config(TransformationStrategy::Remove)).unwrap();

        assert_eq!(result.text, "Email  today");
        assert_eq!(result.transformations[0].replacement, "");
        assert_eq!(
            result.transformations[0].output_byte_range,
            TextRange { start: 6, end: 6 }
        );
        assert_eq!(
            result.transformations[0].output_codepoint_range,
            TextRange { start: 6, end: 6 }
        );
    }

    #[test]
    fn rejects_a_finding_whose_matched_text_differs_from_the_source() {
        let text = "Contact jane@example.com";
        let mut findings = scan(text);
        findings[0].matched_text = "other@example.com".to_owned();

        assert_eq!(
            transform(text, &findings, &config(TransformationStrategy::Redact)),
            Err(PrivacyError::invalid_finding(
                0,
                FindingValidationError::MatchedTextMismatch,
            ))
        );
    }

    #[test]
    fn rejects_malformed_ranges_and_confidence() {
        let text = "👋 jane@example.com";
        let original = scan(text).remove(0);

        let cases = [
            (
                Finding {
                    byte_range: TextRange { start: 5, end: 5 },
                    ..original.clone()
                },
                FindingValidationError::EmptyOrReversedByteRange,
            ),
            (
                Finding {
                    byte_range: TextRange { start: 5, end: 99 },
                    ..original.clone()
                },
                FindingValidationError::ByteRangeOutOfBounds,
            ),
            (
                Finding {
                    byte_range: TextRange { start: 1, end: 21 },
                    ..original.clone()
                },
                FindingValidationError::InvalidUtf8Boundary,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 2, end: 2 },
                    ..original.clone()
                },
                FindingValidationError::EmptyOrReversedCodepointRange,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 2, end: 99 },
                    ..original.clone()
                },
                FindingValidationError::CodepointRangeOutOfBounds,
            ),
            (
                Finding {
                    codepoint_range: TextRange { start: 1, end: 17 },
                    ..original.clone()
                },
                FindingValidationError::InconsistentRanges,
            ),
            (
                Finding {
                    confidence: Some(f32::NAN),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
            (
                Finding {
                    confidence: Some(-0.1),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
            (
                Finding {
                    confidence: Some(1.1),
                    ..original.clone()
                },
                FindingValidationError::InvalidConfidence,
            ),
        ];

        for (finding, expected_kind) in cases {
            assert_eq!(
                transform(text, &[finding], &config(TransformationStrategy::Redact)),
                Err(PrivacyError::invalid_finding(0, expected_kind))
            );
        }
    }

    #[test]
    fn collapses_duplicate_findings_and_retains_higher_confidence() {
        let text = "Email jane@example.com";
        let mut lower_confidence = scan(text).remove(0);
        lower_confidence.confidence = Some(0.7);
        lower_confidence.detector_name = "z-detector".to_owned();
        let mut higher_confidence = lower_confidence.clone();
        higher_confidence.confidence = Some(0.9);
        higher_confidence.detector_name = "a-detector".to_owned();

        let result = transform(
            text,
            &[lower_confidence, higher_confidence.clone()],
            &config(TransformationStrategy::Redact),
        )
        .unwrap();

        assert_eq!(result.text, "Email [EMAIL]");
        assert_eq!(result.transformations.len(), 1);
        assert_transformation_source(&result.transformations[0], &higher_confidence);
    }

    #[test]
    fn containing_overlap_wins_even_when_the_inner_finding_has_higher_confidence() {
        let text = "Acme Corporation announced";
        let outer = supplied_ascii_finding(
            text,
            "ORGANIZATION",
            0,
            16,
            Some(0.6),
            "organization-detector",
        );
        let inner = supplied_ascii_finding(text, "PERSON", 0, 4, Some(0.99), "person-detector");

        let result = transform(
            text,
            &[inner, outer.clone()],
            &config(TransformationStrategy::Redact),
        )
        .unwrap();

        assert_eq!(result.text, "[ORGANIZATION] announced");
        assert_eq!(result.transformations.len(), 1);
        assert_transformation_source(&result.transformations[0], &outer);
    }

    #[test]
    fn scan_and_transform_redacts_unicode_input_with_exact_output_ranges() {
        let text = "👋 jane@example.com and jane@example.com";

        let config = ScanAndTransformConfig::new(config(TransformationStrategy::Redact));
        let result = scan_and_transform(text, &config).unwrap();

        assert_eq!(result.text, "👋 [EMAIL] and [EMAIL]");
        assert_eq!(result.transformations.len(), 2);
        assert_eq!(
            result.transformations[0].output_byte_range,
            TextRange { start: 5, end: 12 }
        );
        assert_eq!(
            result.transformations[0].output_codepoint_range,
            TextRange { start: 2, end: 9 }
        );
        assert_eq!(
            &result.text[result.transformations[1].output_byte_range.start
                ..result.transformations[1].output_byte_range.end],
            "[EMAIL]"
        );
        assert_eq!(result.transformations[0].replacement, "[EMAIL]");
        assert_eq!(result.transformations[1].replacement, "[EMAIL]");
    }

    #[test]
    fn equal_length_overlaps_use_confidence_when_both_findings_provide_it() {
        let text = "123456789";
        let lower = supplied_ascii_finding(text, "ALPHA", 0, 9, Some(0.7), "a");
        let higher = supplied_ascii_finding(text, "ZETA", 0, 9, Some(0.9), "z");

        let result = transform(
            text,
            &[lower, higher.clone()],
            &config(TransformationStrategy::Redact),
        )
        .unwrap();

        assert_eq!(result.text, "[ZETA]");
        assert_transformation_source(&result.transformations[0], &higher);
    }

    #[test]
    fn missing_confidence_does_not_rank_as_zero() {
        let text = "123456789";
        let unscored = supplied_ascii_finding(text, "ALPHA", 0, 9, None, "z");
        let scored = supplied_ascii_finding(text, "ZETA", 0, 9, Some(0.99), "a");

        let result = transform(
            text,
            &[scored, unscored.clone()],
            &config(TransformationStrategy::Redact),
        )
        .unwrap();

        assert_eq!(result.text, "[ALPHA]");
        assert_transformation_source(&result.transformations[0], &unscored);
    }

    #[test]
    fn equal_partial_overlaps_prefer_the_earlier_source_position() {
        let text = "abcdef";
        let earlier = supplied_ascii_finding(text, "ZETA", 0, 4, None, "z");
        let later = supplied_ascii_finding(text, "ALPHA", 2, 6, None, "a");

        let result = transform(
            text,
            &[later, earlier.clone()],
            &config(TransformationStrategy::Redact),
        )
        .unwrap();

        assert_eq!(result.text, "[ZETA]ef");
        assert_transformation_source(&result.transformations[0], &earlier);
    }

    #[test]
    fn empty_findings_leave_text_unchanged() {
        let result = transform("plain text", &[], &config(TransformationStrategy::Redact)).unwrap();

        assert_eq!(result.text, "plain text");
        assert!(result.transformations.is_empty());
    }

    #[test]
    fn tokenization_round_trips_unicode_with_fresh_tokens_and_exact_ranges() {
        let text = "👋 jane@example.com jane@example.com";
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "customers/default"}
        }))
        .unwrap();
        let context = PrivacyContext::new("tenant/α").unwrap();
        let provider = TestTokenProvider::default();
        let manager = PrivacyManager::<NoKeyProvider, _>::token_provider_only(provider.clone());

        let transformed =
            block_on(manager.transform_with_context(text, &scan(text), &config, Some(&context)))
                .unwrap();

        assert_eq!(transformed.transformations.len(), 2);
        assert_ne!(
            transformed.transformations[0].replacement,
            transformed.transformations[1].replacement,
        );
        assert!(transformed.transformations.iter().all(|record| {
            record.strategy
                == TransformationStrategy::Tokenize(
                    super::TokenizeConfig::new("customers/default").unwrap(),
                )
                && record.token_ref.as_deref() == Some("customers/default")
                && record.resolved_token_version.as_deref() == Some("active-7")
                && record.key_ref.is_none()
        }));
        assert_eq!(
            provider.tokenize_calls.lock().unwrap().as_slice(),
            &[("tenant/α".to_owned(), 2)]
        );

        let restored = block_on(manager.restore(&transformed.text, &context)).unwrap();
        assert_eq!(restored.text, text);
        assert_eq!(restored.restorations.len(), 2);
        for record in &restored.restorations {
            assert!(
                transformed.text[record.source_byte_range.start..record.source_byte_range.end]
                    .starts_with("DFTOKENv1(")
            );
            assert_eq!(
                &restored.text[record.output_byte_range.start..record.output_byte_range.end],
                "jane@example.com",
            );
            assert_eq!(record.token_ref, "customers/default");
            assert_eq!(record.resolved_token_version, "active-7");
        }
    }

    #[test]
    fn restoration_is_atomic_and_scope_bound() {
        let text = "jane@example.com";
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "customers/default"}
        }))
        .unwrap();
        let context = PrivacyContext::new("tenant-a").unwrap();
        let manager =
            PrivacyManager::<NoKeyProvider, _>::token_provider_only(TestTokenProvider::default());
        let transformed =
            block_on(manager.transform_with_context(text, &scan(text), &config, Some(&context)))
                .unwrap();

        let error =
            block_on(manager.restore(&transformed.text, &PrivacyContext::new("tenant-b").unwrap()))
                .unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::TokenAccessDenied);

        let mut tampered = transformed.text.clone();
        let payload_start = tampered.rfind('.').unwrap() + 1;
        let replacement = if &tampered[payload_start..payload_start + 1] == "A" {
            "B"
        } else {
            "A"
        };
        tampered.replace_range(payload_start..payload_start + 1, replacement);
        let error = block_on(manager.restore(&tampered, &context)).unwrap_err();
        assert!(matches!(
            error.code(),
            PrivacyErrorCode::TokenNotFound | PrivacyErrorCode::InvalidToken
        ));
    }

    #[test]
    fn restore_rejects_malformed_versions_and_incomplete_provider_results() {
        let context = PrivacyContext::new("tenant").unwrap();
        assert_eq!(
            required_restore_items("DFTOKENv1(999):abc", &context)
                .unwrap_err()
                .code(),
            PrivacyErrorCode::InvalidToken
        );
        assert_eq!(
            required_restore_items("DFTOKENv1(008):YQ.Yg.Yw", &context)
                .unwrap_err()
                .code(),
            PrivacyErrorCode::InvalidToken
        );
        assert_eq!(
            required_restore_items("DFTOKENv2(3):abc", &context)
                .unwrap_err()
                .code(),
            PrivacyErrorCode::UnsupportedTokenVersion
        );
        let token = super::encode_token("profile", "1", b"payload");
        let error = restore_with_results(&token, &context, Vec::new()).unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::InvalidTokenMaterial);
        let ordinary = restore_with_results("ordinary text", &context, Vec::new()).unwrap();
        assert_eq!(ordinary.text, "ordinary text");
        assert!(ordinary.restorations.is_empty());
    }

    #[test]
    fn token_provider_responses_require_complete_unique_ids_but_allow_reordering() {
        let text = "jane@example.com jane@example.com";
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "profile"}
        }))
        .unwrap();
        let findings = scan(text);
        let context = PrivacyContext::new("tenant").unwrap();

        for invalid in [
            Vec::new(),
            vec![TokenizeResult::new("unexpected", vec![1], "1")],
            vec![
                TokenizeResult::new("0", vec![1], "1"),
                TokenizeResult::new("0", vec![2], "1"),
            ],
        ] {
            let error = super::transform_with_provider_results(
                text,
                &findings,
                &config,
                Some(&context),
                Vec::new(),
                invalid,
            )
            .unwrap_err();
            assert_eq!(error.code(), PrivacyErrorCode::InvalidTokenMaterial);
        }

        let result = super::transform_with_provider_results(
            text,
            &findings,
            &config,
            Some(&context),
            Vec::new(),
            vec![
                TokenizeResult::new("1", vec![2], "1"),
                TokenizeResult::new("0", vec![1], "1"),
            ],
        )
        .unwrap();
        assert_eq!(result.transformations.len(), 2);
    }

    #[test]
    fn token_provider_failures_have_sanitized_stable_categories() {
        for (kind, expected) in [
            (
                TokenProviderErrorKind::NotFound,
                PrivacyErrorCode::TokenNotFound,
            ),
            (
                TokenProviderErrorKind::Expired,
                PrivacyErrorCode::TokenExpired,
            ),
            (
                TokenProviderErrorKind::AccessDenied,
                PrivacyErrorCode::TokenAccessDenied,
            ),
            (
                TokenProviderErrorKind::Unavailable,
                PrivacyErrorCode::TokenProviderUnavailable,
            ),
            (
                TokenProviderErrorKind::ProviderError,
                PrivacyErrorCode::TokenProviderError,
            ),
        ] {
            let error =
                super::PrivacyError::from_token_provider_error(TokenProviderError::new(kind));
            assert_eq!(error.code(), expected);
            assert!(error.path().is_none());
        }
    }

    #[test]
    fn mixed_requests_resolve_keys_before_creating_tokens() {
        let text = "jane@example.com 2125550100";
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "profile"},
            "overrides": {
                "EMAIL": {"strategy": "pseudonymize", "key_ref": "email-key"}
            }
        }))
        .unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let key_provider = TestKeyProvider::default()
            .with_key("email-key", None, vec![7; 32], "1")
            .with_order(Arc::clone(&order));
        let token_provider = TestTokenProvider::default().with_order(Arc::clone(&order));
        let manager = PrivacyManager::new(key_provider).with_token_provider(token_provider);
        let result = block_on(manager.transform_with_context(
            text,
            &scan(text),
            &config,
            Some(&PrivacyContext::new("tenant").unwrap()),
        ))
        .unwrap();

        assert_eq!(order.lock().unwrap().as_slice(), &["key", "token"]);
        assert_eq!(result.transformations.len(), 2);
    }

    #[test]
    fn token_requests_and_results_redact_sensitive_debug_values() {
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "profile"}
        }))
        .unwrap();
        let context = PrivacyContext::new("secret-scope").unwrap();
        let items = super::required_tokenization_items(
            "jane@example.com",
            &scan("jane@example.com"),
            &config,
            Some(&context),
        )
        .unwrap();
        assert!(!format!("{context:?}").contains("secret-scope"));
        assert!(!format!("{:?}", items[0]).contains("jane@example.com"));
        assert!(
            !format!("{:?}", TokenizeResult::new("0", b"secret".to_vec(), "1")).contains("secret")
        );
    }

    #[test]
    fn token_configuration_is_strict_and_dormant_when_unselected() {
        for invalid in [
            json!({"default": {"strategy": "tokenize"}}),
            json!({"default": {"strategy": "tokenize", "token_ref": " "}}),
            json!({"default": {"strategy": "tokenize", "token_ref": "profile", "ttl": 30}}),
        ] {
            assert!(parse_transformation_config(&invalid).is_err());
        }
        let config = parse_transformation_config(&json!({
            "default": {"strategy": "redact"},
            "overrides": {"PHONE": {"strategy": "tokenize", "token_ref": "profile"}}
        }))
        .unwrap();
        let result = transform("jane@example.com", &scan("jane@example.com"), &config).unwrap();
        assert_eq!(result.text, "[EMAIL]");
    }

    #[test]
    fn optional_manager_capabilities_fail_only_when_selected() {
        let context = PrivacyContext::new("tenant").unwrap();
        let manager = PrivacyManager::new(NoKeyProvider);
        let ordinary = block_on(manager.restore("ordinary text", &context)).unwrap();
        assert_eq!(ordinary.text, "ordinary text");

        let config = parse_transformation_config(&json!({
            "default": {"strategy": "tokenize", "token_ref": "profile"}
        }))
        .unwrap();
        let error = block_on(manager.transform_with_context(
            "jane@example.com",
            &scan("jane@example.com"),
            &config,
            Some(&context),
        ))
        .unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::TokenProviderRequired);

        let token = super::encode_token("profile", "1", b"payload");
        let error = block_on(manager.restore(&token, &context)).unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::TokenProviderRequired);
    }

    #[test]
    fn tokenization_rejects_nested_tokens_and_restoration_is_not_recursive() {
        let context = PrivacyContext::new("tenant").unwrap();
        let inner = super::encode_token("profile", "1", b"inner");
        let finding = supplied_ascii_finding(&inner, "CUSTOM", 0, inner.len(), None, "custom");
        let config = TransformationConfig::new(TransformationStrategy::Tokenize(
            super::TokenizeConfig::new("profile").unwrap(),
        ));
        let error = super::required_tokenization_items(&inner, &[finding], &config, Some(&context))
            .unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::InvalidToken);

        let outer = super::encode_token("profile", "1", b"outer");
        let restored = restore_with_results(
            &outer,
            &context,
            vec![RestoredValue::new("0", inner.clone())],
        )
        .unwrap();
        assert_eq!(restored.text, inner);
        assert_eq!(restored.restorations.len(), 1);
    }
}
