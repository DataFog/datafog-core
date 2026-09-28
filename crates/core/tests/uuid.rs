use datafog_core::{
    ScanConfig, parse_scan_and_transform_config, parse_scan_config, parse_transformation_config,
    scan_and_transform, scan_with_config, structured, transform, utf16_range,
};
use serde_json::{Value, json};

#[test]
fn shared_uuid_contract() {
    for line in include_str!("../../../fixtures/uuid.jsonl").lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let config = parse_scan_config(&row["config"]).unwrap();
        let findings = scan_with_config(text, &config);
        let matches:Vec<_>=findings.iter().filter(|f|f.entity_type=="UUID").map(|f| {
            assert_eq!(&text[f.byte_range.start..f.byte_range.end],f.matched_text);
            assert_eq!(f.confidence,None);
            assert_eq!(f.detector_name,"datafog-core/uuid");
            assert_eq!(f.detector_version.as_deref(),Some(env!("CARGO_PKG_VERSION")));
            let units:Vec<_>=text.encode_utf16().collect();let range=utf16_range(text,f.byte_range).unwrap();
            assert_eq!(String::from_utf16(&units[range.start..range.end]).unwrap(),f.matched_text);
            json!({"text":f.matched_text,"start":f.codepoint_range.start,"end":f.codepoint_range.end})
        }).collect();
        assert_eq!(json!(matches), row["matches"], "{}", row["id"]);
        let data = json!({"a/b":text,"items":[text]});
        let located = structured::scan(
            &data,
            &structured::parse_scan_config(&row["config"]).unwrap(),
        )
        .unwrap();
        for path in ["/a~1b", "/items/0"] {
            let local: Vec<_> = located
                .findings
                .iter()
                .filter(|f| f.path == path)
                .map(|f| f.finding.clone())
                .collect();
            assert_eq!(local, findings);
        }
        for strategy in ["redact", "mask", "remove"] {
            let policy = json!({"default":{"strategy":strategy},"entities":["UUID"]});
            let envelope = json!({"scan":row["config"],"transform":policy});
            let result =
                scan_and_transform(text, &parse_scan_and_transform_config(&envelope).unwrap())
                    .unwrap();
            let mut expected = text.to_owned();
            for f in findings.iter().filter(|f| f.entity_type == "UUID").rev() {
                let replacement = match strategy {
                    "redact" => "[UUID]".to_owned(),
                    "mask" => "*".repeat(36),
                    _ => String::new(),
                };
                expected.replace_range(f.byte_range.start..f.byte_range.end, &replacement);
            }
            assert_eq!(result.text, expected);
            assert_eq!(
                transform(
                    text,
                    &findings,
                    &parse_transformation_config(&policy).unwrap()
                )
                .unwrap(),
                result
            );
            for record in &result.transformations {
                assert_eq!(
                    &result.text[record.output_byte_range.start..record.output_byte_range.end],
                    record.replacement
                );
                assert_eq!(
                    &text[record.source_byte_range.start..record.source_byte_range.end].len(),
                    &36
                );
            }
            let structured = structured::scan_and_transform(
                &data,
                &structured::parse_scan_and_transform_config(&envelope).unwrap(),
            )
            .unwrap();
            assert_eq!(structured.data, json!({"a/b":expected,"items":[expected]}));
        }
    }
}

#[test]
fn uuid_configuration_and_selection_are_independent() {
    let text = "550e8400-e29b-41d4-a716-446655440000";
    let findings = scan_with_config(text, &ScanConfig::new().with_uuid_detection(true));
    assert!(!ScanConfig::default().uuid_detection_enabled());
    for invalid in [json!(null), json!(1), json!("true"), json!([]), json!({})] {
        let bad = json!({"detect_uuid":invalid});
        let error = parse_scan_config(&bad).unwrap_err();
        assert_eq!(error.path(), Some("/detect_uuid"));
        assert!(structured::parse_scan_config(&bad).is_err());
    }
    let policy = json!({"default":{"strategy":"redact"},"entities":["UUID"]});
    assert_eq!(
        scan_and_transform(
            text,
            &parse_scan_and_transform_config(&json!({"transform":policy})).unwrap()
        )
        .unwrap()
        .text,
        text
    );
    for allow in [
        json!({"exact":{"UUID":[text]}}),
        json!({"regex":{"UUID":[{"pattern":text}]}}),
    ] {
        let config = parse_transformation_config(
            &json!({"default":{"strategy":"redact"},"entities":["UUID"],"allow":allow}),
        )
        .unwrap();
        assert_eq!(transform(text, &findings, &config).unwrap().text, text);
    }
    let config=parse_transformation_config(&json!({"default":{"strategy":"remove"},"entities":["UUID"],"overrides":{"UUID":{"strategy":"redact"}},"allow":{"regex":{"UUID":[{"pattern":"550e"}]}}})).unwrap();
    assert_eq!(transform(text, &findings, &config).unwrap().text, "[UUID]");
}
