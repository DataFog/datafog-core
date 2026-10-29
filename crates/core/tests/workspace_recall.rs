use datafog_core::{
    TextRange,
    workspace::{ErrorCode, FindingSource, Workspace, WorkspaceFinding},
};

fn committed(values: &[(&str, &str)]) -> Workspace {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    for (i, &(label, text)) in values.iter().enumerate() {
        let finding = WorkspaceFinding::new(
            label,
            text,
            TextRange {
                start: 0,
                end: text.len(),
            },
        );
        let mut plan = workspace
            .prepare(text, &[finding], [(i + 3) as u8; 16])
            .unwrap();
        let expected = plan.expected_head().unwrap();
        let candidate = plan.candidate_head().unwrap();
        workspace
            .activate_saved(&mut plan, expected, candidate)
            .unwrap();
    }
    workspace
}

#[test]
fn only_committed_entries_recall_and_query_never_changes_state_or_plans() {
    let mut workspace = committed(&[("PERSON", "Jane Smith")]);
    let before = workspace.export_bundle().unwrap();
    let f = WorkspaceFinding::new("PERSON", "Bob", TextRange { start: 0, end: 3 });
    let mut pending = workspace.prepare("Bob", &[f], [9; 16]).unwrap();
    assert!(workspace.recall("Bob").unwrap().findings.is_empty());
    for _ in 0..40 {
        let result = workspace.recall("JANE\t SMITH").unwrap();
        assert_eq!(result.head, workspace.head().unwrap());
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].finding.source, FindingSource::Recall);
        assert!(result.findings[0].finding.needs_review);
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    assert!(pending.preview().is_ok());
    let expected = pending.expected_head().unwrap();
    let candidate = pending.candidate_head().unwrap();
    workspace
        .activate_saved(&mut pending, expected, candidate)
        .unwrap();
    assert_eq!(workspace.recall("Bob").unwrap().findings.len(), 1);
    let mut discarded = workspace
        .prepare(
            "Carol",
            &[WorkspaceFinding::new(
                "PERSON",
                "Carol",
                TextRange { start: 0, end: 5 },
            )],
            [10; 16],
        )
        .unwrap();
    discarded.discard();
    assert!(workspace.recall("Carol").unwrap().findings.is_empty());
    workspace.close();
    assert_eq!(
        workspace.recall("Jane").err().unwrap().code,
        ErrorCode::ClosedWorkspace
    );
}

#[test]
fn unicode_identity_and_all_original_coordinate_units_are_preserved() {
    let workspace = committed(&[("PERSON", "İ Jane"), ("CUSTOM", "é"), ("CUSTOM", "ß")]);
    let text = "👋 i\u{307}\u{a0}\tJANE! İ Jane, e\u{301} é SS ß";
    let matches = workspace.recall(text).unwrap().findings;
    let values: Vec<_> = matches
        .iter()
        .map(|m| m.finding.matched_text.as_str())
        .collect();
    assert_eq!(values, ["i\u{307}\u{a0}\tJANE", "İ Jane", "é", "ß"]);
    for found in &matches {
        let range = found.finding.byte_range;
        assert_eq!(&text[range.start..range.end], found.finding.matched_text);
        assert_eq!(
            found.codepoint_range,
            TextRange {
                start: text[..range.start].chars().count(),
                end: text[..range.end].chars().count()
            }
        );
        assert_eq!(
            found.utf16_range,
            TextRange {
                start: text[..range.start].encode_utf16().count(),
                end: text[..range.end].encode_utf16().count()
            }
        );
    }
    let reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    let findings: Vec<_> = reopened
        .recall(text)
        .unwrap()
        .findings
        .into_iter()
        .map(|m| m.finding)
        .collect();
    let plan = reopened.prepare(text, &findings, [22; 16]).unwrap();
    let preview = plan.preview().unwrap();
    assert_eq!(
        reopened.restore(&preview.text).unwrap(),
        "👋 İ Jane! İ Jane, e\u{301} é SS ß"
    );
    assert!(
        preview
            .transformations
            .iter()
            .all(|t| t.source == FindingSource::Recall)
    );
}

