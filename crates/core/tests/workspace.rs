use datafog_core::{
    TextRange,
    workspace::{ErrorCode, Workspace, WorkspaceFinding, WorkspaceInput, byte_range_from_utf16},
};

fn saved_person(namespace: [u8; 16]) -> (Workspace, String) {
    let mut workspace = Workspace::create(namespace, [2; 16]).unwrap();
    let finding = WorkspaceFinding::new("PERSON", "Jane", TextRange { start: 0, end: 4 });
    let mut pending = workspace.prepare("Jane", &[finding], [3; 16]).unwrap();
    let marker = pending.preview().unwrap().text.clone();
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    (workspace, marker)
}

#[test]
fn recovery_stages_a_fresh_head_and_preserves_known_markers() {
    let (workspace, jane) = saved_person([1; 16]);
    let backup = workspace.export_bundle().unwrap();
    let john = WorkspaceFinding::new("PERSON", "John", TextRange { start: 0, end: 4 });
    let later = workspace.prepare("John", &[john], [4; 16]).unwrap();
    let missing_marker = later.preview().unwrap().text.clone();
    let mut recovered = Workspace::open_bundle(&backup).unwrap();
    let mut recovery = recovered.prepare_recovery([5; 16]).unwrap();
    assert!(recovery.previews().unwrap().is_empty());
    assert_eq!(recovered.export_bundle().unwrap(), backup);
    let parent = recovery.expected_head().unwrap();
    let candidate = recovery.candidate_head().unwrap();
    assert_eq!(candidate.revision, parent.revision + 1);
    assert_eq!(candidate.commit_id, [5; 16]);
    let saved = recovery.export_candidate_bundle().unwrap().to_vec();
    recovered
        .activate_saved(&mut recovery, parent, candidate)
        .unwrap();
    assert_eq!(
        recovery.candidate_head().unwrap_err().code,
        ErrorCode::InactivePlan
    );
    assert_eq!(recovered.restore(&jane).unwrap(), "Jane");
    let reopened = Workspace::open_bundle(&saved).unwrap();
    let reuse = reopened
        .prepare(
            "Jane",
            &[WorkspaceFinding::new(
                "PERSON",
                "Jane",
                TextRange { start: 0, end: 4 },
            )],
            [6; 16],
        )
        .unwrap();
    assert_eq!(reuse.preview().unwrap().text, jane);
    let carol = reopened
        .prepare(
            "Carol",
            &[WorkspaceFinding::new(
                "PERSON",
                "Carol",
                TextRange { start: 0, end: 5 },
            )],
            [7; 16],
        )
        .unwrap();
    let new_marker = &carol.preview().unwrap().text;
    assert!(new_marker.ends_with(":PERSON:2]]"));
    assert_ne!(new_marker, &missing_marker);
    assert_eq!(
        reopened.restore(&missing_marker).unwrap_err().code,
        ErrorCode::InvalidMarker
    );
    for used in [[3; 16], [5; 16]] {
        assert_eq!(
            reopened.prepare_recovery(used).err().unwrap().code,
            ErrorCode::InvalidId
        );
    }
    let mut discarded = reopened.prepare_recovery([8; 16]).unwrap();
    discarded.discard();
    assert_eq!(
        discarded.export_candidate_bundle().unwrap_err().code,
        ErrorCode::InactivePlan
    );
    assert_eq!(reopened.export_bundle().unwrap(), saved);
}

