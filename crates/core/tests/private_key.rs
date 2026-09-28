use datafog_core::{
    Finding, parse_scan_and_transform_config, parse_scan_config, parse_transformation_config,
    scan_and_transform, scan_with_config, structured, transform, utf16_range,
};
use serde_json::{Value, json};

fn project(text: &str, findings: &[Finding]) -> Value {
    json!(findings.iter().filter(|f| f.entity_type == "PRIVATE_KEY").map(|f| {
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
fn shared_private_key_conformance() {
    for line in include_str!("../../../fixtures/private-key.jsonl").lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let config = parse_scan_config(&row["config"]).unwrap();
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
fn blocks_do_not_cross_fields_and_source_is_validated() {
    let data = json!({"header":"-----BEGIN PRIVATE KEY-----", "body":"YWJjZA==", "footer":"-----END PRIVATE KEY-----"});
    let config = structured::parse_scan_config(&json!({})).unwrap();
    assert!(
        structured::scan(&data, &config)
            .unwrap()
            .findings
            .is_empty()
    );
    let text = "-----BEGIN PRIVATE KEY-----\nYWJjZA==\n-----END PRIVATE KEY-----";
    let mut findings = datafog_core::scan(text);
    findings[0].matched_text.push('x');
    let config = parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap();
    assert!(transform(text, &findings, &config).is_err());
}

#[test]
fn repeated_unclosed_headers_recover_without_rescanning() {
    let mut text = "-----BEGIN PRIVATE KEY-----\nYWJj\n".repeat(10_000);
    let start = text.len();
    text.push_str("-----BEGIN PRIVATE KEY-----\nYWJj\n-----END PRIVATE KEY-----");
    let findings: Vec<_> = datafog_core::scan(&text)
        .into_iter()
        .filter(|f| f.entity_type == "PRIVATE_KEY")
        .collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].byte_range.start, start);
}
