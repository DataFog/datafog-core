use datafog_core::{
    TextRange,
    workspace::{
        ErrorCode, MAX_OPERATION_TIME_MS, OperationContext, OperationKind, OperationOutcome,
        OutcomeKind, PolicyEdits, PreUpgradeHistory, TypeRule, TypeRuleEdit, Workspace,
        WorkspaceFinding, WorkspaceInput,
    },
};
fn operation(id: u8, time: u64) -> OperationContext {
    OperationContext {
        id: [id; 16],
        occurred_at_ms: time,
    }
}
fn activate(workspace: &mut Workspace, plan: &mut datafog_core::workspace::PreparedChange) {
    workspace
        .activate_saved(
            plan,
            plan.expected_head().unwrap(),
            plan.candidate_head().unwrap(),
        )
        .unwrap();
}
#[test]
fn committed_batch_counts_occurrences_and_inputs_without_storing_content() {
    let mut workspace = Workspace::create_with_operation([1; 16], operation(2, 0)).unwrap();
    let finding = |start| {
        WorkspaceFinding::new(
            "PERSON",
            "Jane",
            TextRange {
                start,
                end: start + 4,
            },
        )
    };
    let a = [finding(0), finding(5)];
    let b = [finding(0)];
    let inputs = [
        WorkspaceInput {
            text: "Jane Jane",
            findings: &a,
        },
        WorkspaceInput {
            text: "Jane",
            findings: &b,
        },
    ];
    let before = workspace.export_bundle().unwrap();
    let mut plan = workspace
        .prepare_batch_with_operation(&inputs, operation(3, MAX_OPERATION_TIME_MS))
        .unwrap();
    let candidate = plan.export_candidate_bundle().unwrap().to_vec();
    let previews = plan.previews().unwrap();
    let outputs: Vec<_> = previews
        .iter()
        .map(|preview| preview.text.clone())
        .collect();
    assert_eq!(
        previews[0].text.split(' ').next().unwrap(),
        previews[1].text
    );
    let staged = Workspace::open_bundle(&candidate).unwrap();
    assert_eq!(staged.counts().unwrap().prepared_inputs, 2);
    assert_eq!(staged.counts().unwrap().transformed_spans, 3);
    assert_eq!(workspace.export_bundle().unwrap(), before);
    assert_eq!(workspace.counts().unwrap().operations, 1);
    assert!(workspace.restore(&outputs[0]).is_err());
    assert_eq!(plan.export_candidate_bundle().unwrap(), candidate);
    let mut wrong_head = plan.candidate_head().unwrap();
    wrong_head.commit_id = [9; 16];
    assert_eq!(
        workspace
            .activate_saved(&mut plan, staged.head().unwrap(), wrong_head)
            .unwrap_err()
            .code,
        ErrorCode::WorkspaceConflict
    );
    assert_eq!(workspace.export_bundle().unwrap(), before);
    activate(&mut workspace, &mut plan);
    assert_eq!(workspace.counts().unwrap(), staged.counts().unwrap());
    assert_eq!(workspace.counts().unwrap().operations, 2);
    assert_eq!(
        workspace.operations().unwrap()[1].occurred_at_ms,
        Some(MAX_OPERATION_TIME_MS)
    );
    assert!(!format!("{:?}", workspace.operations().unwrap()).contains("Jane"));
    let reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    assert_eq!(reopened.restore(&outputs[0]).unwrap(), "Jane Jane");
    assert_eq!(reopened.restore(&outputs[1]).unwrap(), "Jane");
}
#[test]
fn explicit_no_change_and_outcomes_are_commits_while_legacy_reads_and_previews_are_not() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let before = workspace.export_bundle().unwrap();
    let mut legacy = workspace.prepare("ordinary words", &[], [3; 16]).unwrap();
    assert_eq!(
        legacy.expected_head().unwrap(),
        legacy.candidate_head().unwrap()
    );
    activate(&mut workspace, &mut legacy);
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let mut timed = workspace
        .prepare_with_operation("", &[], operation(3, 100))
        .unwrap();
    assert_eq!(timed.candidate_head().unwrap().revision, 1);
    timed.preview().unwrap();
    timed.export_candidate_bundle().unwrap();
    assert_eq!(workspace.counts().unwrap().operations, 1);
    activate(&mut workspace, &mut timed);
    assert_eq!(workspace.counts().unwrap().prepared_inputs, 1);
    let mut blocked = workspace
        .prepare_outcome(
            OperationOutcome {
                kind: OutcomeKind::BlockedInput,
                issue_count: 2,
            },
            operation(4, 0),
        )
        .unwrap();
    assert!(blocked.previews().unwrap().is_empty());
    assert_eq!(workspace.counts().unwrap().reported_blocked_attempts, 0);
    activate(&mut workspace, &mut blocked);
    assert_eq!(workspace.counts().unwrap().reported_blocked_attempts, 1);
    assert_eq!(workspace.operations().unwrap()[2].occurred_at_ms, Some(0));
    let mut copied = workspace
        .prepare_outcome(
            OperationOutcome {
                kind: OutcomeKind::CopyConfirmed,
                issue_count: 0,
            },
            operation(5, 1),
        )
        .unwrap();
    activate(&mut workspace, &mut copied);
    assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 0);
    let saved = workspace.export_bundle().unwrap();
    assert!(
        workspace
            .restore_report("[[PERSON_BAD]]")
            .unwrap()
            .text
            .is_none()
    );
    assert_eq!(
        workspace.restore("ordinary words").unwrap(),
        "ordinary words"
    );
    assert_eq!(workspace.export_bundle().unwrap(), saved);
    for id in [2, 3, 4, 5] {
        assert_eq!(
            workspace
                .prepare_with_operation("", &[], operation(id, 0))
                .err()
                .unwrap()
                .code,
            ErrorCode::OperationReused
        );
    }
    assert_eq!(
        workspace.prepare("", &[], [4; 16]).err().unwrap().code,
        ErrorCode::InvalidId
    );
}
#[test]
fn malformed_context_failed_batch_cancel_and_stale_activation_preserve_counts() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let before = workspace.export_bundle().unwrap();
    assert_eq!(
        workspace
            .prepare_with_operation("", &[], operation(3, MAX_OPERATION_TIME_MS + 1))
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidOperation
    );
    for (kind, issue_count) in [
        (OutcomeKind::BlockedInput, 0),
        (OutcomeKind::CopyConfirmed, 1),
        (OutcomeKind::RestoreBlocked, 10001),
    ] {
        assert_eq!(
            workspace
                .prepare_outcome(OperationOutcome { kind, issue_count }, operation(3, 0))
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidOperation
        );
    }
    let invalid = WorkspaceFinding::new("PERSON", "wrong", TextRange { start: 0, end: 4 });
    let inputs = [
        WorkspaceInput {
            text: "ordinary",
            findings: &[],
        },
        WorkspaceInput {
            text: "Jane",
            findings: &[invalid],
        },
    ];
    assert!(
        workspace
            .prepare_batch_with_operation(&inputs, operation(3, 0))
            .is_err()
    );
    let mut cancelled = workspace
        .prepare_outcome(
            OperationOutcome {
                kind: OutcomeKind::UserCancelled,
                issue_count: 0,
            },
            operation(3, 0),
        )
        .unwrap();
    cancelled.discard();
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let mut first = workspace
        .prepare_with_operation("", &[], operation(3, 0))
        .unwrap();
    let mut stale = workspace
        .prepare_with_operation("", &[], operation(4, 0))
        .unwrap();
    activate(&mut workspace, &mut first);
    let saved = workspace.export_bundle().unwrap();
    let parent = stale.expected_head().unwrap();
    let head = stale.candidate_head().unwrap();
    assert_eq!(
        workspace
            .activate_saved(&mut stale, parent, head)
            .unwrap_err()
            .code,
        ErrorCode::WorkspaceConflict
    );
    assert_eq!(workspace.export_bundle().unwrap(), saved);
    let mut recovery = workspace.prepare_recovery([5; 16]).unwrap();
    activate(&mut workspace, &mut recovery);
    assert_eq!(
        workspace.operations().unwrap().last().unwrap().kind,
        OperationKind::Recovery
    );
    assert_eq!(
        workspace
            .operations()
            .unwrap()
            .last()
            .unwrap()
            .occurred_at_ms,
        None
    );
}
#[test]
fn legacy_snapshot_bytes_and_unknown_history_remain_unchanged_until_saved_upgrade() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    let hex = fixtures["high"]["bundleHex"].as_str().unwrap();
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let mut workspace = Workspace::open_bundle(&bytes).unwrap();
    assert_eq!(workspace.counts().unwrap().operations, 0);
    assert!(workspace.operations().unwrap().is_empty());
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    let mut recovery = workspace
        .prepare_recovery_with_operation(operation(7, 0))
        .unwrap();
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    let upgraded = Workspace::open_bundle(recovery.export_candidate_bundle().unwrap()).unwrap();
    assert_eq!(upgraded.counts().unwrap().operations, 1);
    assert_eq!(
        upgraded.operations().unwrap()[0].kind,
        OperationKind::Recovery
    );
    activate(&mut workspace, &mut recovery);
    assert_eq!(workspace.counts().unwrap(), upgraded.counts().unwrap());
    assert_eq!(
        workspace
            .restore(fixtures["high"]["marker"].as_str().unwrap())
            .unwrap(),
        "Jane"
    );
}