#[test]
fn committed_markers_pass_through_followups_after_reopening() {
    let (workspace, marker) = saved_person([1; 16]);
    let saved = workspace.export_bundle().unwrap();
    let mut reopened = Workspace::open_bundle(&saved).unwrap();
    let source = format!("👋 e\u{301} {marker} vs John, then {marker}.");
    let start = source.find("John").unwrap();
    let range = TextRange {
        start,
        end: start + 4,
    };
    let finding = WorkspaceFinding::new("PERSON", "John", range);
    let mut pending = reopened.prepare(&source, &[finding], [4; 16]).unwrap();
    let preview = pending.preview().unwrap();
    assert_eq!(preview.text.matches(&marker).count(), 2);
    assert_eq!(preview.transformations.len(), 1);
    let transformation = &preview.transformations[0];
    assert_eq!(transformation.source_byte_range, range);
    assert_eq!(
        transformation.source_codepoint_range.start,
        source[..start].chars().count()
    );
    assert_eq!(
        transformation.source_utf16_range.start,
        source[..start].encode_utf16().count()
    );
    let replacement =
        &preview.text[transformation.output_byte_range.start..transformation.output_byte_range.end];
    assert_eq!(preview.text, source.replace("John", replacement));
    assert!(replacement.ends_with(":PERSON:2]]"));
    let output = preview.text.clone();
    assert_eq!(reopened.export_bundle().unwrap(), saved);
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    reopened
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    assert_eq!(
        reopened.restore(&output).unwrap(),
        source.replace(&marker, "Jane")
    );
    let saved = reopened.export_bundle().unwrap();
    let next = Workspace::open_bundle(&saved).unwrap();
    let unchanged = next.prepare(&output, &[], [5; 16]).unwrap();
    assert_eq!(unchanged.preview().unwrap().text, output);
    assert!(unchanged.preview().unwrap().transformations.is_empty());
    assert_eq!(
        unchanged.expected_head().unwrap(),
        unchanged.candidate_head().unwrap()
    );
    assert_eq!(unchanged.export_candidate_bundle().unwrap(), saved);
}

#[test]
fn findings_cannot_replace_or_cross_existing_markers() {
    let (workspace, marker) = saved_person([1; 16]);
    let saved = workspace.export_bundle().unwrap();
    let source = format!("John{marker}Jane");
    let end = 4 + marker.len();
    let outside = WorkspaceFinding::new("PERSON", "John", TextRange { start: 0, end: 4 });
    for range in [
        TextRange { start: 4, end },
        TextRange { start: 5, end: 6 },
        TextRange { start: 3, end: 5 },
        TextRange {
            start: end - 1,
            end: end + 1,
        },
        TextRange {
            start: 0,
            end: source.len(),
        },
    ] {
        let inside = WorkspaceFinding::new("PERSON", &source[range.start..range.end], range);
        let failure = workspace
            .prepare(&source, &[outside.clone(), inside], [4; 16])
            .err()
            .unwrap();
        assert_eq!(failure.code, ErrorCode::InvalidFinding);
        assert_eq!(failure.input_index, Some(0));
        assert_eq!(failure.finding_index, Some(1));
        assert_eq!(failure.to_string(), "invalid_finding");
        assert_eq!(workspace.export_bundle().unwrap(), saved);
    }
    let right = WorkspaceFinding::new(
        "PERSON",
        "Jane",
        TextRange {
            start: end,
            end: source.len(),
        },
    );
    let pending = workspace
        .prepare(&source, &[outside, right], [4; 16])
        .unwrap();
    let preview = pending.preview().unwrap();
    assert_eq!(preview.transformations.len(), 2);
    assert_eq!(preview.text.matches(&marker).count(), 2);
    assert!(preview.text.ends_with(&format!("{marker}{marker}")));
    let first = &preview.transformations[0];
    assert!(
        preview.text[first.output_byte_range.start..first.output_byte_range.end]
            .ends_with(":PERSON:2]]")
    );
}

