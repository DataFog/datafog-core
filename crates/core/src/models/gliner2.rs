//! Pinned official GLiNER2 span runtime, executed in an explicitly installed Python environment.
use super::*;
use std::process::Command;

pub const MODEL: &str = "fastino/gliner2-privacy-filter-PII-multi";
const ADAPTER: &str = "gliner2-worker-v1";
const SCALE: &str = "normalized_span_sigmoid";
const LABELS: &str = "person full_name first_name middle_name last_name date_of_birth email phone_number address street_address city state_or_region postal_code country government_id national_id_number passport_number drivers_license_number license_number tax_id tax_number bank_account account_number routing_number iban payment_card card_number card_expiry card_cvv username ip_address account_id sensitive_account_id password secret api_key access_token recovery_code sensitive_date document_date expiration_date transaction_date";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gliner2Label {
    pub label: String,
    pub threshold: f64,
    #[serde(default)]
    pub description: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gliner2Config {
    /// Absolute interpreter path in the separately provisioned worker environment.
    pub python: PathBuf,
    pub labels: Vec<Gliner2Label>,
    pub intra_threads: u32,
    /// Official runtime word-window parameters, not character windows.
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

pub(super) fn parse_config(value: &serde_json::Value) -> Result<Gliner2Config, ModelError> {
    let object = crate::require_object(value, "/gliner2", "worker settings must be an object")?;
    let names = [
        "python",
        "labels",
        "intra_threads",
        "chunk_size",
        "chunk_overlap",
    ];
    crate::reject_unknown_fields(object, &names, "/gliner2")?;
    for name in names {
        if !object.contains_key(name) {
            return Err(ModelError::config(
                "missing_field",
                &format!("/gliner2/{name}"),
            ));
        }
    }
    let config: Gliner2Config = serde_json::from_value(value.clone())
        .map_err(|_| ModelError::config("invalid_type", "/gliner2"))?;
    validate_config(&config)?;
    Ok(config)
}
fn validate_config(config: &Gliner2Config) -> Result<(), ModelError> {
    if !config.python.is_absolute() {
        return Err(ModelError::config("invalid_value", "/gliner2/python"));
    }
    if config.intra_threads == 0
        || config.chunk_size == 0
        || config.chunk_overlap >= config.chunk_size
    {
        return Err(ModelError::config("invalid_value", "/gliner2"));
    }
    if config.labels.is_empty() {
        return Err(ModelError::config("empty_value", "/gliner2/labels"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (index, label) in config.labels.iter().enumerate() {
        let path = format!("/gliner2/labels/{index}");
        if !LABELS.split_whitespace().any(|name| name == label.label) || !seen.insert(&label.label)
        {
            return Err(ModelError::config(
                "invalid_value",
                &format!("{path}/label"),
            ));
        }
        if !label.threshold.is_finite() || !(0.0..=1.0).contains(&label.threshold) {
            return Err(ModelError::config(
                "invalid_value",
                &format!("{path}/threshold"),
            ));
        }
        if label
            .description
            .as_ref()
            .is_some_and(|d| d.trim().is_empty())
        {
            return Err(ModelError::config(
                "empty_value",
                &format!("{path}/description"),
            ));
        }
    }
    Ok(())
}
fn mapping(label: &str) -> String {
    match label {
        "phone_number" => "PHONE".into(),
        _ => label.to_ascii_uppercase(),
    }
}

fn runtime() -> BTreeMap<String, String> {
    let (transformers, torch) = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        ("4.57.6", "2.2.2")
    } else {
        ("5.8.0", "2.12.0")
    };
    [
        ("gliner2", "2.1.0"),
        ("transformers", transformers),
        ("torch", torch),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}

pub(super) fn open(
    config: ModelSessionConfig,
    deadline: Instant,
) -> Result<ModelSession, ModelError> {
    let options = config
        .gliner2
        .as_ref()
        .ok_or_else(|| ModelError::config("missing_field", "/gliner2"))?;
    validate_config(options)?;
    host_platform()
        .ok_or_else(|| ModelError::new("model_unavailable", "unsupported_platform", "/bundle"))?;
    let invalid = || ModelError::new("model_load_failed", "integrity_mismatch", "/bundle");
    let bundle = config
        .bundle
        .canonicalize()
        .map_err(|_| ModelError::new("model_load_failed", "missing_asset", "/bundle"))?;
    let manifest_path = bundle.join("bundle-manifest.json");
    let mut bytes = Vec::new();
    File::open(manifest_path)
        .map_err(|_| invalid())?
        .take(config.limits.max_response_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > config.limits.max_response_bytes || digest(&bytes) != config.manifest_sha256 {
        return Err(invalid());
    }
    let manifest: BundleManifest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let lock: serde_json::Value =
        serde_json::from_str(include_str!("gliner2-lock.json")).map_err(|_| invalid())?;
    let expected: BTreeMap<String, AssetIdentity> =
        serde_json::from_value(lock["files"].clone()).map_err(|_| invalid())?;
    if manifest.platform != "portable-cpu" || manifest.files.len() != expected.len() {
        return Err(invalid());
    }
    for (name, expected) in expected {
        let identity = manifest.files.get(&name).ok_or_else(invalid)?;
        if identity.sha256 != expected.sha256 || identity.bytes != expected.bytes {
            return Err(invalid());
        }
        let path = bundle.join(name).canonicalize().map_err(|_| invalid())?;
        if !path.starts_with(&bundle) || !path.is_file() {
            return Err(invalid());
        }
        let (hash, size) = file_digest(&path, deadline)?;
        if hash != expected.sha256 || size != expected.bytes {
            return Err(invalid());
        }
    }
    let mut command = Command::new(&options.python);
    command
        .args(["-I", "-u", "-m", "datafog_core._gliner2_worker"])
        .arg(&bundle)
        .arg(serde_json::to_string(options).map_err(|_| invalid())?)
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("TOKENIZERS_PARALLELISM", "false")
        .env_remove("USE_FLASHDEBERTA");
    let worker = process::Worker::spawn_command(&mut command, &config.limits)?;
    let cleanup_failed = Arc::clone(&worker.cleanup_failed);
    let session = ModelSession {
        descriptor: ModelSessionDescriptor {
            backend: "gliner2_worker".into(),
            model: MODEL.into(),
            adapter_version: ADAPTER.into(),
            labels: options
                .labels
                .iter()
                .map(|l| (l.label.clone(), mapping(&l.label)))
                .collect(),
            limits: config.limits,
            score_scale: SCALE.into(),
            thresholds: options
                .labels
                .iter()
                .map(|l| (l.label.clone(), l.threshold))
                .collect(),
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
        if !session
            .native(&mut inner, "", deadline, &Cancellation::default())?
            .spans
            .is_empty()
        {
            return Err(ModelError::invalid_output());
        }
    }
    session.state.store(1, Ordering::Release);
    Ok(session)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerOutput {
    findings: Vec<WorkerSpan>,
    runtime: BTreeMap<String, String>,
    worker_version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerSpan {
    label: String,
    start: usize,
    end: usize,
    text: String,
    raw_score: f64,
}

pub(super) fn decode(bytes: &[u8], max_findings: usize) -> Result<BackendResult, ModelError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ModelError::invalid_output())?;
    if let Some(code) = value.get("error").and_then(|v| v.as_str()) {
        return Err(match code {
            "resource_exhausted" => {
                ModelError::new("model_resource_exhausted", "resource_exhausted", "/backend")
            }
            "load_failed" => {
                ModelError::new("model_load_failed", "runtime_load_failed", "/backend")
            }
            "runtime_incompatible" => ModelError::new(
                "model_unavailable",
                "runtime_incompatible",
                "/gliner2/python",
            ),
            _ => ModelError::invalid_output(),
        });
    }
    let output: WorkerOutput =
        serde_json::from_value(value).map_err(|_| ModelError::invalid_output())?;
    let expected = runtime();
    if output.runtime != expected
        || output.worker_version != "0.4.5b3"
        || output.findings.len() > max_findings
    {
        return Err(ModelError::invalid_output());
    }
    Ok(BackendResult {
        spans: output
            .findings
            .into_iter()
            .map(|s| NativeSpan {
                native_label: s.label,
                start: s.start,
                end: s.end,
                coordinate_unit: CoordinateUnit::UnicodeCodepoint,
                raw_score: Some(RawScore {
                    value: s.raw_score,
                    scale_id: SCALE.into(),
                }),
                calibrated_confidence: None,
                matched_text: Some(s.text),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_labels_and_thresholds_are_validated_without_loading() {
        let valid = serde_json::json!({"python":"/worker/python", "labels":[{"label":"street_address", "threshold":0.5}],
            "intra_threads":1,"chunk_size":384,"chunk_overlap":64});
        let mut parsed: Gliner2Config = serde_json::from_value(valid).unwrap();
        // Absolute interpreter spelling is platform-specific.
        parsed.python = std::env::current_exe().unwrap();
        assert!(validate_config(&parsed).is_ok());
        for threshold in [-0.1, 1.4, f64::NAN, f64::INFINITY] {
            let mut bad = parsed.clone();
            bad.labels[0].threshold = threshold;
            assert_eq!(
                validate_config(&bad).unwrap_err().path,
                "/gliner2/labels/0/threshold"
            );
        }
        let mut bad = parsed.clone();
        bad.labels.push(bad.labels[0].clone());
        assert!(validate_config(&bad).is_err());
        let mut bad = parsed.clone();
        bad.labels[0].label = "not_a_checkpoint_label".into();
        assert!(validate_config(&bad).is_err());
        let mut bad = parsed;
        bad.chunk_overlap = bad.chunk_size;
        assert!(validate_config(&bad).is_err());
    }
    #[test]
    fn uncalibrated_scores_are_validated_before_thresholding_and_rebased() {
        let mut descriptor = super::super::tests::descriptor();
        descriptor.backend = "gliner2_worker".into();
        descriptor.adapter_version = ADAPTER.into();
        descriptor.labels = [("street_address".into(), "STREET_ADDRESS".into())].into();
        descriptor.thresholds = [("street_address".into(), 0.5)].into();
        descriptor.score_scale = SCALE.into();
        let response = |score: f64| {
            serde_json::json!({"worker_version":"0.4.5b3","runtime":runtime(),
            "findings":[{"label":"street_address","start":2,"end":17,"text":"123 Main Street","raw_score":score}]}).to_string()
        };
        let valid = decode(response(0.9).as_bytes(), 100).unwrap();
        let result = convert("👋 123 Main Street", valid, &descriptor).unwrap();
        assert_eq!(result[0].0.confidence, None);
        assert_eq!(result[0].0.byte_range.start, 5);
        assert_eq!(result[0].1.raw_score.as_ref().unwrap().scale_id, SCALE);
        for score in [-0.1, 1.4] {
            assert!(
                convert(
                    "👋 123 Main Street",
                    decode(response(score).as_bytes(), 100).unwrap(),
                    &descriptor
                )
                .is_err()
            );
        }
        assert!(
            convert(
                "👋 123 Main Street",
                decode(response(0.1).as_bytes(), 100).unwrap(),
                &descriptor
            )
            .unwrap()
            .is_empty()
        );
        let corrupt =
            serde_json::json!({"worker_version":"0.4.5b3","runtime":{},"findings":[]}).to_string();
        assert!(decode(corrupt.as_bytes(), 100).is_err());
    }
}
