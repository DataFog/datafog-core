use datafog_core::{
    TextRange,
    workspace::{
        ErrorCode, MarkerIssueKind, Workspace, WorkspaceFinding, WorkspaceInput,
        byte_range_from_utf16,
    },
};

/// `[[LABEL_CODE]]` with a six-character canonical Crockford base32 code.
fn is_marker(text: &str, label: &str) -> bool {
    text.strip_prefix(&format!("[[{label}_"))
        .and_then(|rest| rest.strip_suffix("]]"))
        .is_some_and(|code| {
            code.len() == 6
                && code
                    .bytes()
                    .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
        })
}

/// Prepare and activate one whole-text finding, returning the issued marker.
fn commit(workspace: &mut Workspace, label: &str, value: &str, id: [u8; 16]) -> String {
    let finding = WorkspaceFinding::new(
        label,
        value,
        TextRange {
            start: 0,
            end: value.len(),
        },
    );
    let mut pending = workspace.prepare(value, &[finding], id).unwrap();
    let marker = pending.preview().unwrap().text.clone();
    let parent = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, parent, candidate)
        .unwrap();
    marker
}

fn saved_person(namespace: [u8; 16]) -> (Workspace, String) {
    let mut workspace = Workspace::create(namespace, [2; 16]).unwrap();
    let marker = commit(&mut workspace, "PERSON", "Jane", [3; 16]);
    (workspace, marker)
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
    assert!(is_marker(replacement, "PERSON") && replacement != marker);
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
    let first_marker = &preview.text[first.output_byte_range.start..first.output_byte_range.end];
    assert!(is_marker(first_marker, "PERSON") && first_marker != marker);
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
    let code = &marker[9..15];
    let other_code = if code == "000000" { "000001" } else { "000000" };
    for bad in [
        foreign,
        uncommitted.clone(),
        marker.replace("PERSON", "OTHER"),
        marker.replace(code, other_code),
        "[[PERSON_ZZZZZZ]]".to_owned(),
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
fn marker_issued_in_same_batch_is_rejected_with_expected_error() {
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let saved = workspace.export_bundle().unwrap();
    let finding = WorkspaceFinding::new("PERSON", "Alice", TextRange { start: 0, end: 5 });
    let op_id = [3; 16];
    let input1 = WorkspaceInput {
        text: "Alice",
        findings: std::slice::from_ref(&finding),
    };
    let mut plan = workspace.prepare_batch(&[input1], op_id).unwrap();
    let marker_from_first = plan.preview().unwrap().text.clone();
    let text_with_new_marker = format!("Then {marker_from_first}");
    plan.discard();
    let input1_again = WorkspaceInput {
        text: "Alice",
        findings: std::slice::from_ref(&finding),
    };
    let input2 = WorkspaceInput {
        text: &text_with_new_marker,
        findings: &[],
    };
    let error = workspace
        .prepare_batch(&[input1_again, input2], op_id)
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::InvalidMarker);
    assert_eq!(error.input_index, Some(1));
    assert_eq!(error.finding_index, None);
    assert_eq!(workspace.export_bundle().unwrap(), saved);
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
            text: "a literal [[PERSON_ZZZZZZ]] collision",
            findings: &[],
        },
    ];
    let failure = workspace.prepare_batch(&collision, [3; 16]).err().unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidMarker);
    assert_eq!(failure.input_index, Some(1));
    assert_eq!(workspace.export_bundle().unwrap(), original);
    let reduced = workspace.prepare_batch(&inputs[..1], [4; 16]).unwrap();
    assert!(is_marker(&reduced.preview().unwrap().text, "PERSON"));
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
    assert!(is_marker(&previews[0].text, "PERSON"));
    let john = previews[1].text.strip_prefix("👋 ").unwrap();
    assert!(is_marker(john, "PERSON") && john != previews[0].text);
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
    let marker = preview
        .strip_prefix("2026-01-06 | ")
        .and_then(|rest| rest.strip_suffix(" | -40.00 USD"))
        .unwrap();
    assert!(is_marker(marker, "PERSON"));
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
        ("Client_Name", "Jane", TextRange { start: 5, end: 9 }),
        (
            "LABEL_LONGER_THAN_THIRTY_TWO_BYTES",
            "Jane",
            TextRange { start: 5, end: 9 },
        ),
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
    let finding = WorkspaceFinding::new("CLIENT_NAME", "Jane", TextRange { start: 5, end: 9 });
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
    let finding = WorkspaceFinding::new("CLIENT_NAME", "Jane", TextRange { start: 9, end: 13 });
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
        marker.replace("PERSON", "OTHER"),
        marker.replace("]]", "X]]"),
        marker.replace("_", "__"),
        "[[PERSON_ZZZZZZ]]".to_owned(),
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
        if name == "maximum" {
            assert_eq!(next.err().unwrap().code, ErrorCode::ResourceLimit);
        } else {
            assert_eq!(
                next.unwrap().candidate_head().unwrap().revision,
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

#[test]
fn markers_are_short_unique_codes_and_restore_tolerates_case_and_crockford_substitutions() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let mut issued = Vec::new();
    for (index, label) in ["PERSON", "ACCOUNT_NUMBER"]
        .iter()
        .flat_map(|label| std::iter::repeat_n(*label, 64))
        .enumerate()
    {
        let value = format!("Value {index}");
        let mut id = [3; 16];
        id[..8].copy_from_slice(&(index as u64).to_be_bytes());
        let marker = commit(&mut workspace, label, &value, id);
        assert!(is_marker(&marker, label), "{marker}");
        issued.push((marker, value));
    }
    let codes: std::collections::BTreeSet<_> = issued
        .iter()
        .map(|(marker, _)| &marker[marker.len() - 8..marker.len() - 2])
        .collect();
    assert_eq!(codes.len(), issued.len());
    for (marker, value) in &issued {
        assert_eq!(&workspace.restore(marker).unwrap(), value);
        assert_eq!(
            &workspace.restore(&marker.to_ascii_lowercase()).unwrap(),
            value
        );
    }
    let (marker, value) = issued
        .iter()
        .find(|(marker, _)| marker[marker.len() - 8..].contains(['0', '1']))
        .unwrap();
    let (label, code) = marker.split_at(marker.len() - 8);
    let substituted = format!("{label}{}", code.replace('1', "l").replace('0', "o"));
    assert_eq!(&workspace.restore(&substituted).unwrap(), value);
    let (person, _) = &issued[0];
    assert_eq!(
        workspace
            .restore(&person.replace("PERSON", "MERCHANT"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidMarker
    );
}

#[test]
fn damaged_markers_fail_restore_but_remain_plain_text_in_new_source() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let marker = commit(&mut workspace, "PERSON", "Jane", [3; 16]);
    let shortened = format!("{}]]", &marker[..marker.len() - 3]);
    for damaged in [shortened.as_str(), "[[PERSON_ABC_DEFGHU]]", "[[PERSON_]]"] {
        assert_eq!(
            workspace
                .restore(&format!("{marker} and {damaged}"))
                .unwrap_err()
                .code,
            ErrorCode::InvalidMarker,
            "{damaged}"
        );
        let text = format!("{marker} and {damaged}");
        let mut pending = workspace.prepare(&text, &[], [4; 16]).unwrap();
        assert_eq!(pending.preview().unwrap().text, text);
        pending.discard();
    }
    for ordinary in [
        "[[Jane]]",
        "[[not a marker]]",
        "[[",
        "]] [[_x]]",
        "[[[[PERSON",
    ] {
        assert_eq!(
            workspace.restore(&format!("{ordinary} {marker}")).unwrap(),
            format!("{ordinary} Jane")
        );
    }
    assert_eq!(workspace.restore(&format!("[{marker}]")).unwrap(), "[Jane]");
}

#[test]
fn snapshots_require_canonical_unique_codes() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let first = commit(&mut workspace, "PERSON", "Jane", [3; 16]);
    let second = commit(&mut workspace, "PERSON", "John", [4; 16]);
    let bundle = workspace.export_bundle().unwrap();
    let code = |marker: &str| marker[marker.len() - 8..marker.len() - 2].to_owned();
    let (first, second) = (code(&first), code(&second));
    let position = |needle: &str| {
        bundle
            .windows(6)
            .position(|window| window == needle.as_bytes())
            .unwrap()
    };
    let mut duplicate = bundle.clone();
    let at = position(&second);
    duplicate[at..at + 6].copy_from_slice(first.as_bytes());
    assert_eq!(
        Workspace::open_bundle(&duplicate).err().unwrap().code,
        ErrorCode::InvalidSnapshot
    );
    let at = position(&first);
    if first.bytes().any(|b| b.is_ascii_alphabetic()) {
        let mut lowercase = bundle.clone();
        lowercase[at..at + 6].copy_from_slice(first.to_ascii_lowercase().as_bytes());
        assert_eq!(
            Workspace::open_bundle(&lowercase).err().unwrap().code,
            ErrorCode::InvalidSnapshot
        );
    }
    let mut excluded = bundle.clone();
    excluded[at] = b'U';
    assert_eq!(
        Workspace::open_bundle(&excluded).err().unwrap().code,
        ErrorCode::InvalidSnapshot
    );
}

#[test]
fn spellings_differing_only_in_case_or_spacing_share_one_marker_and_restore_the_first() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let first = commit(&mut workspace, "PERSON", "JOHN SMITH", [3; 16]);
    let second = commit(&mut workspace, "PERSON", "John  Smith", [4; 16]);
    let third = commit(&mut workspace, "PERSON", " john\tsmith ", [5; 16]);
    let account = commit(&mut workspace, "ACCOUNT_NUMBER", "JOHN SMITH", [6; 16]);
    assert_eq!(first, second);
    assert_eq!(first, third);
    assert!(is_marker(&account, "ACCOUNT_NUMBER"));
    assert_eq!(workspace.restore(&first).unwrap(), "JOHN SMITH");
    let reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    let later = [WorkspaceFinding::new(
        "PERSON",
        "John Smith",
        TextRange { start: 0, end: 10 },
    )];
    let next = reopened.prepare("John Smith", &later, [7; 16]).unwrap();
    assert_eq!(next.preview().unwrap().text, first);
    assert_eq!(
        next.expected_head().unwrap(),
        next.candidate_head().unwrap()
    );
    let blank = [WorkspaceFinding::new(
        "PERSON",
        " \t",
        TextRange { start: 1, end: 3 },
    )];
    assert_eq!(
        reopened
            .prepare("x \tx", &blank, [8; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidFinding
    );
}

#[test]
fn snapshots_reject_entries_that_share_a_normalized_identity() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    commit(&mut workspace, "PERSON", "Jane", [3; 16]);
    commit(&mut workspace, "PERSON", "Kane", [4; 16]);
    let mut bundle = workspace.export_bundle().unwrap();
    let at = bundle.windows(4).position(|w| w == b"Kane").unwrap();
    bundle[at] = b'j';
    assert_eq!(
        Workspace::open_bundle(&bundle).err().unwrap().code,
        ErrorCode::InvalidSnapshot
    );
}

#[test]
fn restore_reports_every_blocking_span_in_order_without_partial_text() {
    let (workspace, marker) = saved_person([1; 16]);
    let bare = &marker[2..marker.len() - 2];
    let damaged = format!("{}]]", &marker[..marker.len() - 3]);
    let text = format!(
        "👋 {marker} paid [[PERSON_ZZZZZZ]], then {damaged}; total for {bare}. ORDER_ABC123 [[note]]"
    );
    let report = workspace.restore_report(&text).unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.restored_markers, 1);
    let kinds: Vec<_> = report.issues.iter().map(|issue| issue.kind).collect();
    assert_eq!(
        kinds,
        [
            MarkerIssueKind::Unknown,
            MarkerIssueKind::Damaged,
            MarkerIssueKind::Unbracketed
        ]
    );
    let utf16: Vec<u16> = text.encode_utf16().collect();
    for (issue, expected) in report
        .issues
        .iter()
        .zip(["[[PERSON_ZZZZZZ]]", damaged.as_str(), bare])
    {
        let range = issue.byte_range;
        assert_eq!(&text[range.start..range.end], expected);
        assert_eq!(
            String::from_utf16(&utf16[issue.utf16_range.start..issue.utf16_range.end]).unwrap(),
            expected
        );
    }
    assert_eq!(
        workspace.restore(&text).unwrap_err().code,
        ErrorCode::InvalidMarker
    );
    assert_eq!(
        workspace
            .restore(&format!("Total for {bare}"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidMarker
    );
    let clean = format!(
        "{marker} and {} (ORDER_ABC123)",
        marker.to_ascii_lowercase()
    );
    let report = workspace.restore_report(&clean).unwrap();
    assert_eq!(report.text.as_deref(), Some("Jane and Jane (ORDER_ABC123)"));
    assert_eq!(report.restored_markers, 2);
    assert!(report.issues.is_empty());
}

#[test]
fn restore_rejects_wrong_label_and_single_bracket_markers() {
    let (workspace, marker) = saved_person([1; 16]);
    let code = &marker[9..15];
    let wrong_label = format!("[[OTHER_{code}]]");
    let report = workspace.restore_report(&wrong_label).unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].kind, MarkerIssueKind::Unknown);

    let single_bracket = format!("[PERSON_{code}]");
    let report = workspace.restore_report(&single_bracket).unwrap();
    assert!(
        report.text.is_none(),
        "single-bracket contains unbracketed committed code"
    );
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].kind, MarkerIssueKind::Unbracketed);
    let bare_code = format!("PERSON_{code}");
    assert_eq!(
        &single_bracket[report.issues[0].byte_range.start..report.issues[0].byte_range.end],
        bare_code
    );
}

