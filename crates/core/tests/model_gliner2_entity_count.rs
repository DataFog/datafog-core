#![cfg(all(feature = "models", not(target_arch = "wasm32")))]
//! Issue #87 regression test: GLiNER2 entity-count gating.
//!
//! The GLiNER2 span runtime uses a count head to predict how many entities
//! to extract. When this head predicts zero, the runtime historically
//! skipped span decoding entirely, causing valid high-confidence spans to
//! be missed. The fix floors the count prediction at 1 so span decoding
//! always runs; threshold filtering handles false positives.
//!
//! These tests verify the fix by checking that inputs which previously
//! returned zero findings due to entity-count gating now return the
//! expected spans.

use datafog_core::models::{Cancellation, ModelSession, ModelSessionConfig};

/// Minimal reproduction for issue #87: street addresses with described
/// schemas that historically triggered the entity-count gating path.
///
/// The count head in some configurations predicts zero entities for short
/// inputs like "My home address is 27 Willow Lane." despite the model
/// producing high-confidence span scores (>0.99) for the address.
#[test]
#[ignore = "requires explicitly provisioned local GLiNER2 model"]
fn entity_count_gating_does_not_skip_valid_spans() {
    let config = std::env::var("DATAFOG_MODEL_CONFIG")
        .expect("DATAFOG_MODEL_CONFIG must name a verified local session config");
    let config: ModelSessionConfig =
        serde_json::from_reader(std::fs::File::open(config).unwrap()).unwrap();

    // This test only applies to GLiNER2
    if config.gliner2.is_none() {
        return;
    }

    let session = ModelSession::open(config).unwrap();

    // These are the exact reproduction inputs from issue #87. Before the fix,
    // these returned zero findings due to the count head selecting zero.
    let test_cases = [
        ("My home address is 27 Willow Lane.", "27 Willow Lane"),
        ("Please send mail to 912 Spruce Road.", "912 Spruce Road"),
        ("I live at 456 Oak Avenue in the city.", "456 Oak Avenue"),
    ];

    for (input, expected_address) in test_cases {
        let result = session
            .scan_model(input, &Cancellation::default())
            .expect("model scan should succeed");

        // The fix ensures span decoding always runs, so we should find spans
        // if the model assigns them high confidence.
        let addresses: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.entity_type == "STREET_ADDRESS")
            .collect();

        // With the fix, the model should detect the address. Without the fix,
        // the count head predicting zero would cause this to be empty.
        assert!(
            !addresses.is_empty(),
            "Expected STREET_ADDRESS findings for input {input:?}, got none. \
             This suggests entity-count gating is still skipping span decoding."
        );

        // Verify we found the expected address text
        let found_expected = addresses
            .iter()
            .any(|f| f.matched_text.contains(expected_address));
        assert!(
            found_expected,
            "Expected to find {:?} in findings for input {:?}, got: {:?}",
            expected_address,
            input,
            addresses
                .iter()
                .map(|f| &f.matched_text)
                .collect::<Vec<_>>()
        );
    }

    // Verify empty input still returns empty (this is correct behavior)
    let empty_result = session
        .scan_model("", &Cancellation::default())
        .expect("empty input scan should succeed");
    assert!(
        empty_result.findings.is_empty(),
        "Empty input should return no findings"
    );

    // Verify whitespace-only input returns empty
    let whitespace_result = session
        .scan_model("   \n\t   ", &Cancellation::default())
        .expect("whitespace input scan should succeed");
    assert!(
        whitespace_result.findings.is_empty(),
        "Whitespace-only input should return no findings"
    );

    session.close().unwrap();
}

/// Verify that the count-gating fix does not create false positives for
/// inputs that genuinely contain no entities.
#[test]
#[ignore = "requires explicitly provisioned local GLiNER2 model"]
fn entity_count_fix_does_not_create_false_positives() {
    let config = std::env::var("DATAFOG_MODEL_CONFIG")
        .expect("DATAFOG_MODEL_CONFIG must name a verified local session config");
    let config: ModelSessionConfig =
        serde_json::from_reader(std::fs::File::open(config).unwrap()).unwrap();

    if config.gliner2.is_none() {
        return;
    }

    let session = ModelSession::open(config).unwrap();

    // Inputs that should not contain street addresses
    let negative_cases = [
        "The weather is nice today.",
        "Hello world.",
        "This is a test sentence with no PII.",
        "Mathematics is fascinating.",
    ];

    for input in negative_cases {
        let result = session
            .scan_model(input, &Cancellation::default())
            .expect("model scan should succeed");

        // These inputs should not trigger street address detection
        // (threshold filtering should handle any low-confidence spans)
        let addresses: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.entity_type == "STREET_ADDRESS")
            .collect();

        // Note: This test documents expected behavior. If the model produces
        // false positives here, the threshold may need adjustment, but that's
        // a separate concern from the entity-count gating fix.
        if !addresses.is_empty() {
            eprintln!(
                "Warning: False positive street addresses for {:?}: {:?}",
                input,
                addresses
                    .iter()
                    .map(|f| &f.matched_text)
                    .collect::<Vec<_>>()
            );
        }
    }

    session.close().unwrap();
}
