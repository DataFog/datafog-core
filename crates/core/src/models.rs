//! Explicit offline model sessions. The initial adapter runs the pinned DataFog
//! native executable; no model assets or inference libraries are loaded by default.
mod gliner2;
mod process;
pub use gliner2::{Gliner2Config, Gliner2Label};
#[cfg(test)]
mod tests;

pub use crate::model_scan::{
    BackendResult, CoordinateUnit, FindingEvidence, ModelError, ModelScanResult, NativeSpan,
    RawScore, ResourceLimits, RoutedScanConfig, SourceEvidence,
};
use crate::model_scan::{
    ModelScanContract as ModelSessionDescriptor, bounded_json, convert, rule_entries,
};
#[cfg(test)]
use crate::{ScanConfig, TextRange};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
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
    /// Omission retains legacy selection from the presence of `gliner2` settings.
    #[serde(default)]
    pub engine: Option<ModelEngine>,
    #[serde(default)]
    pub gliner2: Option<Gliner2Config>,
    pub bundle: PathBuf,
    pub manifest_sha256: String,
    pub limits: ResourceLimits,
}

/// Native engines compiled into the optional model-session API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelEngine {
    Datafog,
    Gliner2,
}

impl ModelSessionConfig {
    /// Resolve legacy configuration and reject contradictory adapter settings.
    pub fn selected_engine(&self) -> Result<ModelEngine, ModelError> {
        match (self.engine, self.gliner2.is_some()) {
            (Some(ModelEngine::Datafog), true) => {
                Err(ModelError::config("invalid_value", "/gliner2"))
            }
            (Some(ModelEngine::Gliner2), false) => {
                Err(ModelError::config("missing_field", "/gliner2"))
            }
            (None | Some(ModelEngine::Gliner2), true) => Ok(ModelEngine::Gliner2),
            (None | Some(ModelEngine::Datafog), false) => Ok(ModelEngine::Datafog),
        }
    }
}

/// Parse binding configuration once in Core, retaining precise value-free paths.
pub fn parse_config(value: &serde_json::Value) -> Result<ModelSessionConfig, ModelError> {
    let object = crate::require_object(value, "", "model configuration must be an object")?;
    crate::reject_unknown_fields(
        object,
        &["bundle", "manifest_sha256", "limits", "engine", "gliner2"],
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
    let engine = object
        .get("engine")
        .map(|value| match value.as_str() {
            Some("datafog") => Ok(ModelEngine::Datafog),
            Some("gliner2") => Ok(ModelEngine::Gliner2),
            Some(_) => Err(ModelError::config("invalid_value", "/engine")),
            None => Err(ModelError::config("invalid_type", "/engine")),
        })
        .transpose()?;
    let config = ModelSessionConfig {
        engine,
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
    };
    config.selected_engine()?;
    Ok(config)
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
        match config.selected_engine()? {
            ModelEngine::Gliner2 => return gliner2::open(config, deadline),
            ModelEngine::Datafog => {}
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

    /// Validate entity routing against this session's model labels and rule detectors.
    pub fn routed_config(&self, value: &serde_json::Value) -> Result<RoutedScanConfig, ModelError> {
        self.descriptor.routed_config(value)
    }

    /// Route all enabled rule detectors and model-only labels.
    pub fn default_routed_config(&self) -> RoutedScanConfig {
        self.descriptor.default_routed_config()
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
            let result = self.descriptor.compose(rule_entries(text, config))?;
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
        let result = self.descriptor.compose(entries)?;
        bounded_json(&result, self.descriptor.limits.max_response_bytes, "/data")?;
        self.active_check(inner, text, deadline, cancellation)?;
        Ok(result)
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
