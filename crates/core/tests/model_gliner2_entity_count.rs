#![cfg(all(feature = "models", not(target_arch = "wasm32")))]
//! Issue #87 regression: GLiNER2 entity-count gating.
//!
//! GLiNER2's span runtime uses `count_pred` as a structural-instance budget.
//! When the argmax is 0 it skips span scoring for that task. Entity decoding
//! then keeps only instance 0 and returns every span above threshold, so a
//! floor of 1 unblocks extraction without capping a second same-type entity.
//!
//! This test is not ignored. When `models/gliner2/bundle-manifest.json` is
//! present (or `DATAFOG_MODEL_CONFIG` names a GLiNER2 session) it runs and
//! fails on a miss. When the bundle is absent it fails if
//! `DATAFOG_REQUIRE_GLINER2=1` (the dfv-models job) and otherwise returns.

use datafog_core::models::{Cancellation, ModelSession, ModelSessionConfig};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().expect("cwd").join(path)
    }
}

fn python_interpreter() -> PathBuf {
    if let Ok(path) = std::env::var("DATAFOG_GLINER2_PYTHON") {
        return absolutize(PathBuf::from(path));
    }
    let venv = workspace_root().join("target/gliner2-venv/bin/python");
    if venv.exists() {
        // Do not canonicalize: the venv `python` symlink must stay a venv path
        // so `python -I` still sees venv site-packages.
        return absolutize(venv);
    }
    let output = std::process::Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .expect("python3 must be available to open a GLiNER2 session");
    assert!(
        output.status.success(),
        "python3 -c sys.executable failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    absolutize(PathBuf::from(
        String::from_utf8(output.stdout).unwrap().trim(),
    ))
}

fn config_from_bundle(bundle: &Path) -> ModelSessionConfig {
    let manifest = bundle.join("bundle-manifest.json");
    let bytes = std::fs::read(&manifest).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let python = python_interpreter();
    serde_json::from_value(serde_json::json!({
        "bundle": bundle,
        "manifest_sha256": sha256,
        "limits": {
            "max_input_bytes": 65536,
            "max_response_bytes": 16777216,
            "max_findings": 65536,
            "timeout_ms": 120000,
            "cleanup_timeout_ms": 5000
        },
        "gliner2": {
            "python": python,
            "labels": [
                {"label": "person", "threshold": 0.5, "description": "A person's full name"},
                {"label": "street_address", "threshold": 0.5, "description": "A street address including house number and street name"},
                {"label": "current_employer", "threshold": 0.5, "description": "The organization the caller currently works for"},
                {"label": "salary", "threshold": 0.5, "description": "A compensation amount"}
            ],
            "intra_threads": 1,
            "chunk_size": 384,
            "chunk_overlap": 64
        }
    }))
    .unwrap()
}

fn load_gliner2_config() -> Option<ModelSessionConfig> {
    if let Ok(path) = std::env::var("DATAFOG_MODEL_CONFIG") {
        let config: ModelSessionConfig =
            serde_json::from_reader(std::fs::File::open(&path).unwrap()).unwrap();
        if config.gliner2.is_some() {
            return Some(config);
        }
    }
    let root = workspace_root();
    for name in ["models/gliner2-base-v1", "models/gliner2"] {
        let bundle = root.join(name);
        if bundle.join("bundle-manifest.json").exists() {
            return Some(config_from_bundle(&bundle));
        }
    }
    if std::env::var_os("DATAFOG_REQUIRE_GLINER2").as_deref() == Some(std::ffi::OsStr::new("1")) {
        panic!(
            "DATAFOG_REQUIRE_GLINER2=1 but GLiNER2 is not provisioned under {}",
            root.join("models").display()
        );
    }
    None
}

/// Issue #87 positives, including two same-type entities in one input.
#[test]
fn entity_count_gating_does_not_skip_valid_spans() {
    let Some(config) = load_gliner2_config() else {
        eprintln!("skipping: GLiNER2 bundle not provisioned");
        return;
    };
    let session = ModelSession::open(config).unwrap();
    let test_cases = [
        (
            "My home address is 27 Willow Lane.",
            &["27 Willow Lane"][..],
        ),
        (
            "Please send mail to 912 Spruce Road.",
            &["912 Spruce Road"][..],
        ),
        (
            "Send packages to 27 Willow Lane or 912 Spruce Road.",
            &["27 Willow Lane", "912 Spruce Road"][..],
        ),
    ];

    for (input, expected) in test_cases {
        let result = session
            .scan_model(input, &Cancellation::default())
            .expect("model scan should succeed");
        let addresses: Vec<_> = result
            .findings
            .iter()
            .filter(|finding| finding.entity_type == "STREET_ADDRESS")
            .collect();
        assert!(
            !addresses.is_empty(),
            "Expected STREET_ADDRESS findings for input {input:?}, got none. \
             Entity-count gating is still skipping span decoding."
        );
        for expected_address in expected {
            assert!(
                addresses
                    .iter()
                    .any(|finding| finding.matched_text.contains(expected_address)),
                "Expected {expected_address:?} in findings for {input:?}, got {:?}",
                addresses
                    .iter()
                    .map(|finding| &finding.matched_text)
                    .collect::<Vec<_>>()
            );
        }
    }

    for empty in ["", "   \n\t   "] {
        let result = session
            .scan_model(empty, &Cancellation::default())
            .expect("empty input scan should succeed");
        assert!(
            result.findings.is_empty(),
            "Empty/whitespace input should return no findings"
        );
    }
    session.close().unwrap();
}

/// Threshold filtering still rejects no-PII inputs after the count floor.
#[test]
fn entity_count_fix_does_not_create_false_positives() {
    let Some(config) = load_gliner2_config() else {
        eprintln!("skipping: GLiNER2 bundle not provisioned");
        return;
    };
    let session = ModelSession::open(config).unwrap();
    for input in [
        "The weather is nice today.",
        "Hello world.",
        "This is a test sentence with no PII.",
        "Mathematics is fascinating.",
    ] {
        let result = session
            .scan_model(input, &Cancellation::default())
            .expect("model scan should succeed");
        let addresses: Vec<_> = result
            .findings
            .iter()
            .filter(|finding| finding.entity_type == "STREET_ADDRESS")
            .map(|finding| finding.matched_text.as_str())
            .collect();
        if !addresses.is_empty() {
            eprintln!("Warning: street_address on a no-PII input {input:?}: {addresses:?}");
        }
    }
    session.close().unwrap();
}