#[test]
fn policy_v2_bytes_noops_discard_and_failed_activation_are_preserved_before_v3_upgrade() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/policy-v2.json")).unwrap();
    let bytes: Vec<u8> = fixture["bundleHex"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let mut workspace = Workspace::open_bundle(&bytes).unwrap();
    assert_eq!(workspace.policy().unwrap().snapshot_version, 2);
    assert!(workspace.operations().unwrap().is_empty());
    assert_eq!(workspace.counts().unwrap().operations, 0);
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    let no_op = workspace.prepare("ordinary", &[], [5; 16]).unwrap();
    assert_eq!(no_op.export_candidate_bundle().unwrap(), bytes);
    assert_eq!(no_op.snapshot_format().unwrap().candidate_version, 2);
    let mut staged = workspace
        .prepare_with_operation("ordinary", &[], operation(5, 0))
        .unwrap();
    assert_eq!(staged.snapshot_format().unwrap().current_version, 2);
    assert_eq!(staged.snapshot_format().unwrap().candidate_version, 3);
    let before = staged.expected_head().unwrap();
    assert_eq!(
        workspace
            .activate_saved(&mut staged, before, before)
            .unwrap_err()
            .code,
        ErrorCode::WorkspaceConflict
    );
    staged.discard();
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    let mut staged = workspace
        .prepare_with_operation("ordinary", &[], operation(5, 0))
        .unwrap();
    let upgraded = Workspace::open_bundle(staged.export_candidate_bundle().unwrap()).unwrap();
    assert_eq!(upgraded.policy().unwrap().snapshot_version, 3);
    assert_eq!(upgraded.counts().unwrap().operations, 1);
    assert_eq!(
        upgraded.operations().unwrap()[0].kind,
        OperationKind::Prepare
    );
    assert_eq!(
        upgraded
            .restore(fixture["marker"].as_str().unwrap())
            .unwrap(),
        "Jane"
    );
    assert_eq!(upgraded.policy().unwrap().choices[0].value, "May");
    assert_eq!(
        upgraded.policy().unwrap().choices[0].disposition,
        datafog_core::workspace::ChoiceDisposition::NotSensitive
    );
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    activate(&mut workspace, &mut staged);
    assert_eq!(
        workspace.export_bundle().unwrap(),
        upgraded.export_bundle().unwrap()
    );
}