#[test]
fn unknown_markers_reject_batches_without_admitting_pending_aliases() {
    let (workspace, marker) = saved_person([1; 16]);
    let (_, foreign) = saved_person([9; 16]);
    let saved = workspace.export_bundle().unwrap();
    let finding = WorkspaceFinding::new("PERSON", "John", TextRange { start: 0, end: 4 });
    let mut pending = workspace
        .prepare("John", &[finding.clone()], [4; 16])
        .unwrap();
    let uncommitted = pending.preview().unwrap().text.clone();
    for bad in [
        foreign,
        uncommitted.clone(),
        marker.replace("DFP1", "DFP2"),
        marker.replace(":PERSON:", ":OTHER:"),
        marker.replace(":PERSON:1]]", ":PERSON:2]]"),
        marker[..marker.len() - 1].to_owned(),
        "[[DFP1:unknown]]".to_owned(),
        "[[DFP".to_owned(),
    ] {
        let source = format!("{marker} then {bad}");
        let inputs = [
            WorkspaceInput {
                text: "John",
                findings: std::slice::from_ref(&finding),
            },
            WorkspaceInput {
                text: &source,
                findings: &[],
            },
        ];
        // The first member allocates the same candidate alias as `pending`;
        // validation must still use only the workspace's committed registry.
        let failure = workspace.prepare_batch(&inputs, [4; 16]).err().unwrap();
        assert_eq!(failure.code, ErrorCode::InvalidMarker);
        assert_eq!(failure.input_index, Some(1));
        assert_eq!(failure.finding_index, None);
        assert_eq!(failure.to_string(), "invalid_marker");
        assert_eq!(workspace.export_bundle().unwrap(), saved);
    }
    pending.discard();
    assert_eq!(
        workspace
            .prepare(&uncommitted, &[], [5; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidMarker
    );
}

#[test]
fn a_batch_shares_aliases_and_commits_one_snapshot_after_approval() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let original = workspace.export_bundle().unwrap();
    let findings = [WorkspaceFinding::new(
        "PERSON",
        "Jane",
        TextRange { start: 0, end: 4 },
    )];
    let inputs = [
        WorkspaceInput {
            text: "Jane | -10 USD",
            findings: &findings,
        },
        WorkspaceInput {
            text: "Jane | -20 USD",
            findings: &findings,
        },
    ];
    let mut pending = workspace.prepare_batch(&inputs, [3; 16]).unwrap();
    let previews = pending.previews().unwrap();
    assert_eq!(previews.len(), 2);
    let first = previews[0].text.clone();
    let second = previews[1].text.clone();
    assert_eq!(
        first.strip_suffix(" | -10 USD"),
        second.strip_suffix(" | -20 USD")
    );
    assert_eq!(workspace.export_bundle().unwrap(), original);
    assert_eq!(pending.candidate_head().unwrap().revision, 1);
    assert!(pending.preview().is_err());
    let saved = pending.export_candidate_bundle().unwrap().to_vec();
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    assert!(pending.previews().is_err());
    let reopened = Workspace::open_bundle(&saved).unwrap();
    assert_eq!(reopened.restore(&first).unwrap(), inputs[0].text);
    assert_eq!(reopened.restore(&second).unwrap(), inputs[1].text);
    let again = reopened.prepare_batch(&inputs, [4; 16]).unwrap();
    assert_eq!(again.previews().unwrap()[0].text, first);
    assert_eq!(again.previews().unwrap()[1].text, second);
    assert_eq!(again.candidate_head().unwrap(), candidate);
}

#[test]
fn rejected_batch_members_preserve_committed_state_and_report_local_indexes() {
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let original = workspace.export_bundle().unwrap();
    let valid = [WorkspaceFinding::new(
        "PERSON",
        "Jane",
        TextRange { start: 0, end: 4 },
    )];
    let invalid = [WorkspaceFinding::new(
        "PERSON",
        "wrong",
        TextRange { start: 5, end: 9 },
    )];
    let inputs = [
        WorkspaceInput {
            text: "Jane",
            findings: &valid,
        },
        WorkspaceInput {
            text: "👋 Jane",
            findings: &invalid,
        },
    ];
    let failure = workspace.prepare_batch(&inputs, [3; 16]).err().unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidFinding);
    assert_eq!(failure.input_index, Some(1));
    assert_eq!(failure.finding_index, Some(0));
    assert_eq!(workspace.export_bundle().unwrap(), original);
    let collision = [
        inputs[0],
        WorkspaceInput {
            text: "[[DFP1:literal",
            findings: &[],
        },
    ];
    let failure = workspace.prepare_batch(&collision, [3; 16]).err().unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidMarker);
    assert_eq!(failure.input_index, Some(1));
    assert_eq!(workspace.export_bundle().unwrap(), original);
    let reduced = workspace.prepare_batch(&inputs[..1], [4; 16]).unwrap();
    assert!(reduced.preview().unwrap().text.ends_with(":PERSON:1]]"));
}

