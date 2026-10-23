use datafog_core::{
    TextRange,
    workspace::{ErrorCode, Workspace, WorkspaceFinding, byte_range_from_utf16},
};

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
    assert!(workspace.prepare(&marker, &[], [4; 16]).is_err());
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