#[test]
fn word_boundaries_include_marks_connectors_joiners_and_non_latin_letters() {
    let workspace = committed(&[("PERSON", "Ann"), ("CUSTOM", "i"), ("CUSTOM", "\u{307}")]);
    let text =
        "Ann Annette _Ann Ann_ éAnn Ann中 Ann\u{301} Ann\u{200d} \u{200c}Ann (Ann), İ i \u{307}";
    let values: Vec<_> = workspace
        .recall(text)
        .unwrap()
        .findings
        .into_iter()
        .map(|m| m.finding.matched_text)
        .collect();
    assert_eq!(values, ["Ann", "Ann", "i", "\u{307}"]);
    assert!(
        workspace.recall("İ").unwrap().findings.is_empty(),
        "never match half a lowercase expansion"
    );
}

#[test]
fn overlapping_values_and_types_are_all_returned_in_stable_source_order() {
    let workspace = committed(&[
        ("PERSON", "Ann"),
        ("PERSON", "Ann Smith"),
        ("CUSTOM", "Smith"),
        ("CUSTOM", "Ann"),
    ]);
    let result = workspace.recall("Ann Smith; Ann").unwrap();
    let shapes: Vec<_> = result
        .findings
        .iter()
        .map(|m| {
            (
                m.finding.entity_type.as_str(),
                m.finding.matched_text.as_str(),
                m.finding.byte_range.start,
            )
        })
        .collect();
    assert_eq!(
        shapes,
        [
            ("CUSTOM", "Ann", 0),
            ("PERSON", "Ann", 0),
            ("PERSON", "Ann Smith", 0),
            ("CUSTOM", "Smith", 4),
            ("CUSTOM", "Ann", 11),
            ("PERSON", "Ann", 11)
        ]
    );
    assert!(
        workspace
            .recall("Ann-Smith")
            .unwrap()
            .findings
            .iter()
            .all(|m| m.finding.matched_text != "Ann Smith")
    );
    let crossing = committed(&[("PERSON", "Ann Smith"), ("PERSON", "Smith Jones")]);
    let crossing = crossing.recall("Ann Smith Jones").unwrap();
    assert_eq!(crossing.findings.len(), 2);
    assert_eq!(
        crossing.findings[0].finding.byte_range,
        TextRange { start: 0, end: 9 }
    );
    assert_eq!(
        crossing.findings[1].finding.byte_range,
        TextRange { start: 4, end: 15 }
    );
}

#[test]
fn committed_markers_are_skipped_and_unrestorable_markers_fail_with_safe_ranges() {
    let workspace = committed(&[("PERSON", "Ann"), ("CUSTOM", "PERSON")]);
    let findings: Vec<_> = workspace
        .recall("Ann")
        .unwrap()
        .findings
        .into_iter()
        .map(|m| m.finding)
        .collect();
    let plan = workspace.prepare("Ann", &findings, [22; 16]).unwrap();
    let marker = &plan.preview().unwrap().text;
    assert!(workspace.recall(marker).unwrap().findings.is_empty());
    assert_eq!(
        workspace
            .recall(&format!("Ann {marker}"))
            .unwrap()
            .findings
            .len(),
        1
    );
    for text in [
        "👋 [[PERSON_BAD]]".to_string(),
        "[[PERSON_000000]]".to_string(),
        marker[2..marker.len() - 2].to_string(),
    ] {
        let failure = workspace.recall(&text).err().unwrap();
        assert_eq!(failure.code, ErrorCode::InvalidMarker);
        assert_eq!(failure.marker_issues.len(), 1);
        assert!(failure.to_string().len() < 32);
    }
    assert!(
        Workspace::create([1; 16], [2; 16])
            .unwrap()
            .recall("plain")
            .unwrap()
            .findings
            .is_empty()
    );
}

#[test]
fn result_source_index_and_matching_work_limits_fail_without_partial_success() {
    let workspace = committed(&[("PERSON", "Ann")]);
    let before = workspace.export_bundle().unwrap();
    assert_eq!(
        workspace
            .recall(&"x".repeat(1024 * 1024 + 1))
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(
        workspace.recall(&"Ann ".repeat(10_001)).err().unwrap().code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let huge = "z".repeat(262_144);
    let indexed = committed(&[("CUSTOM", &huge)]);
    assert_eq!(
        indexed.recall("plain").err().unwrap().code,
        ErrorCode::ResourceLimit
    );
    let prefix = format!("{}!", "*".repeat(4096));
    let adversarial = committed(&[("CUSTOM", &prefix)]);
    assert_eq!(
        adversarial.recall(&"*".repeat(20_000)).err().unwrap().code,
        ErrorCode::ResourceLimit
    );
}
