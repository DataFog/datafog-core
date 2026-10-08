use super::*;
use serde_json::json;

#[test]
fn selected_scans_match_filtered_defaults_for_every_text_detector() {
    let fixtures = [
        include_str!("../../../fixtures/development.jsonl"),
        include_str!("../../../fixtures/german.jsonl"),
        include_str!("../../../fixtures/uuid.jsonl"),
        include_str!("../../../fixtures/credential-overlaps.jsonl"),
        include_str!("../../../fixtures/credential-uri.jsonl"),
        include_str!("../../../fixtures/api-key.jsonl"),
        include_str!("../../../fixtures/bearer-token.jsonl"),
        include_str!("../../../fixtures/jwt.jsonl"),
        include_str!("../../../fixtures/npi.jsonl"),
        include_str!("../../../fixtures/us-routing-number.jsonl"),
        include_str!("../../../fixtures/private-key.jsonl"),
        include_str!("../../../fixtures/email-context.jsonl"),
    ];
    let mut covered = BTreeSet::new();
    for fixture in fixtures {
        for line in fixture.lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let text = row["text"].as_str().unwrap();
            let config = parse_scan_config(row.get("config").unwrap_or(&json!({}))).unwrap();
            let baseline = scan_with_config(text, &config);
            for name in capabilities()
                .supported_entities
                .into_iter()
                .filter(|name| name != "PERSON")
            {
                let selected = config.clone().with_entities(vec![name.clone()]).unwrap();
                let expected: Vec<_> = baseline
                    .iter()
                    .filter(|finding| finding.entity_type == name)
                    .cloned()
                    .collect();
                if !expected.is_empty() {
                    covered.insert(name.clone());
                }
                assert_eq!(
                    scan_with_config(text, &selected),
                    expected,
                    "{}: {name}",
                    row["id"]
                );
            }
        }
    }
    let expected: BTreeSet<_> = capabilities()
        .supported_entities
        .into_iter()
        .filter(|name| name != "PERSON")
        .collect();
    assert_eq!(covered, expected);
}

#[test]
fn invalid_text_selections_have_precise_safe_errors() {
    for (value, path, reason) in [
        (
            json!({"entities": []}),
            "/entities",
            PrivacyErrorReason::EmptyValue,
        ),
        (
            json!({"entities": null}),
            "/entities",
            PrivacyErrorReason::InvalidType,
        ),
        (
            json!({"entities": "EMAIL"}),
            "/entities",
            PrivacyErrorReason::InvalidType,
        ),
        (
            json!({"entities": [2]}),
            "/entities/0",
            PrivacyErrorReason::InvalidType,
        ),
        (
            json!({"entities": ["EMAIL", "EMAIL"]}),
            "/entities/1",
            PrivacyErrorReason::DuplicateValue,
        ),
        (
            json!({"entities": ["PERSON"]}),
            "/entities/0",
            PrivacyErrorReason::InvalidValue,
        ),
        (
            json!({"entities": ["NOT_SUPPORTED"]}),
            "/entities/0",
            PrivacyErrorReason::InvalidValue,
        ),
        (
            json!({"entities": ["email"]}),
            "/entities/0",
            PrivacyErrorReason::InvalidValue,
        ),
    ] {
        let error = parse_scan_config(&value).unwrap_err();
        assert_eq!(error.code(), PrivacyErrorCode::InvalidConfiguration);
        assert_eq!(error.path(), Some(path));
        assert_eq!(error.reason(), Some(reason));
    }
    let error = parse_scan_and_transform_config(
        &json!({"scan": {"entities": []}, "transform": {"default": {"strategy": "redact"}}}),
    )
    .unwrap_err();
    assert_eq!(error.path(), Some("/scan/entities"));
    assert!(structured::parse_scan_config(&json!({"entities": ["EMAIL"]})).is_err());
}

#[test]
fn selection_does_not_activate_uuid_or_locale_detectors() {
    for (entity, text, active) in [
        (
            "UUID",
            "550e8400-e29b-41d4-a716-446655440000",
            json!({"detect_uuid": true}),
        ),
        (
            "DE_IBAN",
            "DE44 5001 0517 5407 3249 31",
            json!({"locale": "de"}),
        ),
    ] {
        let selected = ScanConfig::new()
            .with_entities(vec![entity.into()])
            .unwrap();
        assert!(scan_with_config(text, &selected).is_empty());
        let active = parse_scan_config(&active)
            .unwrap()
            .with_entities(vec![entity.into()])
            .unwrap();
        assert_eq!(scan_with_config(text, &active).len(), 1);
        let excluded = active.with_entities(vec!["EMAIL".into()]).unwrap();
        assert!(scan_with_config(text, &excluded).is_empty());
    }
}

#[test]
fn scan_and_transform_selection_preserves_unselected_content_and_unicode_offsets() {
    let text = "👋 person@example.com / 123-45-6789";
    let config = parse_scan_and_transform_config(
        &json!({"scan": {"entities": ["EMAIL"]}, "transform": {"default": {"strategy": "redact"}}}),
    )
    .unwrap();
    let result = scan_and_transform(text, &config).unwrap();
    assert_eq!(result.text, "👋 [EMAIL] / 123-45-6789");
    let finding = &scan_with_config(text, config.scan_config())[0];
    assert_eq!(finding.codepoint_range.start, 2);
    assert_eq!(finding.byte_range.start, 5);
}
