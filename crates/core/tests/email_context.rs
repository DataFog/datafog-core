use datafog_core::{
    parse_scan_and_transform_config, parse_scan_config, parse_transformation_config, scan,
    scan_and_transform, scan_with_config, structured, transform, utf16_range,
};
use serde_json::{Value, json};

#[test]
fn email_context_preserves_source_spans_and_syntax() {
    for line in include_str!("../../../fixtures/email-context.jsonl").lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let findings = scan_with_config(text, &parse_scan_config(&row["config"]).unwrap());
        let emails: Vec<_> = findings
            .iter()
            .filter(|f| f.entity_type == "EMAIL")
            .collect();
        let matches: Vec<_> = emails.iter().map(|f| {
            assert_eq!(&text[f.byte_range.start..f.byte_range.end], f.matched_text);
            let range = utf16_range(text, f.byte_range).unwrap();
            let units: Vec<_> = text.encode_utf16().collect();
            assert_eq!(String::from_utf16(&units[range.start..range.end]).unwrap(), f.matched_text);
            json!({"text":f.matched_text,"start":f.codepoint_range.start,"end":f.codepoint_range.end})
        }).collect();
        assert_eq!(json!(matches), row["matches"], "{}", row["id"]);
        let data = json!({"source":text});
        let located = structured::scan(
            &data,
            &structured::parse_scan_config(&row["config"]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            located
                .findings
                .iter()
                .map(|f| f.finding.clone())
                .collect::<Vec<_>>(),
            findings
        );
        for strategy in ["redact", "mask", "remove"] {
            let policy = json!({"default":{"strategy":strategy},"entities":["EMAIL"]});
            let envelope = json!({"scan":row["config"],"transform":policy});
            let result =
                scan_and_transform(text, &parse_scan_and_transform_config(&envelope).unwrap())
                    .unwrap();
            assert_eq!(
                result.text, row["outputs"][strategy],
                "{} {strategy}",
                row["id"]
            );
            assert_eq!(
                transform(
                    text,
                    &findings,
                    &parse_transformation_config(&policy).unwrap()
                )
                .unwrap(),
                result
            );
            for (record, finding) in result.transformations.iter().zip(&emails) {
                assert_eq!(record.source_byte_range, finding.byte_range);
                assert_eq!(record.source_codepoint_range, finding.codepoint_range);
                assert_eq!(
                    &result.text[record.output_byte_range.start..record.output_byte_range.end],
                    record.replacement
                );
            }
            let result = structured::scan_and_transform(
                &data,
                &structured::parse_scan_and_transform_config(&envelope).unwrap(),
            )
            .unwrap();
            assert_eq!(result.data, json!({"source":row["outputs"][strategy]}));
        }
        if row["config"]["format"] == "text" {
            assert_eq!(scan(text), findings);
        }
    }
}

#[test]
fn invalid_format_is_reported_at_the_configuration_path() {
    for format in [
        json!(null),
        json!(1),
        json!(true),
        json!({}),
        json!("yaml"),
        json!("ENV"),
    ] {
        let config = json!({"format":format});
        assert_eq!(
            parse_scan_config(&config).unwrap_err().path(),
            Some("/format")
        );
        assert_eq!(
            structured::parse_scan_config(&config).unwrap_err().path(),
            Some("/format")
        );
        let envelope = json!({"scan":config,"transform":{"default":{"strategy":"redact"}}});
        assert_eq!(
            parse_scan_and_transform_config(&envelope)
                .unwrap_err()
                .path(),
            Some("/scan/format")
        );
    }
}
