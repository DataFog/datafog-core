#[path = "support/synthetic_fixtures.rs"]
mod synthetic_fixtures;
use datafog_core::{
    Finding, parse_scan_and_transform_config, parse_scan_config, parse_transformation_config,
    scan_and_transform, scan_with_config, structured, transform, utf16_range,
};
use serde_json::{Value, json};

fn project(text: &str, findings: &[Finding]) -> Value {
    json!(findings.iter().filter(|f| f.entity_type == "API_KEY").map(|f| {
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
fn shared_api_key_conformance() {
    for line in include_str!("../../../fixtures/api-key.jsonl").lines() {
        let row: Value = synthetic_fixtures::expand(serde_json::from_str(line).unwrap());
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
fn full_api_key_wins_over_explicit_nested_jwt() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0IiwiZXhwIjowfQ.c2ln";
    let text = format!("Authorization: Bearer ghs_12345_{jwt}");
    let mut findings = datafog_core::scan(&text);
    let start = text.find(jwt).unwrap();
    let mut nested = datafog_core::scan(jwt).remove(0);
    nested.byte_range.start += start;
    nested.byte_range.end += start;
    nested.codepoint_range.start += start;
    nested.codepoint_range.end += start;
    findings.push(nested);
    let config = parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap();
    assert_eq!(
        transform(&text, &findings, &config).unwrap().text,
        "Authorization: Bearer [API_KEY]"
    );
    let selected = parse_transformation_config(
        &json!({"default":{"strategy":"redact"},"entities":["API_KEY"]}),
    )
    .unwrap();
    assert_eq!(
        transform(&text, &findings, &selected).unwrap().text,
        "Authorization: Bearer [API_KEY]"
    );
}

#[test]
fn long_installation_tokens_are_not_truncated() {
    let token = format!("ghs_APP_{}.payload.signature", "a".repeat(100_000));
    let findings = datafog_core::scan(&token);
    let key = findings
        .iter()
        .find(|f| f.entity_type == "API_KEY")
        .unwrap();
    assert_eq!(key.matched_text, token);
    assert_eq!(key.byte_range.end, token.len());
}
