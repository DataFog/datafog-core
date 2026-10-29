//! Model routing, span validation and evidence composition shared by native and browser adapters.
use crate::{Finding, PrivacyError, PrivacyErrorReason, ScanConfig, TextIndex, TextRange};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Stable sanitized error envelope for the new model API.
#[derive(Debug, Clone, Serialize)]
pub struct ModelError {
    pub code: String,
    pub reason: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_index: Option<usize>,
}
impl ModelError {
    pub(crate) fn new(code: &str, reason: &str, path: &str) -> Self {
        Self {
            code: code.into(),
            reason: reason.into(),
            path: path.into(),
            finding_index: None,
        }
    }
    pub(crate) fn config(reason: &str, path: &str) -> Self {
        Self::new("invalid_configuration", reason, path)
    }
    pub(crate) fn invalid_output() -> Self {
        Self::new("model_inference_failed", "invalid_output", "/backend")
    }
    #[cfg(all(feature = "models", not(target_arch = "wasm32")))]
    pub(crate) fn timeout() -> Self {
        Self::new("model_timeout", "deadline_exceeded", "/data")
    }
    #[cfg(all(feature = "models", not(target_arch = "wasm32")))]
    pub(crate) fn cleanup() -> Self {
        Self::new("model_cleanup_failed", "process_cleanup_failed", "/backend")
    }
    #[cfg(all(feature = "models", not(target_arch = "wasm32")))]
    pub(crate) fn closed() -> Self {
        Self::new("model_session_closed", "closed", "/backend")
    }
}
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "model operation failed ({}/{})", self.code, self.reason)
    }
}
impl std::error::Error for ModelError {}
impl From<PrivacyError> for ModelError {
    fn from(error: PrivacyError) -> Self {
        Self {
            code: error.code().as_str().into(),
            reason: error
                .reason()
                .map_or("operation_failed", |v| v.as_str())
                .into(),
            path: error.path().unwrap_or("").into(),
            finding_index: error.finding_index(),
        }
    }
}

/// Caller-selected bounds. There are no unmeasured universal resource defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub max_findings: usize,
    pub timeout_ms: u64,
    pub cleanup_timeout_ms: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateUnit {
    Utf8Byte,
    UnicodeCodepoint,
}

/// Raw normalized model scores are separate from calibrated confidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScore {
    pub value: f64,
    pub scale_id: String,
}

