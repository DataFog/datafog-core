use datafog_core::{
    TextRange,
    workspace::{
        ChoiceDisposition, ErrorCode, FindingSource, PolicyEdits, ReviewCorrection, ReviewInput,
        ValueChoiceEdit, Workspace, WorkspaceFinding,
    },
};
use serde_json::Value;
fn fresh() -> Workspace {
    Workspace::create([1; 16], [2; 16]).unwrap()
}
fn finding(text: &str, label: &str, start: usize, end: usize) -> WorkspaceFinding {
    WorkspaceFinding::new(label, &text[start..end], TextRange { start, end })
}
fn input<'a>(
    text: &'a str,
    scanner: &'a [WorkspaceFinding],
    user: &'a [WorkspaceFinding],
    corrections: &'a [ReviewCorrection],
) -> ReviewInput<'a> {
    ReviewInput {
        text,
        scanner_findings: scanner,
        user_selections: user,
        corrections,
    }
}
fn save(workspace: &mut Workspace, mut plan: datafog_core::workspace::PreparedChange) {
    let expected = plan.expected_head().unwrap();
    let candidate = plan.candidate_head().unwrap();
    workspace
        .activate_saved(&mut plan, expected, candidate)
        .unwrap();
}
fn waiver(workspace: &mut Workspace, label: &str, value: &str, op: u8) {
    let plan = workspace
        .prepare_policy_changes(
            &PolicyEdits {
                rules: vec![],
                choices: vec![ValueChoiceEdit {
                    entity_type: label.into(),
                    value: value.into(),
                    disposition: Some(ChoiceDisposition::NotSensitive),
                }],
            },
            [op; 16],
        )
        .unwrap();
    save(workspace, plan);
}
#[test]
fn shared_positive_negative_and_unicode_corpus() {
    for line in include_str!("../../../fixtures/workspace/review-conflicts.jsonl").lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let text = case["text"].as_str().unwrap();
        let mut workspace = fresh();
        for choice in case["choices"].as_array().into_iter().flatten() {
            waiver(
                &mut workspace,
                choice["entityType"].as_str().unwrap(),
                choice["value"].as_str().unwrap(),
                3,
            );
        }
        let parse = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|raw| {
                    let mut f = finding(
                        text,
                        raw["entityType"].as_str().unwrap(),
                        raw["start"].as_u64().unwrap() as usize,
                        raw["end"].as_u64().unwrap() as usize,
                    );
                    if let Some(value) = raw["matchedText"].as_str() {
                        f.matched_text = value.into();
                    }
                    if let Some(value) = raw["detectorName"].as_str() {
                        f.detector_name = Some(value.into());
                    }
                    f.needs_review = raw["needsReview"].as_bool().unwrap_or(true);
                    f.source = match raw["source"].as_str() {
                        Some("user") => FindingSource::User,
                        Some("recall") => FindingSource::Recall,
                        _ => FindingSource::Scanner,
                    };
                    f
                })
                .collect::<Vec<_>>()
        };
        let scanner = parse("scanner");
        let user = parse("user");
        let args = input(text, &scanner, &user, &[]);
        let before = workspace.export_bundle().unwrap();
        let result = workspace.review(args);
        match case["expected"].as_str().unwrap() {
            "invalid_finding" => assert_eq!(
                result.err().unwrap().code,
                ErrorCode::InvalidFinding,
                "{}",
                case["id"]
            ),
            expected => {
                let result = result.unwrap();
                assert_eq!(
                    result.conflicts.is_empty(),
                    expected == "ok",
                    "{}",
                    case["id"]
                );
                for c in &result.conflicts {
                    assert_eq!(
                        c.codepoint_range.start,
                        text[..c.byte_range.start].chars().count()
                    );
                    assert_eq!(
                        c.utf16_range.end,
                        text[..c.byte_range.end].encode_utf16().count()
                    );
                }
                let prepared = workspace.prepare_review(&[args], [4; 16]);
                if expected == "conflict" {
                    let error = prepared.err().unwrap();
                    assert_eq!(error.code, ErrorCode::ReviewConflict);
                    assert_eq!(error.input_index, Some(0));
                    assert!(!error.review_conflicts.is_empty());
                    assert!(!error.to_string().contains(text));
                } else {
                    let mut plan = prepared.unwrap();
                    plan.discard();
                }
                if case["id"] == "review-exact-duplicate-origins" {
                    assert_eq!(result.candidates.len(), 1);
                    assert_eq!(
                        result.candidates[0].sources,
                        [FindingSource::Scanner, FindingSource::User]
                    );
                    assert!(result.candidates[0].finding.needs_review);
                }
            }
        }
        assert_eq!(workspace.export_bundle().unwrap(), before);
    }
}
#[test]
fn legacy_single_and_batch_do_not_allow_crossing_bypass_or_auto_recall() {
    let mut workspace = fresh();
    let text = "abcdef";
    let crossing = [finding(text, "PERSON", 0, 4), finding(text, "CUSTOM", 2, 6)];
    let before = workspace.export_bundle().unwrap();
    let error = workspace.prepare(text, &crossing, [3; 16]).err().unwrap();
    assert_eq!(error.code, ErrorCode::ReviewConflict);
    assert!(!error.review_conflicts.is_empty());
    let inputs = [
        datafog_core::workspace::WorkspaceInput {
            text: "Jane",
            findings: &[finding("Jane", "PERSON", 0, 4)],
        },
        datafog_core::workspace::WorkspaceInput {
            text,
            findings: &crossing,
        },
    ];
    let error = workspace.prepare_batch(&inputs, [3; 16]).err().unwrap();
    assert_eq!(error.code, ErrorCode::ReviewConflict);
    assert_eq!(error.input_index, Some(1));
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let plan = workspace
        .prepare("Jane", &[finding("Jane", "PERSON", 0, 4)], [3; 16])
        .unwrap();
    save(&mut workspace, plan);
    assert_eq!(
        workspace
            .prepare("Jane", &[], [4; 16])
            .unwrap()
            .preview()
            .unwrap()
            .text,
        "Jane"
    );
    let reviewed = workspace
        .prepare_review(&[input("Jane", &[], &[], &[])], [4; 16])
        .unwrap();
    assert!(reviewed.preview().unwrap().text.starts_with("[[PERSON_"));
}
#[test]
fn correction_cannot_shrink_or_relabel_into_waiver_and_keys_bind_head_and_source() {
    let mut workspace = fresh();
    let text = "abcdef";
    let scanner = [finding(text, "PERSON", 0, 4), finding(text, "CUSTOM", 2, 6)];
    let args = input(text, &scanner, &[], &[]);
    let review = workspace.review(args).unwrap();
    let candidate = review
        .candidates
        .iter()
        .find(|c| c.finding.entity_type == "CUSTOM")
        .unwrap();
    let correction = [ReviewCorrection {
        candidate_key: candidate.key.clone(),
        replacement: finding(text, "CUSTOM", 2, 4),
    }];
    let result = workspace
        .review(input(text, &scanner, &[], &correction))
        .unwrap();
    assert!(!result.conflicts.is_empty());
    // Expanding to the whole protected union resolves the crossing spans.
    let correction = [ReviewCorrection {
        candidate_key: candidate.key.clone(),
        replacement: finding(text, "CUSTOM", 0, 6),
    }];
    let result = workspace
        .review(input(text, &scanner, &[], &correction))
        .unwrap();
    assert!(result.conflicts.is_empty());
    assert!(
        workspace
            .prepare_review(&[input(text, &scanner, &[], &correction)], [3; 16])
            .is_ok()
    );
    assert_eq!(
        workspace
            .review(input("abcdef!", &scanner, &[], &correction))
            .err()
            .unwrap()
            .code,
        ErrorCode::WorkspaceConflict
    );
    waiver(&mut workspace, "PRODUCT_CODE", "abcdef", 3);
    assert_eq!(
        workspace
            .review(input(text, &scanner, &[], &correction))
            .err()
            .unwrap()
            .code,
        ErrorCode::WorkspaceConflict
    );
    let current = workspace.review(args).unwrap();
    let current = current
        .candidates
        .iter()
        .find(|c| c.finding.entity_type == "CUSTOM")
        .unwrap();
    let correction = [ReviewCorrection {
        candidate_key: current.key.clone(),
        replacement: finding(text, "PRODUCT_CODE", 0, 6),
    }];
    let result = workspace
        .review(input(text, &scanner, &[], &correction))
        .unwrap();
    assert!(!result.conflicts.is_empty());
}
#[test]
fn recall_scanner_user_union_is_deterministic_and_never_mutates_saved_state() {
    let mut workspace = fresh();
    let original = "Jane Smith";
    let plan = workspace
        .prepare(
            original,
            &[finding(original, "PERSON", 0, original.len())],
            [3; 16],
        )
        .unwrap();
    save(&mut workspace, plan);
    let text = "👋 JANE\tSMITH";
    let mut scanner = finding(text, "PERSON", 5, text.len());
    scanner.detector_name = Some("model".into());
    scanner.detector_version = Some("v1".into());
    scanner.confidence = Some(0.9);
    scanner.source = FindingSource::User;
    let mut user = scanner.clone();
    user.needs_review = false;
    user.source = FindingSource::Scanner;
    let scanner = [scanner];
    let user = [user];
    let args = input(text, &scanner, &user, &[]);
    let before = workspace.export_bundle().unwrap();
    for _ in 0..40 {
        let result = workspace.review(args).unwrap();
        assert!(result.conflicts.is_empty());
        assert_eq!(result.candidates.len(), 1);
        let c = &result.candidates[0];
        assert_eq!(
            c.sources,
            [
                FindingSource::Recall,
                FindingSource::Scanner,
                FindingSource::User
            ]
        );
        assert!(c.finding.needs_review);
        assert_eq!(c.finding.source, FindingSource::User);
        assert_eq!(c.finding.detector_name.as_deref(), Some("model"));
        assert_eq!(c.finding.detector_version.as_deref(), Some("v1"));
        assert_eq!(c.finding.confidence, Some(0.9));
        assert_eq!(c.codepoint_range.start, 2);
        assert_eq!(c.utf16_range.start, 3);
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let plan = workspace.prepare_review(&[args], [4; 16]).unwrap();
    assert_eq!(plan.expected_head().unwrap(), workspace.head().unwrap());
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
#[test]
fn reviewed_batch_is_atomic_and_damaged_markers_fail_before_candidate_creation() {
    let workspace = fresh();
    let text = "abcdef";
    let scanner = [finding(text, "PERSON", 0, 4), finding(text, "CUSTOM", 2, 6)];
    let before = workspace.export_bundle().unwrap();
    for _ in 0..40 {
        let error = workspace
            .prepare_review(
                &[
                    input("Jane", &[finding("Jane", "PERSON", 0, 4)], &[], &[]),
                    input(text, &scanner, &[], &[]),
                ],
                [3; 16],
            )
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::ReviewConflict);
        assert_eq!(error.input_index, Some(1));
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let error = workspace
        .review(input("[[my_note]]", &[], &[], &[]))
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::InvalidMarker);
    assert!(!error.marker_issues.is_empty());
}

#[test]
fn duplicate_targets_invalid_corrections_and_aggregate_limits_reject_explicitly() {
    let workspace = fresh();
    let text = "Jane";
    let scanner = [finding(text, "PERSON", 0, 4)];
    let result = workspace.review(input(text, &scanner, &[], &[])).unwrap();
    let key = &result.candidates[0].key;
    let corrections = [
        ReviewCorrection {
            candidate_key: key.clone(),
            replacement: scanner[0].clone(),
        },
        ReviewCorrection {
            candidate_key: key.clone(),
            replacement: scanner[0].clone(),
        },
    ];
    assert_eq!(
        workspace
            .review(input(text, &scanner, &[], &corrections))
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidFinding
    );
    let invalid = [ReviewCorrection {
        candidate_key: key.clone(),
        replacement: WorkspaceFinding::new("PERSON", "wrong", TextRange { start: 0, end: 4 }),
    }];
    assert_eq!(
        workspace
            .review(input(text, &scanner, &[], &invalid))
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidFinding
    );
    let too_many = vec![scanner[0].clone(); 10_001];
    assert_eq!(
        workspace
            .review(input(text, &too_many, &[], &[]))
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    let many = vec![scanner[0].clone(); 5_001];
    let args = input(text, &many, &[], &[]);
    assert_eq!(
        workspace
            .prepare_review(&[args, args], [3; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    let before = workspace.export_bundle().unwrap();
    assert_eq!(
        workspace
            .prepare_review(&[args; 33], [3; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
