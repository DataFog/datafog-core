//! Explicit offline model sessions. The initial adapter runs the pinned DataFog
//! native executable; no model assets or inference libraries are loaded by default.
mod gliner2;
mod process;
pub use gliner2::{Gliner2Config, Gliner2Label};
#[cfg(test)]
mod tests;

use crate::{Finding, PrivacyError, PrivacyErrorReason, ScanConfig, TextIndex, TextRange};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

pub const MODEL_REVISION: &str = "02cc6ca86a11dcb2b328861687770a22a7ea1b76";
pub const RUNTIME_REVISION: &str = "7cadd41e21f59b2a149e31378d073cf6ca057531";
const ADAPTER_VERSION: &str = "datafog-native-v1";
const LABELS: [(&str, &str); 7] = [
    ("first_name", "FIRST_NAME"),
    ("last_name", "LAST_NAME"),
    ("email", "EMAIL"),
    ("phone_number", "PHONE"),
    ("street_address", "STREET_ADDRESS"),
    ("customer_id", "CUSTOMER_ID"),
    ("account_number", "ACCOUNT_NUMBER"),
];

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
    fn new(code: &str, reason: &str, path: &str) -> Self {
        Self {
            code: code.into(),
            reason: reason.into(),
            path: path.into(),
            finding_index: None,
        }
    }
    fn config(reason: &str, path: &str) -> Self {
        Self::new("invalid_configuration", reason, path)
    }
    fn invalid_output() -> Self {
        Self::new("model_inference_failed", "invalid_output", "/backend")
    }
    fn timeout() -> Self {
        Self::new("model_timeout", "deadline_exceeded", "/data")
    }
    fn cleanup() -> Self {
        Self::new("model_cleanup_failed", "process_cleanup_failed", "/backend")
    }
    fn closed() -> Self {
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
impl ResourceLimits {
    fn validate(&self) -> Result<(), ModelError> {
        if self.max_input_bytes == 0
            || self.max_response_bytes == 0
            || self.max_findings == 0
            || self.max_response_bytes == usize::MAX
            || self.timeout_ms == 0
            || self.cleanup_timeout_ms == 0
            || Instant::now()
                .checked_add(Duration::from_millis(self.timeout_ms))
                .is_none()
            || Instant::now()
                .checked_add(Duration::from_millis(self.cleanup_timeout_ms))
                .is_none()
        {
            return Err(ModelError::config("invalid_value", "/limits"));
        }
        Ok(())
    }
}

/// Local model assets and immutable expected manifest identity. No download occurs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSessionConfig {
    #[serde(default)]
    pub gliner2: Option<Gliner2Config>,
    pub bundle: PathBuf,
    pub manifest_sha256: String,
    pub limits: ResourceLimits,
}

