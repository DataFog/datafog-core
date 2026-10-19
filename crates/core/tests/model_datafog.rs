#![cfg(all(feature = "models", not(target_arch = "wasm32")))]
use datafog_core::models::{Cancellation, ModelSession, ModelSessionConfig};

/// Explicit real-model integration. Missing config or assets fail the test.
#[test]
#[ignore = "requires explicitly provisioned local model"]
fn pinned_model_execution() {
    let config = std::env::var("DATAFOG_MODEL_CONFIG")
        .expect("DATAFOG_MODEL_CONFIG must name a verified local session config");
    let config: ModelSessionConfig =
        serde_json::from_reader(std::fs::File::open(config).unwrap()).unwrap();
    let session = ModelSession::open(config).unwrap();
    let text = "👋 John Smith lives at 123 Main Street. Email john@example.com.";
    let result = session.scan(text, &Cancellation::default()).unwrap();
    assert!(!result.findings.is_empty());
    assert_eq!(result.contract_version, 1);
    assert_eq!(result.evidence.len(), result.findings.len());
    for finding in &result.findings {
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
    assert!(
        session
            .scan("", &Cancellation::default())
            .unwrap()
            .findings
            .is_empty()
    );
    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(
        session.scan("", &Cancellation::default()).unwrap_err().code,
        "model_session_closed"
    );
}
