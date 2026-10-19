#![cfg(all(feature = "models", not(target_arch = "wasm32")))]
use datafog_core::models::{Cancellation, ModelSession, ModelSessionConfig, ScanMode};

/// This is an explicit qualification entry point, never a synthetic-model pass.
/// Run --ignored with DATAFOG_MODEL_CONFIG; missing assets/config fail the test.
#[test]
#[ignore = "explicit installed pinned-model qualification"]
fn pinned_datafog_execution() {
    let config = std::env::var("DATAFOG_MODEL_CONFIG")
        .expect("DATAFOG_MODEL_CONFIG must name a verified local session config");
    let config: ModelSessionConfig =
        serde_json::from_reader(std::fs::File::open(config).unwrap()).unwrap();
    let session = ModelSession::open(config).unwrap();
    let text = "👋 John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.";
    let result = session
        .scan(text, ScanMode::Combined, false, &Cancellation::default())
        .unwrap();
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.entity_type == "STREET_ADDRESS" && f.matched_text == "123 Main Street")
    );
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
    let transformed = datafog_core::transform(
        text,
        &result.findings,
        &datafog_core::TransformationConfig::new(datafog_core::TransformationStrategy::Redact),
    )
    .unwrap();
    assert!(transformed.text.contains("[STREET_ADDRESS]"));
    assert!(!transformed.text.contains("john@example.com"));
    let negative = session
        .scan(
            "The weather is clear.",
            ScanMode::ModelOnly,
            false,
            &Cancellation::default(),
        )
        .unwrap();
    assert!(negative.findings.is_empty());
    let structured = serde_json::json!({"first_name":"John","note":text,"number":12,"nested":["The weather is clear."]});
    let result = session
        .scan_structured(
            &structured,
            ScanMode::Combined,
            false,
            &Cancellation::default(),
        )
        .unwrap();
    assert!(result.findings.iter().any(|located|located.path=="/first_name" && located.finding.entity_type=="PERSON"));
    assert!(
        result
            .findings
            .iter()
            .any(|located| located.path == "/note"
                && located.finding.entity_type == "STREET_ADDRESS")
    );
    assert_eq!(result.evidence.len(), result.findings.len());
    session.health().unwrap();
    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(session.inspect().state, "closed");
}
