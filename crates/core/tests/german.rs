use datafog_core::{
    Finding, TextRange, parse_scan_and_transform_config, parse_scan_config,
    parse_transformation_config, scan_and_transform, scan_with_config, structured, transform,
    utf16_range,
};
use serde_json::{Value, json};

fn project(text: &str, findings: &[Finding]) -> Value {
    json!(findings.iter().filter(|f| f.entity_type.starts_with("DE_")).map(|f| {
        assert_eq!(&text[f.byte_range.start..f.byte_range.end], f.matched_text);
        assert_eq!(text.chars().skip(f.codepoint_range.start).take(f.codepoint_range.end-f.codepoint_range.start).collect::<String>(), f.matched_text);
        let range = utf16_range(text, f.byte_range).unwrap();
        let units: Vec<_> = text.encode_utf16().collect();
        assert_eq!(String::from_utf16(&units[range.start..range.end]).unwrap(), f.matched_text);
        assert_eq!(f.confidence, None);
        assert_eq!(f.detector_name, format!("datafog-core/{}", f.entity_type.to_lowercase().replace('_', "-")));
        assert_eq!(f.detector_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
        json!({"label":f.entity_type,"text":f.matched_text,"start":f.codepoint_range.start,"end":f.codepoint_range.end})
    }).collect::<Vec<_>>())
}

#[test]
fn shared_german_conformance() {
    for line in include_str!("../../../fixtures/german.jsonl").lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let config = parse_scan_config(&row["config"]).unwrap();
        assert_eq!(config.locale(), row["config"]["locale"].as_str());
        let findings = scan_with_config(text, &config);
        assert_eq!(project(text, &findings), row["entities"], "{}", row["id"]);
        let data = json!({"a/b":text,"array":[text],"z":text});
        let structured_config = structured::parse_scan_config(&row["config"]).unwrap();
        let located = structured::scan(&data, &structured_config).unwrap();
        for path in ["/a~1b", "/array/0", "/z"] {
            let local: Vec<_> = located
                .findings
                .iter()
                .filter(|f| f.path == path)
                .map(|f| f.finding.clone())
                .collect();
            assert_eq!(
                project(text, &local),
                row["entities"],
                "{} {path}",
                row["id"]
            );
        }
        for case in row["transforms"].as_array().into_iter().flatten() {
            let transform_config = parse_transformation_config(&case["config"]).unwrap();
            let result = transform(text, &findings, &transform_config).unwrap();
            let envelope = json!({"scan":row["config"],"transform":case["config"]});
            assert_eq!(
                result,
                scan_and_transform(text, &parse_scan_and_transform_config(&envelope).unwrap())
                    .unwrap()
            );
            assert_eq!(result.text, case["text"].as_str().unwrap(), "{}", row["id"]);
            for record in &result.transformations {
                assert_eq!(
                    &result.text[record.output_byte_range.start..record.output_byte_range.end],
                    record.replacement
                );
                assert!(
                    findings
                        .iter()
                        .any(|f| f.byte_range == record.source_byte_range
                            && f.entity_type == record.entity_type)
                );
            }
            let result = structured::scan_and_transform(
                &data,
                &structured::parse_scan_and_transform_config(&envelope).unwrap(),
            )
            .unwrap();
            assert_eq!(
                result.data,
                json!({"a/b":case["text"],"array":[case["text"]],"z":case["text"]})
            );
        }
    }
}

#[test]
fn context_is_local_and_config_validation_is_preserved() {
    let data = json!({"Steuer-ID":"12345678901", "a":"Steuer-ID", "b":"12345678901", "c":["Passport","C12345678"],"d":"DE4450010517","e":"5407324931"});
    let config = structured::parse_scan_config(&json!({"locale":"de"})).unwrap();
    assert!(
        structured::scan(&data, &config)
            .unwrap()
            .findings
            .iter()
            .all(|f| !f.finding.entity_type.starts_with("DE_"))
    );
    for bad in [
        json!({"locale":""}),
        json!({"locale":" \t"}),
        json!({"locale":null}),
        json!({"locale":1}),
        json!({"locales":["de"]}),
    ] {
        assert!(parse_scan_config(&bad).is_err());
        assert!(structured::parse_scan_config(&bad).is_err());
    }
    let result = scan_and_transform(
        "DE44500105175407324931",
        &parse_scan_and_transform_config(
            &json!({"transform":{"default":{"strategy":"redact"},"entities":["DE_IBAN"]}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result.text, "DE44500105175407324931");
}

#[test]
fn generic_overlaps_are_retained_and_existing_selection_prefers_german_spans() {
    for (text, label, generic, expected) in [
        (
            "DE44\t4111111111111111\t11",
            "DE_IBAN",
            "CREDIT_CARD",
            "[DE_IBAN]",
        ),
        ("DE 123456789", "DE_VAT_ID", "SSN", "[DE_VAT_ID]"),
        (
            "Steuer-ID 12345678901",
            "DE_TAX_ID",
            "PHONE",
            "Steuer-ID [DE_TAX_ID]",
        ),
        ("DE-10115", "DE_POSTAL_CODE", "ZIP_CODE", "[DE_POSTAL_CODE]"),
    ] {
        let config = parse_scan_config(&json!({"locale":"de"})).unwrap();
        let findings = scan_with_config(text, &config);
        assert!(findings.iter().any(|f| f.entity_type == label), "{text}");
        assert!(
            findings.iter().any(|f| f.entity_type == generic),
            "{text}: {findings:?}"
        );
        let result = transform(
            text,
            &findings,
            &parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap(),
        )
        .unwrap();
        assert_eq!(result.text, expected);
        assert_eq!(result.transformations.len(), 1);
        if label == "DE_TAX_ID" {
            assert_eq!(
                findings
                    .iter()
                    .find(|f| f.entity_type == label)
                    .unwrap()
                    .byte_range,
                TextRange { start: 10, end: 21 }
            );
            assert_eq!(
                findings
                    .iter()
                    .find(|f| f.entity_type == generic)
                    .unwrap()
                    .byte_range,
                TextRange { start: 10, end: 21 }
            );
        }
    }
}

#[test]
fn unchanged_base_fixtures() {
    for corpus in [
        include_str!("../../../fixtures/development.jsonl"),
        include_str!("../../../fixtures/final.jsonl"),
    ] {
        for line in corpus.lines() {
            let row: Value = serde_json::from_str(line).unwrap();
            let findings = datafog_core::scan(row["text"].as_str().unwrap());
            let actual: Vec<_> = findings.iter().map(|f| json!({"label":f.entity_type,"text":f.matched_text,"start":f.codepoint_range.start,"end":f.codepoint_range.end})).collect();
            assert_eq!(json!(actual), row["entities"], "{}", row["id"]);
        }
    }
}
