use datafog_core::{
    Finding, ScanConfig, TransformationConfig, TransformationStrategy, scan, scan_with_config,
    transform, utf16_range,
    workspace::{ReviewInput, Workspace, WorkspaceFinding},
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/adjacent-entities.json")).unwrap()
}

fn validate_ranges(text: &str, findings: &[Finding]) {
    for finding in findings {
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
        let range = utf16_range(text, finding.byte_range).unwrap();
        assert_eq!(
            String::from_utf16(&text.encode_utf16().collect::<Vec<_>>()[range.start..range.end])
                .unwrap(),
            finding.matched_text
        );
    }
}

#[test]
fn adjacent_entities_keep_exact_boundaries_selection_and_transformations() {
    for case in fixture()["pairs"].as_array().unwrap() {
        let left = case["left"].as_str().unwrap();
        let right = case["right"].as_str().unwrap();
        let left_type = case["leftType"].as_str().unwrap();
        let right_type = case["rightType"].as_str().unwrap();
        for separator in case["separators"].as_array().unwrap() {
            let separator = separator.as_str().unwrap();
            let text = format!("👋 e\u{301} {left}{separator}{right} 中");
            let findings = scan(&text);
            validate_ranges(&text, &findings);
            for (value, entity_type) in [(left, left_type), (right, right_type)] {
                assert!(
                    findings
                        .iter()
                        .any(|f| f.entity_type == entity_type && f.matched_text == value),
                    "missing exact {entity_type} in {text}: {findings:?}"
                );
                let config = ScanConfig::new()
                    .with_entities(vec![entity_type.to_owned()])
                    .unwrap();
                assert_eq!(
                    scan_with_config(&text, &config),
                    findings
                        .iter()
                        .filter(|f| f.entity_type == entity_type)
                        .cloned()
                        .collect::<Vec<_>>(),
                    "selected scan: {text}"
                );
                let config = datafog_core::parse_transformation_config(&serde_json::json!({
                    "default":{"strategy":"redact"},"entities":[entity_type]
                }))
                .unwrap();
                assert_eq!(
                    transform(&text, &findings, &config).unwrap().text,
                    text.replacen(value, &format!("[{entity_type}]"), 1),
                    "selected transformation: {text}"
                );
            }
            let result = transform(
                &text,
                &findings,
                &TransformationConfig::new(TransformationStrategy::Redact),
            )
            .unwrap();
            assert_eq!(
                result.text,
                format!("👋 e\u{301} [{left_type}]{separator}[{right_type}] 中"),
                "{text}"
            );
            let scanner: Vec<_> = findings
                .iter()
                .map(|finding| {
                    WorkspaceFinding::new(
                        &finding.entity_type,
                        &finding.matched_text,
                        finding.byte_range,
                    )
                })
                .collect();
            let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
            let mut plan = workspace
                .prepare_review(
                    &[ReviewInput {
                        text: &text,
                        scanner_findings: &scanner,
                        user_selections: &[],
                        corrections: &[],
                    }],
                    [3; 16],
                )
                .unwrap_or_else(|error| panic!("workspace review for {text}: {error:?}"));
            let preview = plan.preview().unwrap().text.clone();
            let parent = plan.expected_head().unwrap();
            let next = plan.candidate_head().unwrap();
            workspace.activate_saved(&mut plan, parent, next).unwrap();
            assert_eq!(workspace.restore(&preview).unwrap(), text);
            let reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
            assert_eq!(reopened.restore(&preview).unwrap(), text);
        }
    }
}

#[test]
fn valid_punctuation_and_isolated_values_are_not_split() {
    for case in fixture()["intact"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let entity_type = case["type"].as_str().unwrap();
        let config = ScanConfig::new()
            .with_entities(vec![entity_type.to_owned()])
            .unwrap();
        let findings = scan_with_config(text, &config);
        assert_eq!(findings.len(), 1, "{text}: {findings:?}");
        assert_eq!(findings[0].matched_text, text);
        assert_eq!(
            transform(
                text,
                &findings,
                &TransformationConfig::new(TransformationStrategy::Redact)
            )
            .unwrap()
            .text,
            format!("[{entity_type}]")
        );
    }
}