#[test]
fn never_committed_look_alike_restores_as_plain_text() {
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let look_alike = "ORDER_ABC123";
    let report = workspace.restore_report(look_alike).unwrap();
    assert_eq!(report.text.as_deref(), Some(look_alike));
    assert!(report.issues.is_empty());

    let bracketed_look_alike = "[[ORDER_123456]]";
    let report = workspace.restore_report(bracketed_look_alike).unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].kind, MarkerIssueKind::Unknown);
}

#[test]
fn restore_report_has_correct_utf16_ranges_in_multibyte_text() {
    let (workspace, marker) = saved_person([1; 16]);
    let text = format!("👋🎉 prefix {marker} suffix 日本語");
    let report = workspace.restore_report(&text).unwrap();
    assert!(report.text.is_some());
    assert_eq!(report.restored_markers, 1);
    assert!(report.issues.is_empty());

    let code = &marker[9..15];
    let unknown = format!("[[OTHER_{code}]]");
    let text_with_issue = format!("日本語 {unknown} 漢字");
    let report = workspace.restore_report(&text_with_issue).unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.issues.len(), 1);

    let issue = &report.issues[0];
    let utf16_chars: Vec<u16> = text_with_issue.encode_utf16().collect();
    let extracted =
        String::from_utf16(&utf16_chars[issue.utf16_range.start..issue.utf16_range.end]).unwrap();
    assert_eq!(extracted, unknown);
    assert_eq!(
        &text_with_issue[issue.byte_range.start..issue.byte_range.end],
        unknown
    );
}