/// Shared adapter-to-Core span contract, independent of transport and binding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSpan {
    pub native_label: String,
    pub start: usize,
    pub end: usize,
    pub coordinate_unit: CoordinateUnit,
    pub raw_score: Option<RawScore>,
    pub calibrated_confidence: Option<f64>,
    pub matched_text: Option<String>,
}
/// Successful adapters certify complete input coverage; partial execution is an error.
#[derive(Debug)]
pub struct BackendResult {
    pub spans: Vec<NativeSpan>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceEvidence {
    pub detector_name: String,
    pub detector_version: Option<String>,
    pub native_label: Option<String>,
    pub raw_score: Option<RawScore>,
    pub calibrated_confidence: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct FindingEvidence {
    pub finding_index: usize,
    pub sources: Vec<SourceEvidence>,
}

/// Entity routing for a model session scan. Rule detectors handle every
/// requested entity type they support; partial rules also retain configured
/// model coverage for that type. Validated against one session's labels.
#[derive(Clone, Debug)]
pub struct RoutedScanConfig {
    pub(crate) rules: Option<ScanConfig>,
    pub(crate) model_entities: BTreeSet<String>,
}

impl RoutedScanConfig {
    /// Whether a scan with this configuration runs model inference.
    pub fn needs_model(&self) -> bool {
        !self.model_entities.is_empty()
    }
}

pub(crate) fn rule_entry(finding: Finding) -> (Finding, SourceEvidence) {
    let source = SourceEvidence {
        detector_name: finding.detector_name.clone(),
        detector_version: finding.detector_version.clone(),
        native_label: None,
        raw_score: None,
        calibrated_confidence: finding.confidence.map(f64::from),
    };
    (finding, source)
}

pub(crate) fn rule_entries(
    text: &str,
    config: &RoutedScanConfig,
) -> Vec<(Finding, SourceEvidence)> {
    config.rules.as_ref().map_or_else(Vec::new, |rules| {
        crate::scan_with_config(text, rules)
            .into_iter()
            .map(rule_entry)
            .collect()
    })
}

#[derive(Debug, Serialize)]
pub struct ModelScanResult {
    pub contract_version: u32,
    pub model: String,
    pub findings: Vec<Finding>,
    pub evidence: Vec<FindingEvidence>,
}

/// Configured model identity and label capabilities, independent of worker state.
/// This snapshot contains no local paths and does not probe runtime readiness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDescriptor {
    pub backend: String,
    pub model: String,
    pub adapter_version: String,
    pub labels: BTreeMap<String, String>,
    pub score_scale: String,
}

pub struct ModelScanContract {
    pub(crate) backend: String,
    pub(crate) model: String,
    pub(crate) adapter_version: String,
    pub(crate) labels: BTreeMap<String, String>,
    pub(crate) limits: ResourceLimits,
    pub(crate) score_scale: String,
    pub(crate) thresholds: BTreeMap<String, f64>,
}

impl ModelScanContract {
    /// Snapshot of this contract's configured model, labels and raw-score scale.
    pub fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            backend: self.backend.clone(),
            model: self.model.clone(),
            adapter_version: self.adapter_version.clone(),
            labels: self.labels.clone(),
            score_scale: self.score_scale.clone(),
        }
    }

    /// Contract for the pinned English GLiNER2.5 Small browser adapter.
    /// Custom maps explicitly associate model prompts with canonical entity types.
    pub fn gliner25_small(labels: Option<&serde_json::Value>) -> Result<Self, ModelError> {
        let labels = match labels {
            None => [
                ("person", "PERSON"),
                ("address", "ADDRESS"),
                ("email", "EMAIL"),
                ("phone number", "PHONE"),
                ("identification number", "IDENTIFICATION_NUMBER"),
                ("url", "URL"),
                ("username", "USERNAME"),
            ]
            .into_iter()
            .map(|(label, entity)| (label.to_owned(), entity.to_owned()))
            .collect(),
            Some(value) => {
                let object = value
                    .as_object()
                    .ok_or_else(|| ModelError::config("invalid_type", "/labels"))?;
                if object.is_empty() || object.len() > 16 {
                    return Err(ModelError::config("invalid_value", "/labels"));
                }
                let mut labels = BTreeMap::new();
                let mut prompts = BTreeSet::new();
                let mut entities = BTreeSet::new();
                for (label, entity) in object {
                    let entity = entity
                        .as_str()
                        .ok_or_else(|| ModelError::config("invalid_type", "/labels"))?;
                    if label.is_empty()
                        || label.trim() != label
                        || label.len() > 64
                        || label.chars().any(|ch| ch.is_control() || "[]".contains(ch))
                        || !prompts.insert(label.to_lowercase())
                        || entity.is_empty()
                        || entity.len() > 64
                        || !entity.starts_with(|ch: char| ch.is_ascii_uppercase())
                        || !entity
                            .chars()
                            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
                        || !entities.insert(entity)
                    {
                        return Err(ModelError::config("invalid_value", "/labels"));
                    }
                    labels.insert(label.clone(), entity.to_owned());
                }
                labels
            }
        };
        Ok(Self {
            backend: "gliner25_onnx_web".into(),
            model:
                "nicolasembleton/gliner2.5-small-v1-onnx@5e2e3f51adfb0eeb7c1f83464400b4d498d41659"
                    .into(),
            adapter_version: "gliner25-onnx-web-v1".into(),
            labels,
            limits: ResourceLimits {
                max_input_bytes: 65536,
                max_response_bytes: 4 * 1024 * 1024,
                max_findings: 1024,
                timeout_ms: 120000,
                cleanup_timeout_ms: 120000,
            },
            score_scale: "gliner25_pair_sigmoid".into(),
            thresholds: BTreeMap::new(),
        })
    }

    /// Model prompt labels in stable order. Canonical entity mapping stays in Core.
    pub fn native_labels(&self) -> Vec<String> {
        self.labels.keys().cloned().collect()
    }

    /// Validate the input before a binding starts model inference.
    pub fn check_input(&self, text: &str) -> Result<(), ModelError> {
        if text.len() > self.limits.max_input_bytes {
            return Err(ModelError::new(
                "model_resource_exhausted",
                "limit_exceeded",
                "/data",
            ));
        }
        Ok(())
    }

    /// Validate every model span before filtering, then compose rule and model findings.
    /// Passing no routing returns model-only findings, including rule-supported types.
    pub fn complete(
        &self,
        text: &str,
        result: BackendResult,
        routing: Option<&RoutedScanConfig>,
    ) -> Result<ModelScanResult, ModelError> {
        self.check_input(text)?;
        if result.spans.len() > self.limits.max_findings {
            return Err(ModelError::new(
                "model_resource_exhausted",
                "limit_exceeded",
                "/data",
            ));
        }
        let mut entries = convert(text, result, self)?;
        if let Some(config) = routing {
            entries.retain(|(finding, _)| config.model_entities.contains(&finding.entity_type));
            entries.splice(0..0, rule_entries(text, config));
        }
        let result = self.compose(entries)?;
        bounded_json(&result, self.limits.max_response_bytes, "/data")?;
        Ok(result)
    }

    /// Validate a routed scan configuration. It accepts the rule scan fields
    /// (`entities`, `locale`, `detect_uuid`, `format`); `entities` may also
    /// name this session's model labels. Without `entities`, every enabled
    /// rule detector runs and models retain types without replacement rules.
    pub fn routed_config(&self, value: &serde_json::Value) -> Result<RoutedScanConfig, ModelError> {
        let invalid = |reason, path: &str, message: &str| {
            ModelError::from(PrivacyError::invalid_configuration(reason, path, message))
        };
        let object = value.as_object().ok_or_else(|| {
            invalid(
                PrivacyErrorReason::InvalidType,
                "",
                "scan configuration must be an object",
            )
        })?;
        let model_labels: BTreeSet<&str> = self.labels.values().map(String::as_str).collect();
        let mut rule_object = object.clone();
        let (rule_entities, model_entities) = match object.get("entities") {
            None => (None, self.model_only_labels()),
            Some(entities) => {
                let entities = entities.as_array().ok_or_else(|| {
                    invalid(
                        PrivacyErrorReason::InvalidType,
                        "/entities",
                        "entities must be an array",
                    )
                })?;
                if entities.is_empty() {
                    return Err(invalid(
                        PrivacyErrorReason::EmptyValue,
                        "/entities",
                        "entities must contain at least one entity type",
                    ));
                }
                let mut seen = BTreeSet::new();
                let mut rules = Vec::new();
                let mut model = BTreeSet::new();
                for (index, entity) in entities.iter().enumerate() {
                    let path = format!("/entities/{index}");
                    let entity = entity.as_str().ok_or_else(|| {
                        invalid(
                            PrivacyErrorReason::InvalidType,
                            &path,
                            "entity type must be a string",
                        )
                    })?;
                    if !seen.insert(entity) {
                        return Err(invalid(
                            PrivacyErrorReason::DuplicateValue,
                            &path,
                            "entity selection contains a duplicate entity type",
                        ));
                    }
                    let supported_by_rules = crate::capabilities::text_entity_supported(entity);
                    if supported_by_rules {
                        rules.push(serde_json::Value::String(entity.to_owned()));
                    }
                    if model_labels.contains(entity)
                        && !crate::capabilities::rules_replace_model(entity)
                    {
                        model.insert(entity.to_owned());
                    } else if !supported_by_rules {
                        return Err(invalid(
                            PrivacyErrorReason::InvalidValue,
                            &path,
                            "entity type is not supported by rules or this model",
                        ));
                    }
                }
                (Some(rules), model)
            }
        };
        // Rule-routed entities were validated above, so remaining errors name
        // the shared locale, UUID and format fields at their own paths.
        let run_rules = match rule_entities {
            None => true,
            Some(rules) if rules.is_empty() => {
                rule_object.remove("entities");
                false
            }
            Some(rules) => {
                rule_object.insert("entities".into(), serde_json::Value::Array(rules));
                true
            }
        };
        let rules = crate::parse_scan_config(&serde_json::Value::Object(rule_object))?;
        Ok(RoutedScanConfig {
            rules: run_rules.then_some(rules),
            model_entities,
        })
    }

    /// The routing used without a configuration: all enabled rule detectors
    /// and model labels without replacement rules.
    pub fn default_routed_config(&self) -> RoutedScanConfig {
        RoutedScanConfig {
            rules: Some(ScanConfig::new()),
            model_entities: self.model_only_labels(),
        }
    }

    fn model_only_labels(&self) -> BTreeSet<String> {
        self.labels
            .values()
            .filter(|label| !crate::capabilities::rules_replace_model(label))
            .cloned()
            .collect()
    }

    pub(crate) fn compose(
        &self,
        entries: Vec<(Finding, SourceEvidence)>,
    ) -> Result<ModelScanResult, ModelError> {
        if entries.len() > self.limits.max_findings {
            return Err(ModelError::new(
                "model_resource_exhausted",
                "limit_exceeded",
                "/data",
            ));
        }
        let mut groups: BTreeMap<(usize, usize, String), (Finding, Vec<SourceEvidence>)> =
            BTreeMap::new();
        for (finding, source) in entries {
            let key = (
                finding.byte_range.start,
                finding.byte_range.end,
                finding.entity_type.clone(),
            );
            match groups.entry(key) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((finding, vec![source]));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let (existing, sources) = entry.get_mut();
                    if crate::duplicate_preference(&finding, existing).is_lt() {
                        *existing = finding;
                    }
                    sources.push(source);
                }
            }
        }
        let mut groups: Vec<_> = groups.into_values().collect();
        groups.sort_by(|(a, _), (b, _)| {
            a.byte_range
                .start
                .cmp(&b.byte_range.start)
                .then_with(|| b.byte_range.end.cmp(&a.byte_range.end))
                .then_with(|| a.entity_type.cmp(&b.entity_type))
        });
        let mut findings = Vec::with_capacity(groups.len());
        let mut evidence = Vec::with_capacity(groups.len());
        for (finding, sources) in groups {
            evidence.push(FindingEvidence {
                finding_index: findings.len(),
                sources,
            });
            findings.push(finding);
        }
        Ok(ModelScanResult {
            contract_version: 1,
            model: self.model.clone(),
            findings,
            evidence,
        })
    }
}