#[test]
fn batch_limits_are_aggregate_and_ranges_remain_local_to_each_input() {
    use datafog_core::workspace::{MAX_FINDINGS, MAX_INPUTS, MAX_SOURCE_BYTES};
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let first = [WorkspaceFinding::new(
        "PERSON",
        "Jane",
        TextRange { start: 0, end: 4 },
    )];
    let second = [WorkspaceFinding::new(
        "PERSON",
        "John",
        TextRange { start: 5, end: 9 },
    )];
    let inputs = [
        WorkspaceInput {
            text: "Jane",
            findings: &first,
        },
        WorkspaceInput {
            text: "👋 John",
            findings: &second,
        },
    ];
    let mut pending = workspace.prepare_batch(&inputs, [3; 16]).unwrap();
    let previews = pending.previews().unwrap();
    assert!(previews[0].text.ends_with(":PERSON:1]]"));
    assert!(previews[1].text.ends_with(":PERSON:2]]"));
    assert_eq!(
        previews[1].transformations[0].source_utf16_range,
        TextRange { start: 3, end: 7 }
    );
    assert_eq!(
        previews[1].transformations[0].source_byte_range,
        TextRange { start: 5, end: 9 }
    );
    pending.discard();
    assert!(pending.previews().is_err());
    assert_eq!(
        workspace.prepare_batch(&[], [3; 16]).err().unwrap().code,
        ErrorCode::InvalidBatch
    );
    let too_many: Vec<_> = (0..=MAX_INPUTS)
        .map(|_| WorkspaceInput {
            text: "",
            findings: &[],
        })
        .collect();
    assert_eq!(
        workspace
            .prepare_batch(&too_many, [3; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    let text = "x".repeat(MAX_SOURCE_BYTES / 2 + 1);
    let too_large = [
        WorkspaceInput {
            text: &text,
            findings: &[],
        },
        WorkspaceInput {
            text: &text,
            findings: &[],
        },
    ];
    let failure = workspace.prepare_batch(&too_large, [3; 16]).err().unwrap();
    assert_eq!(failure.code, ErrorCode::ResourceLimit);
    assert_eq!(failure.input_index, Some(1));
    let repeated = vec![first[0].clone(); MAX_FINDINGS / 2 + 1];
    let too_many_findings = [
        WorkspaceInput {
            text: "Jane",
            findings: &repeated,
        },
        WorkspaceInput {
            text: "Jane",
            findings: &repeated,
        },
    ];
    assert_eq!(
        workspace
            .prepare_batch(&too_many_findings, [3; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
}

#[test]
fn approved_workspace_reopens_with_identical_aliases_and_exact_restoration() {
    let source = "2026-01-06 | Jane Smith | -40.00 USD";
    let finding = WorkspaceFinding::new("PERSON", "Jane Smith", TextRange { start: 13, end: 23 });
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let original_bundle = workspace.export_bundle().unwrap();
    let mut pending = workspace
        .prepare(source, &[finding.clone()], [3; 16])
        .unwrap();
    let preview = pending.preview().unwrap().text.clone();
    assert!(preview.starts_with("2026-01-06 | [[DFP1:"));
    assert!(preview.ends_with(" | -40.00 USD"));
    assert_eq!(workspace.export_bundle().unwrap(), original_bundle);
    let saved = pending.export_candidate_bundle().unwrap().to_vec();
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    assert_eq!(workspace.restore(&preview).unwrap(), source);
    assert!(pending.preview().is_err());

    let reopened = Workspace::open_bundle(&saved).unwrap();
    let next = reopened.prepare(source, &[finding], [4; 16]).unwrap();
    assert_eq!(next.preview().unwrap().text, preview);
    assert_eq!(reopened.restore(&preview).unwrap(), source);
    assert_eq!(reopened.export_bundle().unwrap(), saved);
}

#[test]
fn invalid_findings_and_discarded_previews_do_not_change_committed_mappings() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let original = workspace.export_bundle().unwrap();
    for (label, matched, range) in [
        ("PERSON", "wrong", TextRange { start: 5, end: 9 }),
        ("bad:type", "Jane", TextRange { start: 5, end: 9 }),
        ("PERSON", "Jane", TextRange { start: 1, end: 9 }),
    ] {
        let finding = WorkspaceFinding::new(label, matched, range);
        let error = workspace
            .prepare("👋 Jane", &[finding], [3; 16])
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::InvalidFinding);
        assert_eq!(error.finding_index, Some(0));
        assert_eq!(workspace.export_bundle().unwrap(), original);
    }
    let finding = WorkspaceFinding::new("Client_Name", "Jane", TextRange { start: 5, end: 9 });
    let mut pending = workspace.prepare("👋 Jane", &[finding], [3; 16]).unwrap();
    pending.discard();
    assert!(pending.export_candidate_bundle().is_err());
    assert!(
        workspace
            .activate_saved(
                &mut pending,
                workspace.head().unwrap(),
                workspace.head().unwrap()
            )
            .is_err()
    );
    assert_eq!(workspace.export_bundle().unwrap(), original);
}

#[test]
fn browser_selections_and_preview_ranges_preserve_unicode_coordinate_semantics() {
    let source = "👋 e\u{301} Jane";
    assert_eq!(
        byte_range_from_utf16(source, TextRange { start: 6, end: 10 }).unwrap(),
        TextRange { start: 9, end: 13 }
    );
    assert!(byte_range_from_utf16(source, TextRange { start: 1, end: 10 }).is_err());
    assert!(
        byte_range_from_utf16(
            source,
            TextRange {
                start: 0,
                end: usize::MAX
            }
        )
        .is_err()
    );
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let finding = WorkspaceFinding::new("Client_Name", "Jane", TextRange { start: 9, end: 13 });
    let pending = workspace.prepare(source, &[finding], [3; 16]).unwrap();
    let preview = pending.preview().unwrap();
    let span = &preview.transformations[0];
    assert_eq!(span.source_utf16_range, TextRange { start: 6, end: 10 });
    assert_eq!(span.source_codepoint_range, TextRange { start: 5, end: 9 });
    assert_eq!(
        &preview.text[span.output_byte_range.start..span.output_byte_range.end],
        preview.text.strip_prefix("👋 e\u{301} ").unwrap()
    );
}

#[test]
fn committed_markers_restore_atomically_and_pending_changes_are_single_use() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let finding = WorkspaceFinding::new("PERSON", "Jane", TextRange { start: 0, end: 4 });
    let mut pending = workspace
        .prepare("Jane", &[finding.clone()], [3; 16])
        .unwrap();
    let marker = pending.preview().unwrap().text.clone();
    assert!(workspace.restore(&marker).is_err());
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    assert!(
        workspace
            .activate_saved(&mut pending, parent, candidate)
            .is_err()
    );
    assert_eq!(workspace.restore(&marker).unwrap(), "Jane");
    for unknown in [
        marker.replace("DFP1", "DFP2"),
        marker.replace(":PERSON:", ":OTHER:"),
        "[[DFP1:damaged".to_owned(),
    ] {
        assert_eq!(
            workspace
                .restore(&format!("{marker} {unknown}"))
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidMarker
        );
    }
    let passthrough = workspace.prepare(&marker, &[], [4; 16]).unwrap();
    assert_eq!(passthrough.preview().unwrap().text, marker);
    let mut no_change = workspace.prepare("Jane", &[finding], [4; 16]).unwrap();
    assert_eq!(
        no_change.expected_head().unwrap(),
        no_change.candidate_head().unwrap()
    );
    let head = workspace.head().unwrap();
    workspace
        .activate_saved(&mut no_change, head, head)
        .unwrap();
    assert!(no_change.preview().is_err());
}

#[test]
fn stale_and_foreign_runtime_plans_cannot_replace_current_state() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let first = WorkspaceFinding::new("PERSON", "Jane", TextRange { start: 0, end: 4 });
    let mut winner = workspace.prepare("Jane", &[first], [3; 16]).unwrap();
    let second = WorkspaceFinding::new("PERSON", "John", TextRange { start: 0, end: 4 });
    let mut stale = workspace.prepare("John", &[second], [4; 16]).unwrap();
    let mut other_runtime = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    let parent = winner.expected_head().unwrap();
    let next = winner.candidate_head().unwrap();
    assert!(
        other_runtime
            .activate_saved(&mut winner, parent, next)
            .is_err()
    );
    workspace.activate_saved(&mut winner, parent, next).unwrap();
    let stale_parent = stale.expected_head().unwrap();
    let stale_next = stale.candidate_head().unwrap();
    assert!(
        workspace
            .activate_saved(&mut stale, stale_parent, stale_next)
            .is_err()
    );
    workspace.close();
    assert!(workspace.export_bundle().is_err());
}

#[test]
fn snapshot_decoder_rejects_noncanonical_truncated_and_duplicate_fields() {
    let bytes = Workspace::create([1; 16], [2; 16])
        .unwrap()
        .export_bundle()
        .unwrap();
    for index in 0..bytes.len() {
        assert!(Workspace::open_bundle(&bytes[..index]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Workspace::open_bundle(&trailing).is_err());
    let mut nonminimal = bytes.clone();
    nonminimal.splice(1..2, [0x18, 0x00]);
    assert!(Workspace::open_bundle(&nonminimal).is_err());
    let mut duplicate = bytes.clone();
    duplicate[3] = 0;
    assert!(Workspace::open_bundle(&duplicate).is_err());
    let mut unsupported = bytes.clone();
    unsupported[2] = 2;
    assert!(Workspace::open_bundle(&unsupported).is_err());
    let mut allocation_attack = vec![0xbf];
    allocation_attack.extend_from_slice(&bytes[1..]);
    assert!(Workspace::open_bundle(&allocation_attack).is_err());
}

#[test]
fn versioned_counter_fixtures_preserve_uint64_values_and_fail_closed_on_overflow() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    for name in ["high", "maximum"] {
        let fixture = &fixtures[name];
        let hex = fixture["bundleHex"].as_str().unwrap();
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        let workspace = Workspace::open_bundle(&bytes).unwrap();
        let revision = fixture["revision"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert_eq!(workspace.head().unwrap().revision, revision);
        assert_eq!(workspace.export_bundle().unwrap(), bytes);
        assert_eq!(
            workspace
                .restore(fixture["marker"].as_str().unwrap())
                .unwrap(),
            "Jane"
        );
        let finding = WorkspaceFinding::new("PERSON", "John", TextRange { start: 0, end: 4 });
        let next = workspace.prepare("John", &[finding], [4; 16]);
        let recovery = workspace.prepare_recovery([5; 16]);
        if name == "maximum" {
            assert_eq!(next.err().unwrap().code, ErrorCode::ResourceLimit);
            assert_eq!(recovery.err().unwrap().code, ErrorCode::ResourceLimit);
        } else {
            assert_eq!(
                next.unwrap().candidate_head().unwrap().revision,
                revision + 1
            );
            assert_eq!(
                recovery.unwrap().candidate_head().unwrap().revision,
                revision + 1
            );
        }
        assert_eq!(workspace.export_bundle().unwrap(), bytes);
    }
}

#[test]
fn closing_a_workspace_invalidates_outstanding_preview_and_export_handles() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let pending = workspace.prepare("plain", &[], [3; 16]).unwrap();
    workspace.close();
    assert!(pending.preview().is_err());
    assert!(pending.export_candidate_bundle().is_err());
}
