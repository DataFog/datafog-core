use datafog_core::{
    DenyBoundary, LiteralDenyRule, ScanConfig, TextRange, parse_scan_and_transform_config,
    parse_scan_config, parse_transformation_config, scan, scan_and_transform, scan_with_config,
    transform, utf16_range,
    workspace::{
        ChoiceDisposition, PolicyEdits, ReviewInput, TypeRule, TypeRuleEdit, ValueChoiceEdit,
        Workspace, WorkspaceFinding,
    },
};
use serde_json::json;

fn denied(text: &str, rules: serde_json::Value) -> Vec<datafog_core::Finding> {
    scan_with_config(
        text,
        &parse_scan_config(&json!({"entities":["EMAIL"],"deny":rules})).unwrap(),
    )
}

#[test]
fn literal_defaults_and_explicit_options() {
    let text = "Atlas atlas _Atlas Atlas2 中Atlas Atlas中 (Atlas)";
    let result = denied(text, json!([{"phrase":"Atlas"}]));
    assert_eq!(
        result
            .iter()
            .map(|f| f.matched_text.as_str())
            .collect::<Vec<_>>(),
        vec!["Atlas", "Atlas"]
    );
    assert_eq!(denied("x(A) (A)y (A)", json!([{"phrase":"(A)"}])).len(), 1);
    assert_eq!(
        denied("xA+B.y", json!([{"phrase":"A+B.","boundary":"substring"}]))[0].matched_text,
        "A+B."
    );
    assert_eq!(
        denied("Atlas\u{0345}", json!([{"phrase":"Atlas"}])).len(),
        1
    );
    assert!(denied("a   b", json!([{"phrase":"a b"}])).is_empty());
    assert!(denied("é", json!([{"phrase":"é"}])).is_empty());
    assert!(
        denied(
            "STRASSE",
            json!([{"phrase":"straße","case_sensitive":false}])
        )
        .is_empty()
    );
    assert!(denied("A B", json!([])).is_empty());
    assert_eq!(scan("Atlas"), scan_with_config("Atlas", &ScanConfig::new()));
}

#[test]
fn unicode_case_folding_preserves_all_coordinate_systems() {
    let text = "👋 é Key ſecret";
    let result = denied(
        text,
        json!([{"phrase":"key","case_sensitive":false},{"phrase":"secret","case_sensitive":false}]),
    );
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].byte_range, TextRange { start: 9, end: 14 });
    assert_eq!(result[0].codepoint_range, TextRange { start: 5, end: 8 });
    for finding in result {
        assert_eq!(finding.entity_type, "DENYLIST");
        assert_eq!(finding.detector_name, "datafog-core/denylist");
        assert_eq!(
            finding.detector_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            &text[finding.byte_range.start..finding.byte_range.end],
            finding.matched_text
        );
        let chars: String = text
            .chars()
            .skip(finding.codepoint_range.start)
            .take(finding.codepoint_range.end - finding.codepoint_range.start)
            .collect();
        assert_eq!(chars, finding.matched_text);
        let range = utf16_range(text, finding.byte_range).unwrap();
        let units: Vec<_> = text.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&units[range.start..range.end]).unwrap(),
            finding.matched_text
        );
    }
}

#[test]
fn malformed_configuration_is_rejected_with_indexed_paths() {
    for (rules, path) in [
        (json!(null), "/deny"),
        (json!(1), "/deny"),
        (json!([null]), "/deny/0"),
        (json!([{}]), "/deny/0/phrase"),
        (json!([{"phrase":""}]), "/deny/0/phrase"),
        (json!([{"phrase":" \t"}]), "/deny/0/phrase"),
        (json!([{"phrase":1}]), "/deny/0/phrase"),
        (
            json!([{"phrase":"a","case_sensitive":null}]),
            "/deny/0/case_sensitive",
        ),
        (
            json!([{"phrase":"a","boundary":"regex"}]),
            "/deny/0/boundary",
        ),
        (json!([{"phrase":"a","boundary":1}]), "/deny/0/boundary"),
        (json!([{"phrase":"a","unknown":true}]), "/deny/0/unknown"),
    ] {
        let error = parse_scan_config(&json!({"deny":rules})).unwrap_err();
        assert_eq!(error.path(), Some(path), "{error:?}");
    }
    assert!(LiteralDenyRule::new(" ", true, DenyBoundary::Word).is_err());
    let rule = LiteralDenyRule::new("Atlas", true, DenyBoundary::Word).unwrap();
    assert_eq!(rule.phrase(), "Atlas");
    assert!(rule.case_sensitive());
    assert_eq!(rule.boundary(), DenyBoundary::Word);
    assert_eq!(ScanConfig::new().with_deny(vec![rule]).deny().len(), 1);
}

