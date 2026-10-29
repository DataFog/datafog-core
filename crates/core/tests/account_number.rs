use datafog_core::{
    Finding, parse_scan_and_transform_config, parse_scan_config, parse_transformation_config,
    scan_and_transform, scan_with_config, structured, transform, utf16_range,
};
use serde_json::{Value, json};

fn project(text: &str, findings: &[Finding]) -> Value {
    json!(findings.iter().filter(|f| f.entity_type.eq("ACCOUNT_NUMBER")).map(|f| {
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
fn labelled_account_number_conformance() {
    for line in [
        include_str!("../../../fixtures/account-number.jsonl"),
        include_str!("../../../fixtures/masked-account-number.jsonl"),
    ]
    .into_iter()
    .flat_map(str::lines)
    {
        let row: Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let config = parse_scan_config(&row["config"]).unwrap();
        assert_eq!(config.locale(), row["config"]["locale"].as_str());
        let findings = scan_with_config(text, &config);
        let selected = scan_with_config(
            text,
            &parse_scan_config(&json!({"entities":["ACCOUNT_NUMBER"]})).unwrap(),
        );
        assert_eq!(
            project(text, &selected),
            row["entities"],
            "{} selected",
            row["id"]
        );
        assert_eq!(project(text, &findings), row["entities"], "{}", row["id"]);
        if row["entities"].as_array().unwrap().is_empty() {
            let config = parse_transformation_config(
                &json!({"entities":["ACCOUNT_NUMBER"],"default":{"strategy":"redact"}}),
            )
            .unwrap();
            assert_eq!(
                transform(text, &findings, &config).unwrap().text,
                text,
                "{} negative transform",
                row["id"]
            );
        }
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
fn context_does_not_cross_structured_fields() {
    let data = json!({"account number":"12345678", "a":"account number", "b":"12345678"});
    let result =
        structured::scan(&data, &structured::parse_scan_config(&json!({})).unwrap()).unwrap();
    assert!(
        result
            .findings
            .iter()
            .all(|f| f.finding.entity_type != "ACCOUNT_NUMBER")
    );
}

#[test]
fn account_addition_keeps_existing_card_findings_and_selection() {
    let text = "account number: 4111111111111111";
    let findings = datafog_core::scan(text);
    assert!(
        findings
            .iter()
            .any(|finding| finding.entity_type == "CREDIT_CARD"
                && finding.matched_text == "4111111111111111")
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.entity_type == "ACCOUNT_NUMBER"
                && finding.matched_text == "4111111111111111")
    );
    for entity in ["CREDIT_CARD", "ACCOUNT_NUMBER"] {
        let config = parse_transformation_config(
            &json!({"entities":[entity], "default":{"strategy":"redact"}}),
        )
        .unwrap();
        assert_eq!(
            transform(text, &findings, &config).unwrap().text,
            format!("account number: [{entity}]")
        );
    }
    let default = parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap();
    assert_eq!(
        transform(text, &findings, &default).unwrap().text,
        "account number: [ACCOUNT_NUMBER]"
    );
    let allow = parse_transformation_config(&json!({"entities":["ACCOUNT_NUMBER"], "default":{"strategy":"redact"}, "allow":{"regex":{"ACCOUNT_NUMBER":[{"pattern":"4111[0-9]+"}]}}})).unwrap();
    assert_eq!(transform(text, &findings, &allow).unwrap().text, text);
}

#[test]
fn masked_accounts_keep_literal_workspace_identity_and_restore_visible_text() {
    use datafog_core::workspace::{Workspace, WorkspaceFinding};
    let mut workspace = Workspace::create([51; 16], [52; 16]).unwrap();
    let mut markers = Vec::new();
    for (index, text) in [
        "account ending in 1234",
        "account number: 00121234",
        "account number: XXXX1234",
        "account number: 00991234",
        "acct ending in 1234",
        "acct no.: xxxx1234",
    ]
    .into_iter()
    .enumerate()
    {
        let findings: Vec<_> = scan_with_config(
            text,
            &parse_scan_config(&json!({"entities":["ACCOUNT_NUMBER"]})).unwrap(),
        )
        .into_iter()
        .map(|finding| {
            WorkspaceFinding::new(
                finding.entity_type,
                finding.matched_text,
                finding.byte_range,
            )
        })
        .collect();
        assert_eq!(findings.len(), 1);
        let mut pending = workspace
            .prepare(text, &findings, [60 + u8::try_from(index).unwrap(); 16])
            .unwrap();
        let preview = pending.preview().unwrap();
        let range = preview.transformations[0].output_byte_range;
        markers.push(preview.text[range.start..range.end].to_owned());
        let expected = if index == 5 {
            "acct no.: XXXX1234"
        } else {
            text
        };
        let protected = preview.text.clone();
        let parent = pending.expected_head().unwrap();
        let candidate = pending.candidate_head().unwrap();
        workspace
            .activate_saved(&mut pending, parent, candidate)
            .unwrap();
        assert_eq!(workspace.restore(&protected).unwrap(), expected);
    }
    assert_ne!(markers[0], markers[1]);
    assert_ne!(markers[0], markers[2]);
    assert_ne!(markers[0], markers[3]);
    assert_ne!(markers[1], markers[3]);
    assert_eq!(markers[0], markers[4]);
    assert_eq!(markers[2], markers[5]);
}

#[test]
fn ending_allowlist_matches_the_observed_suffix() {
    let text = "account ending in 1234";
    let config = parse_scan_and_transform_config(&json!({
        "transform":{"default":{"strategy":"redact"},"entities":["ACCOUNT_NUMBER"],
            "allow":{"regex":{"ACCOUNT_NUMBER":[{"pattern":"1234"}]}}}
    }))
    .unwrap();
    assert_eq!(scan_and_transform(text, &config).unwrap().text, text);
}