pub(crate) fn bounded_json(
    value: &impl Serialize,
    limit: usize,
    path: &str,
) -> Result<(), ModelError> {
    struct Counter {
        remaining: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.remaining = self
                .remaining
                .checked_sub(bytes.len())
                .ok_or_else(|| std::io::Error::other("size limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter { remaining: limit }, value)
        .map_err(|_| ModelError::config("limit_exceeded", path))
}

pub(crate) fn convert(
    text: &str,
    result: BackendResult,
    descriptor: &ModelScanContract,
) -> Result<Vec<(Finding, SourceEvidence)>, ModelError> {
    let mut offsets = TextIndex::new(text);
    let mut matched_bytes = 0usize;
    let converted: Result<Vec<_>, ModelError> = result
        .spans
        .into_iter()
        .enumerate()
        .map(|(index, span)| {
            let invalid = || {
                let mut error = ModelError::invalid_output();
                error.finding_index = Some(index);
                error
            };
            let entity = descriptor
                .labels
                .get(&span.native_label)
                .ok_or_else(invalid)?;
            for score in [
                span.raw_score.as_ref().map(|s| s.value),
                span.calibrated_confidence,
            ]
            .into_iter()
            .flatten()
            {
                if !score.is_finite() || !(0.0..=1.0).contains(&score) {
                    return Err(invalid());
                }
            }
            if span
                .raw_score
                .as_ref()
                .is_some_and(|s| s.scale_id != descriptor.score_scale)
            {
                return Err(invalid());
            }
            let (start, end) = match span.coordinate_unit {
                CoordinateUnit::Utf8Byte => (span.start, span.end),
                CoordinateUnit::UnicodeCodepoint => (
                    offsets
                        .byte_offset_at_codepoint(span.start)
                        .ok_or_else(invalid)?,
                    offsets
                        .byte_offset_at_codepoint(span.end)
                        .ok_or_else(invalid)?,
                ),
            };
            if start >= end
                || end > text.len()
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
            {
                return Err(invalid());
            }
            let matched_text = &text[start..end];
            matched_bytes = matched_bytes
                .checked_add(matched_text.len())
                .ok_or_else(invalid)?;
            if matched_bytes > descriptor.limits.max_response_bytes {
                return Err(invalid());
            }
            if span
                .matched_text
                .as_ref()
                .is_some_and(|value| value != matched_text)
            {
                return Err(invalid());
            }
            let backend_name = if matches!(
                descriptor.backend.as_str(),
                "gliner2_worker" | "gliner25_onnx_web"
            ) {
                "gliner2"
            } else {
                "datafog"
            };
            let detector_name = format!("datafog-core/model/{backend_name}/{}", span.native_label);
            let detector_version = Some(format!(
                "{}:{}",
                descriptor.adapter_version, descriptor.model
            ));
            let finding = Finding {
                entity_type: entity.clone(),
                matched_text: matched_text.into(),
                byte_range: TextRange { start, end },
                codepoint_range: TextRange {
                    start: offsets.codepoint_offset(start),
                    end: offsets.codepoint_offset(end),
                },
                confidence: span.calibrated_confidence.map(|v| v as f32),
                detector_name: detector_name.clone(),
                detector_version: detector_version.clone(),
            };
            crate::validate_finding_with_index(text, &finding, &mut offsets)
                .map_err(|_| invalid())?;
            Ok((
                finding,
                SourceEvidence {
                    detector_name,
                    detector_version,
                    native_label: Some(span.native_label),
                    raw_score: span.raw_score,
                    calibrated_confidence: span.calibrated_confidence,
                },
            ))
        })
        .collect();
    let mut converted = converted?;
    converted.retain(|(_, evidence)| {
        evidence
            .native_label
            .as_ref()
            .and_then(|label| descriptor.thresholds.get(label))
            .is_none_or(|threshold| {
                evidence
                    .raw_score
                    .as_ref()
                    .is_some_and(|score| score.value >= *threshold)
            })
    });
    Ok(converted)
}

#[cfg(test)]
mod browser_tests {
    use super::*;
    use serde_json::json;

    fn contract() -> ModelScanContract {
        ModelScanContract::gliner25_small(None).unwrap()
    }

    #[test]
    fn descriptor_uses_configured_labels_and_returns_an_independent_snapshot() {
        let contract =
            ModelScanContract::gliner25_small(Some(&json!({"company": "COMPANY"}))).unwrap();
        let mut descriptor = contract.describe();
        assert_eq!(descriptor.backend, "gliner25_onnx_web");
        assert_eq!(descriptor.model, contract.model);
        assert_eq!(descriptor.adapter_version, contract.adapter_version);
        assert_eq!(descriptor.score_scale, "gliner25_pair_sigmoid");
        assert_eq!(
            descriptor.labels,
            BTreeMap::from([("company".into(), "COMPANY".into())])
        );
        descriptor.labels.clear();
        assert_eq!(contract.native_labels(), vec!["company"]);
        assert_eq!(contract.describe().labels.len(), 1);
    }

    fn span(label: &str, start: usize, end: usize) -> NativeSpan {
        NativeSpan {
            native_label: label.into(),
            start,
            end,
            coordinate_unit: CoordinateUnit::Utf8Byte,
            raw_score: Some(RawScore {
                value: 0.9,
                scale_id: "gliner25_pair_sigmoid".into(),
            }),
            calibrated_confidence: None,
            matched_text: None,
        }
    }

    #[test]
    fn small_scan_routes_rules_and_preserves_uncalibrated_model_evidence() {
        let contract = contract();
        let text = "👋 José at jose@example.com";
        let routing = contract
            .routed_config(&json!({"entities": ["PERSON", "EMAIL"]}))
            .unwrap();
        assert!(routing.needs_model());
        let result = contract
            .complete(
                text,
                BackendResult {
                    spans: vec![span("person", 5, 10), span("email", 14, text.len())],
                },
                Some(&routing),
            )
            .unwrap();
        assert_eq!(result.findings.len(), 2);
        let person = &result.findings[0];
        assert_eq!(person.entity_type, "PERSON");
        assert_eq!(person.matched_text, "José");
        assert_eq!(person.codepoint_range, TextRange { start: 2, end: 6 });
        assert_eq!(person.confidence, None);
        assert!(person.detector_name.contains("/model/gliner2/"));
        assert!(
            person
                .detector_version
                .as_ref()
                .unwrap()
                .contains("5e2e3f51")
        );
        let evidence = &result.evidence[0].sources[0];
        assert_eq!(evidence.raw_score.as_ref().unwrap().value, 0.9);
        assert_eq!(evidence.calibrated_confidence, None);
        assert_eq!(result.findings[1].entity_type, "EMAIL");
        assert!(!result.findings[1].detector_name.contains("/model/"));
        assert!(result.evidence[1].sources[0].raw_score.is_none());
        let raw = contract
            .complete(
                text,
                BackendResult {
                    spans: vec![span("email", 14, text.len())],
                },
                None,
            )
            .unwrap();
        assert!(raw.findings[0].detector_name.contains("/model/"));
    }

    #[test]
    fn partial_account_rule_preserves_custom_model_coverage_and_evidence() {
        let contract = ModelScanContract::gliner25_small(Some(&json!({
            "bank account": "ACCOUNT_NUMBER", "email": "EMAIL"
        })))
        .unwrap();
        for config in [json!({}), json!({"entities": ["ACCOUNT_NUMBER"]})] {
            let routing = contract.routed_config(&config).unwrap();
            assert!(routing.needs_model());
            assert!(routing.model_entities.contains("ACCOUNT_NUMBER"));
            assert!(!routing.model_entities.contains("EMAIL"));
            assert!(routing.rules.is_some());
            for (text, model_value) in [
                ("👋 account number: 12345678", "12345678"),
                ("👋 12345678", "12345678"),
                ("👋 XX-12345678", "XX-12345678"),
                ("👋 account number: XX-12345678", "XX-12345678"),
            ] {
                let start = text.find(model_value).unwrap();
                let result = contract
                    .complete(
                        text,
                        BackendResult {
                            spans: vec![span("bank account", start, text.len())],
                        },
                        Some(&routing),
                    )
                    .unwrap();
                assert_eq!(result.findings.len(), 1, "{text}");
                assert_eq!(result.findings[0].entity_type, "ACCOUNT_NUMBER");
                assert_eq!(result.findings[0].matched_text, model_value);
                let sources = &result.evidence[0].sources;
                assert!(
                    sources
                        .iter()
                        .any(|source| source.native_label.as_deref() == Some("bank account"))
                );
                assert_eq!(
                    sources.len(),
                    if text.contains("account number:") && model_value == "12345678" {
                        2
                    } else {
                        1
                    }
                );
                let transformed = crate::transform(
                    text,
                    &result.findings,
                    &crate::parse_transformation_config(&json!({
                        "entities":["ACCOUNT_NUMBER"], "default":{"strategy":"redact"}
                    }))
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(
                    transformed.text,
                    format!("{}[ACCOUNT_NUMBER]", &text[..start])
                );
            }
        }
        let only_email = contract
            .routed_config(&json!({"entities":["EMAIL"]}))
            .unwrap();
        assert!(!only_email.needs_model());
        assert!(
            contract
                .default_routed_config()
                .model_entities
                .contains("ACCOUNT_NUMBER")
        );
    }

    #[test]
    fn small_rule_only_and_negative_scans_do_not_require_model_output() {
        let contract = contract();
        let routing = contract
            .routed_config(&json!({"entities": ["EMAIL"]}))
            .unwrap();
        assert!(!routing.needs_model());
        assert_eq!(
            contract
                .complete(
                    "jane@example.com",
                    BackendResult { spans: vec![] },
                    Some(&routing)
                )
                .unwrap()
                .findings
                .len(),
            1
        );
        assert!(
            contract
                .complete(
                    "Turn off the lights.",
                    BackendResult { spans: vec![] },
                    Some(&contract.default_routed_config())
                )
                .unwrap()
                .findings
                .is_empty()
        );
        let defaults = contract.default_routed_config();
        assert!(defaults.model_entities.contains("ADDRESS"));
        assert!(!defaults.model_entities.contains("STREET_ADDRESS"));
        assert!(!defaults.model_entities.contains("EMAIL"));
        assert!(!defaults.model_entities.contains("PHONE"));
    }

    #[test]
    fn small_rejects_malformed_spans_atomically_even_if_the_type_would_be_filtered() {
        let contract = contract();
        let routing = contract
            .routed_config(&json!({"entities": ["PERSON"]}))
            .unwrap();
        let text = "👋 José at jose@example.com";
        let mut invalid = vec![
            span("unknown", 5, 10),
            span("email", 1, 4),
            span("person", 5, 999),
            span("person", 10, 5),
        ];
        let mut bad_score = span("person", 5, 10);
        bad_score.raw_score.as_mut().unwrap().value = f64::NAN;
        invalid.push(bad_score);
        let mut bad_scale = span("person", 5, 10);
        bad_scale.raw_score.as_mut().unwrap().scale_id = "other".into();
        invalid.push(bad_scale);
        let mut wrong_text = span("email", 14, text.len());
        wrong_text.matched_text = Some("not the source".into());
        invalid.push(wrong_text);
        for bad in invalid {
            let error = contract
                .complete(
                    text,
                    BackendResult {
                        spans: vec![span("person", 5, 10), bad],
                    },
                    Some(&routing),
                )
                .unwrap_err();
            assert_eq!(error.code, "model_inference_failed");
            assert_eq!(error.finding_index, Some(1));
        }
    }

    #[test]
    fn small_bounds_and_entity_configuration_fail_with_value_free_paths() {
        let contract = contract();
        for config in [
            json!(null),
            json!({"entities": []}),
            json!({"entities": ["UNKNOWN"]}),
            json!({"entities": ["PERSON", "PERSON"]}),
            json!({"format": "yaml"}),
        ] {
            assert_eq!(
                contract.routed_config(&config).unwrap_err().code,
                "invalid_configuration"
            );
        }
        assert_eq!(
            contract.check_input(&"é".repeat(32769)).unwrap_err().code,
            "model_resource_exhausted"
        );
        assert_eq!(
            contract
                .complete(
                    "John",
                    BackendResult {
                        spans: vec![span("person", 0, 4); 1025]
                    },
                    None
                )
                .unwrap_err()
                .code,
            "model_resource_exhausted"
        );
        for labels in [
            json!({}),
            json!({"person": "PERSON", "Person": "OTHER"}),
            json!({"person": "PERSON", "name": "PERSON"}),
            json!({"[E]": "PERSON"}),
            json!({"person": "not canonical"}),
            json!({"person": 4}),
        ] {
            let error = ModelScanContract::gliner25_small(Some(&labels))
                .err()
                .unwrap();
            assert_eq!(error.code, "invalid_configuration");
            assert_eq!(error.path, "/labels");
        }
        let custom =
            ModelScanContract::gliner25_small(Some(&json!({"company": "COMPANY"}))).unwrap();
        let result = custom
            .complete(
                "Apple",
                BackendResult {
                    spans: vec![span("company", 0, 5)],
                },
                None,
            )
            .unwrap();
        assert_eq!(result.findings[0].entity_type, "COMPANY");
    }
}