#[test]
fn coverage_never_invents_unknown_prior_history_or_changes_legacy_bytes() {
    let mut created = Workspace::create([1; 16], [2; 16]).unwrap();
    let coverage = created.history_coverage().unwrap();
    assert!(coverage.recorded_history_only);
    assert_eq!(
        coverage.pre_upgrade_history,
        PreUpgradeHistory::NotApplicable
    );
    created.close();
    assert_eq!(
        created.history_coverage().err().unwrap().code,
        ErrorCode::ClosedWorkspace
    );
    for fixture in [
        include_str!("../../../fixtures/workspace/policy-v2.json"),
        include_str!("../../../fixtures/workspace/counters.json"),
    ] {
        let data: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let value = if data.get("bundleHex").is_some() {
            &data
        } else {
            &data["high"]
        };
        let hex = value["bundleHex"].as_str().unwrap();
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let mut legacy = Workspace::open_bundle(&bytes).unwrap();
        assert!(legacy.history_coverage().unwrap().recorded_history_only);
        assert_eq!(
            legacy.history_coverage().unwrap().pre_upgrade_history,
            PreUpgradeHistory::Unknown
        );
        assert_eq!(legacy.counts().unwrap().operations, 0);
        assert_eq!(legacy.export_bundle().unwrap(), bytes);
        // A staged recovery records only its real new row; discarding preserves bytes.
        if legacy.head().unwrap().revision != u64::MAX {
            let mut plan = legacy.prepare_recovery([210; 16]).unwrap();
            let candidate =
                Workspace::open_bundle(plan.export_candidate_bundle().unwrap()).unwrap();
            assert_eq!(
                candidate.history_coverage().unwrap().pre_upgrade_history,
                PreUpgradeHistory::Unknown
            );
            assert_eq!(candidate.counts().unwrap().operations, 1);
            plan.discard();
            assert_eq!(legacy.export_bundle().unwrap(), bytes);
        }
        legacy.close();
    }
}