/// Parse binding configuration once in Core, retaining precise value-free paths.
pub fn parse_config(value: &serde_json::Value) -> Result<ModelSessionConfig, ModelError> {
    let object = crate::require_object(value, "", "model configuration must be an object")?;
    crate::reject_unknown_fields(
        object,
        &["bundle", "manifest_sha256", "limits", "gliner2"],
        "",
    )?;
    let required = |key: &str| {
        object
            .get(key)
            .ok_or_else(|| ModelError::config("missing_field", &format!("/{key}")))
    };
    let bundle = crate::require_string(required("bundle")?, "/bundle", "bundle must be a string")?;
    if bundle.trim().is_empty() {
        return Err(ModelError::config("empty_value", "/bundle"));
    }
    let manifest_sha256 = crate::require_string(
        required("manifest_sha256")?,
        "/manifest_sha256",
        "manifest identity must be a string",
    )?;
    let limits = crate::require_object(required("limits")?, "/limits", "limits must be an object")?;
    let names = [
        "max_input_bytes",
        "max_response_bytes",
        "max_findings",
        "timeout_ms",
        "cleanup_timeout_ms",
    ];
    crate::reject_unknown_fields(limits, &names, "/limits")?;
    let number = |name: &str| -> Result<u64, ModelError> {
        let path = format!("/limits/{name}");
        limits
            .get(name)
            .ok_or_else(|| ModelError::config("missing_field", &path))?
            .as_u64()
            .ok_or_else(|| ModelError::config("invalid_type", &path))
    };
    let size = |name: &str| {
        usize::try_from(number(name)?)
            .map_err(|_| ModelError::config("invalid_value", &format!("/limits/{name}")))
    };
    Ok(ModelSessionConfig {
        gliner2: object
            .get("gliner2")
            .map(gliner2::parse_config)
            .transpose()?,
        bundle: bundle.into(),
        manifest_sha256,
        limits: ResourceLimits {
            max_input_bytes: size("max_input_bytes")?,
            max_response_bytes: size("max_response_bytes")?,
            max_findings: size("max_findings")?,
            timeout_ms: number("timeout_ms")?,
            cleanup_timeout_ms: number("cleanup_timeout_ms")?,
        },
    })
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
/// requested entity type they support; the model runs only for requested
/// types that it alone provides. Validated against one session's labels.
#[derive(Clone, Debug)]
pub struct RoutedScanConfig {
    rules: Option<ScanConfig>,
    model_entities: BTreeSet<String>,
}

impl RoutedScanConfig {
    /// Whether a scan with this configuration runs model inference.
    pub fn needs_model(&self) -> bool {
        !self.model_entities.is_empty()
    }
}

fn rule_entry(finding: Finding) -> (Finding, SourceEvidence) {
    let source = SourceEvidence {
        detector_name: finding.detector_name.clone(),
        detector_version: finding.detector_version.clone(),
        native_label: None,
        raw_score: None,
        calibrated_confidence: finding.confidence.map(f64::from),
    };
    (finding, source)
}

fn rule_entries(text: &str, config: &RoutedScanConfig) -> Vec<(Finding, SourceEvidence)> {
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
struct ModelSessionDescriptor {
    backend: String,
    model: String,
    adapter_version: String,
    labels: BTreeMap<String, String>,
    limits: ResourceLimits,
    score_scale: String,
    thresholds: BTreeMap<String, f64>,
}

/// A cancellation handle can be shared with another thread.
#[derive(Default)]
pub struct Cancellation {
    cancelled: AtomicBool,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// A single admitted request. Dropping it releases admission, including before execution.
/// The deadline starts at admission, before a binding schedules blocking work.
pub struct ModelRequest {
    admitted: Arc<AtomicBool>,
    deadline: Instant,
}
impl Drop for ModelRequest {
    fn drop(&mut self) {
        self.admitted.store(false, Ordering::Release);
    }
}

struct SessionInner {
    worker: Option<process::Worker>,
}
pub struct ModelSession {
    descriptor: ModelSessionDescriptor,
    inner: Mutex<SessionInner>,
    closing: AtomicBool,
    state: AtomicU8,
    cleanup_failed: Arc<AtomicBool>,
    admitted: Arc<AtomicBool>,
}

impl ModelSession {
    pub fn open(config: ModelSessionConfig) -> Result<Self, ModelError> {
        Self::open_started(config, Instant::now())
    }

    /// Include binding scheduling and decoding time in the loading deadline.
    pub fn open_started(config: ModelSessionConfig, started: Instant) -> Result<Self, ModelError> {
        config.limits.validate()?;
        let deadline = started
            .checked_add(Duration::from_millis(config.limits.timeout_ms))
            .ok_or_else(ModelError::timeout)?;
        if config.gliner2.is_some() {
            return gliner2::open(config, deadline);
        }
        let (bundle, _, _) = validate_bundle(&config, deadline)?;
        let executable = bundle.join(if cfg!(windows) {
            "datafog-pii.exe"
        } else {
            "datafog-pii"
        });
        let worker = process::Worker::spawn(&executable, &bundle, &config.limits)?;
        let cleanup_failed = Arc::clone(&worker.cleanup_failed);
        let session = Self {
            descriptor: ModelSessionDescriptor {
                backend: "datafog_native".into(),
                model: "DataFog/pii-en-65m".into(),
                adapter_version: ADAPTER_VERSION.into(),
                labels: LABELS
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect(),
                limits: config.limits,
                score_scale: "normalized_token_softmax_min".into(),
                thresholds: BTreeMap::new(),
            },
            inner: Mutex::new(SessionInner {
                worker: Some(worker),
            }),
            closing: AtomicBool::new(false),
            state: AtomicU8::new(0),
            cleanup_failed,
            admitted: Arc::new(AtomicBool::new(false)),
        };
        {
            let mut inner = session.lock()?;
            let result = session.native(&mut inner, "", deadline, &Cancellation::default())?;
            if !result.spans.is_empty() {
                return Err(ModelError::invalid_output());
            }
        }
        session.state.store(1, Ordering::Release);
        Ok(session)
    }

    fn lock(&self) -> Result<MutexGuard<'_, SessionInner>, ModelError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(ModelError::closed());
        }
        match self.inner.try_lock() {
            Ok(inner) => {
                if self.state.load(Ordering::Acquire) == 2 {
                    Err(ModelError::new(
                        "model_session_failed",
                        "failed",
                        "/backend",
                    ))
                } else {
                    Ok(inner)
                }
            }
            Err(TryLockError::WouldBlock) => Err(ModelError::new("model_busy", "busy", "/backend")),
            Err(TryLockError::Poisoned(_)) => Err(ModelError::new(
                "model_session_failed",
                "failed",
                "/backend",
            )),
        }
    }

    fn check(
        &self,
        text: &str,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<(), ModelError> {
        if self.cleanup_failed.load(Ordering::Acquire) {
            return Err(ModelError::cleanup());
        }
        if self.state.load(Ordering::Acquire) == 3 {
            return Err(ModelError::closed());
        }
        if text.len() > self.descriptor.limits.max_input_bytes {
            return Err(ModelError::config("limit_exceeded", "/data"));
        }
        if self.closing.load(Ordering::Acquire) || cancellation.is_cancelled() {
            return Err(ModelError::new("model_cancelled", "cancelled", "/data"));
        }
        if Instant::now() >= deadline {
            return Err(ModelError::timeout());
        }
        Ok(())
    }

    fn native(
        &self,
        inner: &mut SessionInner,
        text: &str,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<BackendResult, ModelError> {
        self.active_check(inner, text, deadline, cancellation)?;
        let result = inner
            .worker
            .as_mut()
            .ok_or_else(ModelError::closed)?
            .exchange(text, deadline, || {
                self.closing.load(Ordering::Acquire) || cancellation.is_cancelled()
            })
            .and_then(|bytes| {
                if self.descriptor.backend == "gliner2_worker" {
                    gliner2::decode(&bytes, self.descriptor.limits.max_findings)
                } else {
                    decode_datafog(&bytes, self.descriptor.limits.max_findings)
                }
            });
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                self.retire(inner)?;
                Err(error)
            }
        }
    }

    fn retire(&self, inner: &mut SessionInner) -> Result<(), ModelError> {
        self.state.store(2, Ordering::Release);
        if let Some(worker) = inner.worker.as_mut() {
            worker.stop()?;
        }
        inner.worker.take();
        Ok(())
    }

    fn active_check(
        &self,
        inner: &mut SessionInner,
        text: &str,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<(), ModelError> {
        if let Err(error) = self.check(text, deadline, cancellation) {
            self.retire(inner)?;
            return Err(error);
        }
        Ok(())
    }

    /// Reserve the session immediately; concurrent callers never enter a binding queue.
    pub fn prepare_request(&self) -> Result<ModelRequest, ModelError> {
        let deadline = deadline(self.descriptor.limits.timeout_ms)?;
        self.check("", deadline, &Cancellation::default())?;
        if self.state.load(Ordering::Acquire) == 2 {
            return Err(ModelError::new(
                "model_session_failed",
                "failed",
                "/backend",
            ));
        }
        self.admitted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ModelError::new("model_busy", "busy", "/backend"))?;
        Ok(ModelRequest {
            admitted: Arc::clone(&self.admitted),
            deadline,
        })
    }

    fn request_deadline(&self, request: &ModelRequest) -> Result<Instant, ModelError> {
        if !Arc::ptr_eq(&self.admitted, &request.admitted) {
            return Err(ModelError::config("invalid_value", "/request"));
        }
        Ok(request.deadline)
    }

    /// Scan with default routing: every enabled rule detector, plus the model
    /// for the labels rules do not produce. Equivalent to an empty routed
    /// configuration, as an omitted rule scan configuration equals `{}`.
    pub fn scan(
        &self,
        text: &str,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        self.scan_routed(text, &self.default_routed_config(), cancellation)
    }

    /// Raw model output: every model label, with no rule detectors.
    pub fn scan_model(
        &self,
        text: &str,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        let request = self.prepare_request()?;
        self.scan_model_request(&request, text, cancellation)
    }

    pub fn scan_model_request(
        &self,
        request: &ModelRequest,
        text: &str,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        let deadline = self.request_deadline(request)?;
        self.check(text, deadline, cancellation)?;
        let mut inner = self.lock()?;
        self.scan_locked(&mut inner, text, None, deadline, cancellation)
    }

    /// Validate a routed scan configuration. It accepts the rule scan fields
    /// (`entities`, `locale`, `detect_uuid`, `format`); `entities` may also
    /// name this session's model labels. Without `entities`, every enabled
    /// rule detector runs and the model returns the labels rules do not cover.
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
        let model_labels: BTreeSet<&str> = self
            .descriptor
            .labels
            .values()
            .map(String::as_str)
            .collect();
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
                    if crate::capabilities::text_entity_supported(entity) {
                        rules.push(serde_json::Value::String(entity.to_owned()));
                    } else if model_labels.contains(entity) {
                        model.insert(entity.to_owned());
                    } else {
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
    /// and the model labels that rules do not produce.
    pub fn default_routed_config(&self) -> RoutedScanConfig {
        RoutedScanConfig {
            rules: Some(ScanConfig::new()),
            model_entities: self.model_only_labels(),
        }
    }

    fn model_only_labels(&self) -> BTreeSet<String> {
        self.descriptor
            .labels
            .values()
            .filter(|label| !crate::capabilities::text_entity_supported(label))
            .cloned()
            .collect()
    }

    /// Scan with entity routing. A configuration that needs no model labels
    /// runs only rule detectors, without reserving or waiting for the worker.
    pub fn scan_routed(
        &self,
        text: &str,
        config: &RoutedScanConfig,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        let request = if config.needs_model() {
            Some(self.prepare_request()?)
        } else {
            None
        };
        self.scan_routed_request(request.as_ref(), text, config, cancellation)
    }

    /// Bindings reserve the session before dispatch only when the routed
    /// configuration needs the model; otherwise `request` is `None`.
    pub fn scan_routed_request(
        &self,
        request: Option<&ModelRequest>,
        text: &str,
        config: &RoutedScanConfig,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        if !config.needs_model() {
            // Lifecycle, input limits and cancellation apply without the worker.
            let deadline = deadline(self.descriptor.limits.timeout_ms)?;
            self.check(text, deadline, cancellation)?;
            if self.state.load(Ordering::Acquire) == 2 {
                return Err(ModelError::new(
                    "model_session_failed",
                    "failed",
                    "/backend",
                ));
            }
            let result = self.compose(rule_entries(text, config))?;
            bounded_json(&result, self.descriptor.limits.max_response_bytes, "/data")?;
            return Ok(result);
        }
        let request = request.ok_or_else(|| ModelError::config("invalid_value", "/request"))?;
        let deadline = self.request_deadline(request)?;
        self.check(text, deadline, cancellation)?;
        let mut inner = self.lock()?;
        self.scan_locked(&mut inner, text, Some(config), deadline, cancellation)
    }

    fn scan_locked(
        &self,
        inner: &mut SessionInner,
        text: &str,
        routing: Option<&RoutedScanConfig>,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<ModelScanResult, ModelError> {
        let native = self.native(inner, text, deadline, cancellation)?;
        let converted = convert(text, native, &self.descriptor);
        let mut entries = match converted {
            Ok(entries) => entries,
            Err(error) => {
                self.retire(inner)?;
                return Err(error);
            }
        };
        if let Some(config) = routing {
            entries.retain(|(finding, _)| config.model_entities.contains(&finding.entity_type));
            entries.splice(0..0, rule_entries(text, config));
        }
        self.active_check(inner, text, deadline, cancellation)?;
        let result = self.compose(entries)?;
        bounded_json(&result, self.descriptor.limits.max_response_bytes, "/data")?;
        self.active_check(inner, text, deadline, cancellation)?;
        Ok(result)
    }

    fn compose(
        &self,
        entries: Vec<(Finding, SourceEvidence)>,
    ) -> Result<ModelScanResult, ModelError> {
        if entries.len() > self.descriptor.limits.max_findings {
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
            model: self.descriptor.model.clone(),
            findings,
            evidence,
        })
    }

    pub fn request_close(&self) {
        self.closing.store(true, Ordering::Release);
    }

    pub fn close(&self) -> Result<(), ModelError> {
        self.closing.store(true, Ordering::Release);
        let deadline = deadline(self.descriptor.limits.cleanup_timeout_ms)?;
        loop {
            match self.inner.try_lock() {
                Ok(mut inner) => {
                    if let Some(worker) = inner.worker.as_mut() {
                        if let Err(error) = worker.stop() {
                            self.state.store(2, Ordering::Release);
                            return Err(error);
                        }
                    }
                    inner.worker.take();
                    self.state.store(3, Ordering::Release);
                    return Ok(());
                }
                Err(TryLockError::Poisoned(_)) => {
                    self.state.store(2, Ordering::Release);
                    return Err(ModelError::cleanup());
                }
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        self.state.store(2, Ordering::Release);
                        return Err(ModelError::cleanup());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

fn bounded_json(value: &impl Serialize, limit: usize, path: &str) -> Result<(), ModelError> {
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

fn deadline(milliseconds: u64) -> Result<Instant, ModelError> {
    Instant::now()
        .checked_add(Duration::from_millis(milliseconds))
        .ok_or_else(|| ModelError::config("invalid_value", "/limits/timeout_ms"))
}

#[derive(Deserialize)]
struct DataFogSpan {
    label: String,
    start: usize,
    end: usize,
    raw_score: f64,
    confidence: f64,
}
fn decode_datafog(bytes: &[u8], max_findings: usize) -> Result<BackendResult, ModelError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ModelError::invalid_output())?;
    if value.get("error").is_some() {
        return Err(ModelError::new(
            "model_inference_failed",
            "runtime_error",
            "/backend",
        ));
    }
    let findings = value
        .get("findings")
        .and_then(|v| v.as_array())
        .ok_or_else(ModelError::invalid_output)?;
    if findings.len() > max_findings {
        return Err(ModelError::invalid_output());
    }
    let spans = findings
        .iter()
        .map(|value| {
            let finding: DataFogSpan =
                serde_json::from_value(value.clone()).map_err(|_| ModelError::invalid_output())?;
            Ok(NativeSpan {
                native_label: finding.label,
                start: finding.start,
                end: finding.end,
                coordinate_unit: CoordinateUnit::Utf8Byte,
                raw_score: Some(RawScore {
                    value: finding.raw_score,
                    scale_id: "normalized_token_softmax_min".into(),
                }),
                calibrated_confidence: Some(finding.confidence),
                matched_text: None,
            })
        })
        .collect::<Result<_, ModelError>>()?;
    Ok(BackendResult { spans })
}

fn convert(
    text: &str,
    result: BackendResult,
    descriptor: &ModelSessionDescriptor,
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
            let backend_name = if descriptor.backend == "gliner2_worker" {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleManifest {
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    model: Option<ModelIdentity>,
    platform: String,
    files: BTreeMap<String, AssetIdentity>,
}

/// A shared identity envelope; adapters still define their supported loading protocol.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelIdentity {
    backend: String,
    id: String,
    revision: String,
}

fn validate_model_identity<'a>(
    manifest: &'a BundleManifest,
    backend: &str,
) -> Result<Option<&'a ModelIdentity>, ModelError> {
    match (&manifest.model, manifest.schema_version) {
        (None, None) => Ok(None),
        (Some(model), Some(1))
            if model.backend == backend
                && !model.id.trim().is_empty()
                && model.id.trim() == model.id
                && !model.revision.trim().is_empty()
                && model.revision.trim() == model.revision =>
        {
            Ok(Some(model))
        }
        _ => Err(ModelError::new(
            "model_load_failed",
            "integrity_mismatch",
            "/bundle",
        )),
    }
}

fn validate_asset_name(name: &str) -> Result<(), ModelError> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || !Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(ModelError::new(
            "model_load_failed",
            "integrity_mismatch",
            "/bundle",
        ));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetIdentity {
    sha256: String,
    bytes: u64,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_digest(path: &Path, deadline: Instant) -> Result<(String, u64), ModelError> {
    let mut file = File::open(path)
        .map_err(|_| ModelError::new("model_load_failed", "missing_asset", "/bundle"))?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        if Instant::now() >= deadline {
            return Err(ModelError::timeout());
        }
        let read = file
            .read(&mut buffer)
            .map_err(|_| ModelError::new("model_load_failed", "asset_unavailable", "/bundle"))?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| ModelError::config("limit_exceeded", "/bundle"))?;
        hash.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hash.finalize()), size))
}

fn host_platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("linux", "aarch64") if cfg!(target_env = "gnu") => Some("linux-aarch64"),
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Some("linux-x86_64"),
        ("windows", "x86_64") if cfg!(target_env = "msvc") => Some("windows-x86_64"),
        _ => None,
    }
}