#[test]
fn duplicates_and_overlaps_keep_full_scanner_coverage() {
    let rules =
        json!([{"phrase":"aba","boundary":"substring"},{"phrase":"aba","boundary":"substring"}]);
    let findings = denied("ababa", rules.clone());
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].byte_range, TextRange { start: 0, end: 3 });
    assert_eq!(findings[1].byte_range, TextRange { start: 2, end: 5 });
    let result = scan_and_transform(
        "ababa",
        &parse_scan_and_transform_config(
            &json!({"scan":{"deny":rules},"transform":{"default":{"strategy":"redact"}}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result.text, "[DENYLIST]");
    // Explicit transform retains the established review requirement for crossing spans.
    assert!(
        transform(
            "ababa",
            &findings,
            &parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap()
        )
        .is_err()
    );
}

#[test]
fn explicit_deny_wins_filters_and_all_allowlist_forms() {
    let text = "Project Atlas jane@example.com";
    let scan_config = json!({"entities":["EMAIL"],"deny":[{"phrase":"Project Atlas"}]});
    let policy = json!({"default":{"strategy":"redact"},"entities":["EMAIL"],"allow":{"exact":{"DENYLIST":["Project Atlas"],"EMAIL":["jane@example.com"]},"regex":{"DENYLIST":[{"pattern":".*"}]}}});
    let config =
        parse_scan_and_transform_config(&json!({"scan":scan_config,"transform":policy})).unwrap();
    assert_eq!(
        scan_and_transform(text, &config).unwrap().text,
        "[DENYLIST] jane@example.com"
    );
    let findings = scan_with_config(text, &parse_scan_config(&scan_config).unwrap());
    let transform_config = parse_transformation_config(&policy).unwrap();
    assert_eq!(
        transform(text, &findings, &transform_config).unwrap().text,
        "[DENYLIST] jane@example.com"
    );
    let mut legacy = findings[0].clone();
    legacy.detector_name = "custom/legacy".into();
    assert_eq!(
        transform(text, &[legacy], &transform_config).unwrap().text,
        text
    );
    let merged = parse_scan_and_transform_config(&json!({
        "scan":{"deny":[{"phrase":"example","boundary":"substring"}]},
        "transform":{"default":{"strategy":"redact"},"allow":{"regex":{"DENYLIST":[{"pattern":".*"}]}}}
    })).unwrap();
    assert_eq!(
        scan_and_transform("jane@example.com", &merged)
            .unwrap()
            .text,
        "[DENYLIST]"
    );
    // A builtin spanning a deny rule must not discard deny provenance during merging.
    let config=parse_scan_and_transform_config(&json!({"scan":{"deny":[{"phrase":"example","boundary":"substring"}]},"transform":{"default":{"strategy":"redact"},"entities":["SSN"]}})).unwrap();
    assert_eq!(
        scan_and_transform("jane@example.com", &config)
            .unwrap()
            .text,
        "jane@[DENYLIST].com"
    );
}

#[test]
fn workspace_saved_keep_or_not_sensitive_cannot_override_a_current_explicit_deny() {
    let text = "Project Atlas";
    for edits in [
        PolicyEdits {
            rules: vec![],
            choices: vec![ValueChoiceEdit {
                entity_type: "DENYLIST".into(),
                value: text.into(),
                disposition: Some(ChoiceDisposition::NotSensitive),
            }],
        },
        PolicyEdits {
            rules: vec![TypeRuleEdit {
                entity_type: "DENYLIST".into(),
                rule: Some(TypeRule::Keep),
            }],
            choices: vec![],
        },
    ] {
        let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
        let mut plan = workspace.prepare_policy_changes(&edits, [3; 16]).unwrap();
        let expected = plan.expected_head().unwrap();
        let next = plan.candidate_head().unwrap();
        workspace.activate_saved(&mut plan, expected, next).unwrap();
        let core = denied(text, json!([{"phrase":text}])).remove(0);
        let mut finding =
            WorkspaceFinding::new(core.entity_type, core.matched_text, core.byte_range);
        finding.detector_name = Some(core.detector_name);
        let findings = [finding.clone()];
        let review = workspace
            .review(ReviewInput {
                text,
                scanner_findings: &findings,
                user_selections: &[],
                corrections: &[],
            })
            .unwrap();
        assert!(review.conflicts.is_empty());
        assert!(review.candidates.iter().any(|c| c.eligible && c.selected));
        let mut duplicate = finding.clone();
        duplicate.detector_name = Some("aaa/custom".into());
        for (scanner, user) in [
            (&findings[..], std::slice::from_ref(&duplicate)),
            (std::slice::from_ref(&duplicate), &findings[..]),
        ] {
            let review = workspace
                .review(ReviewInput {
                    text,
                    scanner_findings: scanner,
                    user_selections: user,
                    corrections: &[],
                })
                .unwrap();
            assert!(review.conflicts.is_empty());
            assert!(review.candidates.iter().any(|c| c.eligible && c.selected));
        }
        let plan = workspace.prepare(text, &findings, [4; 16]).unwrap();
        assert!(plan.preview().unwrap().text.starts_with("[[DENYLIST_"));
        finding.detector_name = Some("custom/legacy".into());
        let findings = [finding];
        let review = workspace
            .review(ReviewInput {
                text,
                scanner_findings: &findings,
                user_selections: &[],
                corrections: &[],
            })
            .unwrap();
        assert!(review.candidates.iter().all(|c| !c.eligible));
    }
}