#[test]
fn identity_key_handles_unicode_case_and_whitespace_edge_cases() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();

    let eszett = commit(&mut workspace, "PERSON", "Straße", [3; 16]);
    let upper_eszett = commit(&mut workspace, "PERSON", "STRAßE", [4; 16]);
    assert_eq!(eszett, upper_eszett, "Straße and STRAßE share identity");
    assert_eq!(workspace.restore(&eszett).unwrap(), "Straße");

    let strasse = commit(&mut workspace, "PERSON", "STRASSE", [5; 16]);
    assert_ne!(
        eszett, strasse,
        "ß and SS are distinct: to_lowercase(ß) = ß, not ss"
    );

    let dotted_i = commit(&mut workspace, "PERSON", "İstanbul", [6; 16]);
    let lower_dotted = commit(&mut workspace, "PERSON", "i̇stanbul", [7; 16]);
    assert_eq!(
        dotted_i, lower_dotted,
        "İ lowercases to i + combining dot above"
    );

    let nbsp = commit(&mut workspace, "PERSON", "John\u{00A0}Doe", [8; 16]);
    let space = commit(&mut workspace, "PERSON", "John Doe", [9; 16]);
    assert_eq!(nbsp, space, "NBSP and space share identity");

    let tabs = commit(&mut workspace, "PERSON", "Jane\t\tSmith", [10; 16]);
    let spaces = commit(&mut workspace, "PERSON", "Jane Smith", [11; 16]);
    assert_eq!(tabs, spaces, "tabs collapse to single space");

    let leading = commit(&mut workspace, "PERSON", "  Alice  ", [12; 16]);
    let trimmed = commit(&mut workspace, "PERSON", "Alice", [13; 16]);
    assert_eq!(leading, trimmed, "surrounding spaces share identity");
    assert_eq!(
        workspace.restore(&leading).unwrap(),
        "  Alice  ",
        "first spelling (with spaces) is restored"
    );
}