fn validate_bundle(
    config: &ModelSessionConfig,
    deadline: Instant,
) -> Result<(PathBuf, String, f64), ModelError> {
    let invalid = || ModelError::new("model_load_failed", "integrity_mismatch", "/bundle");
    if config.manifest_sha256.len() != 64
        || !config
            .manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ModelError::config("invalid_value", "/manifest_sha256"));
    }
    let root = config
        .bundle
        .canonicalize()
        .map_err(|_| ModelError::new("model_load_failed", "missing_asset", "/bundle"))?;
    let manifest = root.join("bundle-manifest.json");
    let mut bytes = Vec::new();
    File::open(&manifest)
        .map_err(|_| invalid())?
        .take(config.limits.max_response_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > config.limits.max_response_bytes || digest(&bytes) != config.manifest_sha256 {
        return Err(invalid());
    }
    let manifest: BundleManifest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if let Some(model) = validate_model_identity(&manifest, "datafog_native")? {
        if model.id != "DataFog/pii-en-65m" || model.revision != MODEL_REVISION {
            return Err(invalid());
        }
    }
    if Some(manifest.platform.as_str()) != host_platform() {
        return Err(ModelError::new(
            "model_unavailable",
            "unsupported_platform",
            "/bundle",
        ));
    }
    for (name, identity) in &manifest.files {
        validate_asset_name(name)?;
        let relative = Path::new(name);
        let path = root.join(relative);
        let canonical = path.canonicalize().map_err(|_| invalid())?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err(invalid());
        }
        let (sha256, size) = file_digest(&canonical, deadline)?;
        if sha256 != identity.sha256 || size != identity.bytes {
            return Err(invalid());
        }
    }
    for (name, expected) in [
        (
            "model.onnx",
            "04889c1ea2e9029d9b89cd5f35697224634bb06d9777abfa6ea575810b9a3eb2",
        ),
        (
            "tokenizer.json",
            "343989712a36cd8b253efeaf8baf6a08b9d2583f78e395e83832e8ee9f8d8ee1",
        ),
        (
            "config.json",
            "446e6d4beb141b894ee2bd1c51a0755bac9a214b3db09fbe86dfe9137f037f1b",
        ),
        (
            "calibration.json",
            "75a946908fdc07549d81c7b72a2c1774da39e0755b88f7083a2492c70a8f1f53",
        ),
    ] {
        if manifest
            .files
            .get(name)
            .is_none_or(|identity| identity.sha256 != expected)
        {
            return Err(invalid());
        }
    }
    let executable = if cfg!(windows) {
        "datafog-pii.exe"
    } else {
        "datafog-pii"
    };
    let library = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    for required in [
        executable,
        library,
        "runtime-provenance.json",
        "LICENSE",
        "NOTICE",
        "tokenizer_config.json",
    ] {
        if !manifest.files.contains_key(required) {
            return Err(invalid());
        }
    }
    let provenance: serde_json::Value = serde_json::from_reader(
        File::open(root.join("runtime-provenance.json")).map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())?;
    if provenance["model_version"] != "0.1.0"
        || provenance["runtime_version"] != "0.2.0"
        || provenance["hf_revision"] != MODEL_REVISION
        || provenance["platform"] != manifest.platform
        || provenance["telemetry_disabled"] != true
    {
        return Err(invalid());
    }
    let calibration: serde_json::Value =
        serde_json::from_reader(File::open(root.join("calibration.json")).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    let threshold = calibration
        .get("threshold")
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .ok_or_else(invalid)?;
    Ok((root, manifest.platform, threshold))
}
