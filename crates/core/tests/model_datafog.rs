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
    let result = session.scan_model(text, &Cancellation::default()).unwrap();
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

    // Routing: rule entities come from rule detectors; model entities are the
    // plain model scan's findings for those types, and nothing else is returned.
    let text =
        "John Smith lives at 123 Main Street. Card 4111 1111 1111 1111, email john@example.com.";
    let rules_only = session
        .routed_config(&serde_json::json!({"entities": ["CREDIT_CARD", "EMAIL"]}))
        .unwrap();
    assert!(!rules_only.needs_model());
    let routed = session
        .scan_routed(text, &rules_only, &Cancellation::default())
        .unwrap();
    let rule_findings = datafog_core::scan_with_config(
        text,
        &datafog_core::ScanConfig::new()
            .with_entities(vec!["CREDIT_CARD".into(), "EMAIL".into()])
            .unwrap(),
    );
    assert_eq!(routed.findings, rule_findings);
    let mixed = session
        .routed_config(&serde_json::json!({"entities": ["STREET_ADDRESS", "EMAIL"]}))
        .unwrap();
    assert!(mixed.needs_model());
    let routed = session
        .scan_routed(text, &mixed, &Cancellation::default())
        .unwrap();
    let model_addresses: Vec<_> = session
        .scan_model(text, &Cancellation::default())
        .unwrap()
        .findings
        .into_iter()
        .filter(|finding| finding.entity_type == "STREET_ADDRESS")
        .collect();
    let routed_addresses: Vec<_> = routed
        .findings
        .iter()
        .filter(|finding| finding.entity_type == "STREET_ADDRESS")
        .cloned()
        .collect();
    assert_eq!(routed_addresses, model_addresses);
    let emails: Vec<_> = routed
        .findings
        .iter()
        .filter(|finding| finding.entity_type == "EMAIL")
        .collect();
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].detector_name, "datafog-core/email");
    assert!(
        routed
            .findings
            .iter()
            .all(|finding| matches!(finding.entity_type.as_str(), "STREET_ADDRESS" | "EMAIL"))
    );
    assert_eq!(routed.evidence.len(), routed.findings.len());

    // Without a configuration, scan routes like `{}`: rule findings as from
    // the rule scanner, plus the model's labels that rules do not produce.
    let default = session.scan(text, &Cancellation::default()).unwrap();
    let empty = session
        .scan_routed(
            text,
            &session.routed_config(&serde_json::json!({})).unwrap(),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(default.findings, empty.findings);
    let rules: Vec<_> = default
        .findings
        .iter()
        .filter(|finding| !finding.detector_name.starts_with("datafog-core/model/"))
        .cloned()
        .collect();
    assert_eq!(rules, datafog_core::scan(text));

    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(
        session.scan("", &Cancellation::default()).unwrap_err().code,
        "model_session_closed"
    );
}