#[test]
fn hmac_code_derivation_fixture_prevents_silent_algorithm_changes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/hmac-code.json")).unwrap();
    let operation: [u8; 16] = hex_bytes(fixture["operationHex"].as_str().unwrap())
        .try_into()
        .unwrap();
    let namespace: [u8; 16] = hex_bytes(fixture["namespaceHex"].as_str().unwrap())
        .try_into()
        .unwrap();
    let workspace = Workspace::create(namespace, [2; 16]).unwrap();
    let value = "Jane";
    let finding = WorkspaceFinding::new(
        fixture["label"].as_str().unwrap(),
        value,
        TextRange {
            start: 0,
            end: value.len(),
        },
    );
    let pending = workspace.prepare(value, &[finding], operation).unwrap();
    let marker = pending.preview().unwrap().text.clone();
    let expected_code = fixture["code"].as_str().unwrap();
    let expected_marker = format!(
        "[[{}_{}]]",
        fixture["label"].as_str().unwrap(),
        expected_code
    );
    assert_eq!(marker, expected_marker, "code derivation changed");
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn thirty_three_sequential_codes_are_unique() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let mut markers = Vec::new();
    for i in 0u64..33 {
        let value = format!("Value {i}");
        let mut id = [3; 16];
        id[..8].copy_from_slice(&i.to_be_bytes());
        let marker = commit(&mut workspace, "PERSON", &value, id);
        assert!(is_marker(&marker, "PERSON"));
        markers.push(marker);
    }
    let codes: std::collections::BTreeSet<_> = markers
        .iter()
        .map(|m| &m[m.len() - 8..m.len() - 2])
        .collect();
    assert_eq!(codes.len(), markers.len(), "codes should be unique");
}