#[test]
fn paged_history_is_lossless_borrowed_and_read_only() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    for id in 3..7 {
        let mut plan = workspace
            .prepare_with_operation("", &[], operation(id, 0))
            .unwrap();
        activate(&mut workspace, &mut plan);
    }
    let before = workspace.export_bundle().unwrap();
    let all = workspace.operations().unwrap();
    let first = workspace.operations_page(0, 2).unwrap();
    assert_eq!(first.records, &all[..2]);
    assert_eq!(first.records.as_ptr(), all.as_ptr());
    assert_eq!(
        (first.total, first.offset, first.next_offset),
        (5, 0, Some(2))
    );
    let second = workspace.operations_page(2, 2).unwrap();
    assert_eq!(second.records, &all[2..4]);
    assert_eq!(second.next_offset, Some(4));
    let last = workspace.operations_page(4, 2).unwrap();
    assert_eq!(last.records, &all[4..]);
    assert_eq!(last.next_offset, None);
    let end = workspace.operations_page(5, 1).unwrap();
    assert!(end.records.is_empty());
    assert_eq!((end.total, end.offset, end.next_offset), (5, 5, None));
    for (offset, limit) in [
        (6, 1),
        (0, 0),
        (0, 100_001),
        (usize::MAX, 1),
        (0, usize::MAX),
    ] {
        assert_eq!(
            workspace.operations_page(offset, limit).unwrap_err().code,
            ErrorCode::ResourceLimit
        );
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    workspace.close();
    assert_eq!(
        workspace.operations_page(0, 1).unwrap_err().code,
        ErrorCode::ClosedWorkspace
    );
}

#[test]
fn every_recognized_reused_id_preserves_legacy_and_explicit_error_contracts() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let mut mapping = workspace
        .prepare(
            "Jane",
            &[WorkspaceFinding::new(
                "PERSON",
                "Jane",
                TextRange { start: 0, end: 4 },
            )],
            [3; 16],
        )
        .unwrap();
    activate(&mut workspace, &mut mapping);
    let edits = PolicyEdits {
        rules: vec![TypeRuleEdit {
            entity_type: "PROJECT_CODE".into(),
            rule: Some(TypeRule::Keep),
        }],
        choices: vec![],
    };
    let mut settings = workspace.prepare_policy_changes(&edits, [4; 16]).unwrap();
    activate(&mut workspace, &mut settings);
    let mut history = workspace
        .prepare_outcome(
            OperationOutcome {
                kind: OutcomeKind::UserCancelled,
                issue_count: 0,
            },
            operation(5, 0),
        )
        .unwrap();
    activate(&mut workspace, &mut history);
    let mut current = workspace.prepare_recovery([6; 16]).unwrap();
    activate(&mut workspace, &mut current);
    let saved = workspace.export_bundle().unwrap();
    let workspace = Workspace::open_bundle(&saved).unwrap();
    let head = workspace.head().unwrap();
    let inputs = [WorkspaceInput {
        text: "ordinary",
        findings: &[],
    }];
    // Current head, mapping issuance, settings, history-only create and outcome.
    for id in [6, 3, 4, 2, 5] {
        for result in [
            workspace.prepare("", &[], [id; 16]),
            workspace.prepare_batch(&inputs, [id; 16]),
            workspace.prepare_recovery([id; 16]),
            workspace.prepare_policy_changes(
                &PolicyEdits {
                    rules: vec![],
                    choices: vec![],
                },
                [id; 16],
            ),
        ] {
            assert_eq!(
                result.err().unwrap().code,
                ErrorCode::InvalidId,
                "legacy ID {id}"
            );
        }
        for result in [
            workspace.prepare_with_operation("", &[], operation(id, 0)),
            workspace.prepare_batch_with_operation(&inputs, operation(id, 0)),
            workspace.prepare_recovery_with_operation(operation(id, 0)),
            workspace.prepare_outcome(
                OperationOutcome {
                    kind: OutcomeKind::UserCancelled,
                    issue_count: 0,
                },
                operation(id, 0),
            ),
        ] {
            assert_eq!(
                result.err().unwrap().code,
                ErrorCode::OperationReused,
                "explicit ID {id}"
            );
        }
        assert_eq!(workspace.head().unwrap(), head);
        assert_eq!(workspace.export_bundle().unwrap(), saved);
    }
}
