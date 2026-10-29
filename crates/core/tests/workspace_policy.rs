use datafog_core::{
    TextRange, capabilities,
    workspace::{
        ChoiceDisposition, ErrorCode, FindingSource, PolicyEdits, TypeRule, TypeRuleEdit,
        ValueChoiceEdit, Workspace, WorkspaceFinding, WorkspaceInput, protected_type,
    },
};

fn finding(label: &str, text: &str) -> WorkspaceFinding {
    WorkspaceFinding::new(
        label,
        text,
        TextRange {
            start: 0,
            end: text.len(),
        },
    )
}
fn choice(label: &str, value: &str, disposition: Option<ChoiceDisposition>) -> PolicyEdits {
    PolicyEdits {
        rules: vec![],
        choices: vec![ValueChoiceEdit {
            entity_type: label.into(),
            value: value.into(),
            disposition,
        }],
    }
}
fn activate(workspace: &mut Workspace, edits: &PolicyEdits, operation: u8) {
    let mut plan = workspace
        .prepare_policy_changes(edits, [operation; 16])
        .unwrap();
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, next).unwrap();
}
fn commit_value(workspace: &mut Workspace, label: &str, text: &str, operation: u8) -> String {
    let mut plan = workspace
        .prepare(text, &[finding(label, text)], [operation; 16])
        .unwrap();
    let preview = plan.preview().unwrap().text.clone();
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, next).unwrap();
    preview
}

#[test]
fn every_builtin_and_model_name_address_type_is_protected() {
    for label in capabilities().supported_entities.into_iter().chain(
        [
            "FIRST_NAME",
            "LAST_NAME",
            "ADDRESS",
            "STREET_ADDRESS",
            "ACCOUNT_NUMBER",
            "CUSTOMER_ID",
        ]
        .map(String::from),
    ) {
        assert!(protected_type(&label), "{label}");
    }
    assert!(!protected_type("PRODUCT_CODE"));
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    assert_eq!(workspace.policy().unwrap().default_rule, TypeRule::Hide);
}

#[test]
fn protected_keep_is_forbidden_and_only_an_explicit_correction_waives_it() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let before = workspace.export_bundle().unwrap();
    for label in ["PERSON", "EMAIL", "SSN", "PRIVATE_KEY", "STREET_ADDRESS"] {
        assert_eq!(
            workspace
                .prepare_policy_changes(
                    &choice(label, "sample", Some(ChoiceDisposition::Keep)),
                    [3; 16]
                )
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidPolicy
        );
        let edits = PolicyEdits {
            rules: vec![TypeRuleEdit {
                entity_type: label.into(),
                rule: Some(TypeRule::Keep),
            }],
            choices: vec![],
        };
        assert_eq!(
            workspace
                .prepare_policy_changes(&edits, [3; 16])
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidPolicy
        );
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    let mut input = finding("PERSON", "May");
    input.source = FindingSource::User;
    input.needs_review = false;
    let plan = workspace.prepare("May", &[input.clone()], [3; 16]).unwrap();
    assert_ne!(plan.preview().unwrap().text, "May");
    drop(plan);
    activate(
        &mut workspace,
        &choice("PERSON", "May", Some(ChoiceDisposition::NotSensitive)),
        3,
    );
    let plan = workspace.prepare("May", &[input], [4; 16]).unwrap();
    assert_eq!(plan.preview().unwrap().text, "May");
    assert!(plan.preview().unwrap().transformations.is_empty());
    assert!(workspace.recall("May").unwrap().findings.is_empty());
}

#[test]
fn corrections_can_precede_aliases_and_removal_restores_hiding_without_reassignment() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    activate(
        &mut workspace,
        &choice(
            "PERSON",
            " Jane\u{a0}Smith ",
            Some(ChoiceDisposition::NotSensitive),
        ),
        3,
    );
    let plan = workspace
        .prepare("JANE  SMITH", &[finding("PERSON", "JANE  SMITH")], [4; 16])
        .unwrap();
    assert_eq!(plan.preview().unwrap().text, "JANE  SMITH");
    assert_eq!(
        plan.expected_head().unwrap(),
        plan.candidate_head().unwrap()
    );
    drop(plan);
    activate(&mut workspace, &choice("PERSON", "jane smith", None), 4);
    let marker = commit_value(&mut workspace, "PERSON", "Jane Smith", 5);
    activate(
        &mut workspace,
        &choice(
            "PERSON",
            "JANE\tSMITH",
            Some(ChoiceDisposition::NotSensitive),
        ),
        6,
    );
    assert_eq!(workspace.restore(&marker).unwrap(), "Jane Smith");
    assert!(workspace.recall("Jane Smith").unwrap().findings.is_empty());
    let saved = workspace.export_bundle().unwrap();
    let mut reopened = Workspace::open_bundle(&saved).unwrap();
    assert!(reopened.recall("Jane Smith").unwrap().findings.is_empty());
    activate(&mut reopened, &choice("PERSON", "jane smith", None), 7);
    assert_eq!(reopened.recall("Jane Smith").unwrap().findings.len(), 1);
    assert_eq!(
        commit_value(&mut reopened, "PERSON", "JANE SMITH", 8),
        marker
    );
    assert_eq!(reopened.restore(&marker).unwrap(), "Jane Smith");
}

#[test]
fn narrow_or_differently_typed_corrections_cannot_waive_wider_protection() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    activate(
        &mut workspace,
        &choice("PERSON", "Jane", Some(ChoiceDisposition::NotSensitive)),
        3,
    );
    let text = "Jane Doe";
    let input = [
        finding("PERSON", text),
        finding("EMAIL", text),
        WorkspaceFinding::new("PERSON", "Jane", TextRange { start: 0, end: 4 }),
    ];
    for findings in [input.to_vec(), input.into_iter().rev().collect()] {
        let plan = workspace.prepare(text, &findings, [4; 16]).unwrap();
        assert!(!plan.preview().unwrap().text.contains("Jane"));
    }
    activate(
        &mut workspace,
        &choice("PERSON", text, Some(ChoiceDisposition::NotSensitive)),
        4,
    );
    let plan = workspace
        .prepare(
            text,
            &[finding("PERSON", text), finding("EMAIL", text)],
            [5; 16],
        )
        .unwrap();
    assert!(plan.preview().unwrap().text.starts_with("[[EMAIL_"));
}

#[test]
fn custom_type_rules_and_exact_choice_updates_are_shared_by_batches_and_recall() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let marker = commit_value(&mut workspace, "PRODUCT_CODE", "Alpha", 3);
    let edits = PolicyEdits {
        rules: vec![TypeRuleEdit {
            entity_type: "PRODUCT_CODE".into(),
            rule: Some(TypeRule::Keep),
        }],
        choices: vec![],
    };
    activate(&mut workspace, &edits, 4);
    assert!(workspace.recall("Alpha").unwrap().findings.is_empty());
    let inputs = [
        WorkspaceInput {
            text: "Alpha",
            findings: &[finding("PRODUCT_CODE", "Alpha")],
        },
        WorkspaceInput {
            text: "Alpha",
            findings: &[finding("PERSON", "Alpha")],
        },
    ];
    let plan = workspace.prepare_batch(&inputs, [5; 16]).unwrap();
    assert_eq!(plan.previews().unwrap()[0].text, "Alpha");
    assert_ne!(plan.previews().unwrap()[1].text, "Alpha");
    drop(plan);
    activate(
        &mut workspace,
        &choice("PRODUCT_CODE", "Alpha", Some(ChoiceDisposition::Keep)),
        5,
    );
    activate(
        &mut workspace,
        &choice(
            "PRODUCT_CODE",
            "Alpha",
            Some(ChoiceDisposition::NotSensitive),
        ),
        6,
    );
    assert_eq!(
        workspace.policy().unwrap().choices[0].disposition,
        ChoiceDisposition::NotSensitive
    );
    activate(
        &mut workspace,
        &PolicyEdits {
            rules: vec![TypeRuleEdit {
                entity_type: "PRODUCT_CODE".into(),
                rule: None,
            }],
            choices: vec![ValueChoiceEdit {
                entity_type: "PRODUCT_CODE".into(),
                value: "Alpha".into(),
                disposition: None,
            }],
        },
        7,
    );
    assert_eq!(
        commit_value(&mut workspace, "PRODUCT_CODE", "ALPHA", 8),
        marker
    );
}

#[test]
fn malformed_findings_and_markers_validate_even_if_a_choice_would_waive_them() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    activate(
        &mut workspace,
        &choice("PERSON", "Jane", Some(ChoiceDisposition::NotSensitive)),
        3,
    );
    let mut bad = finding("PERSON", "Jane");
    bad.confidence = Some(f32::NAN);
    assert_eq!(
        workspace
            .prepare("Jane", &[bad], [4; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidFinding
    );
    let bad = finding("PERSON", "Jane");
    assert_eq!(
        workspace
            .prepare("John", &[bad], [4; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidFinding
    );
    assert_eq!(
        workspace
            .prepare("[[PERSON_ZZZZZZ]]", &[], [4; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidMarker
    );
}

#[test]
fn candidate_edits_are_inert_until_save_activation_and_obey_lifetimes_and_heads() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let before = workspace.export_bundle().unwrap();
    let edits = choice("PERSON", "May", Some(ChoiceDisposition::NotSensitive));
    let mut plan = workspace.prepare_policy_changes(&edits, [3; 16]).unwrap();
    assert_eq!(workspace.export_bundle().unwrap(), before);
    assert!(workspace.policy().unwrap().choices.is_empty());
    assert_eq!(
        plan.policy_preview().unwrap().unwrap().choices[0].before,
        None
    );
    assert!(plan.previews().unwrap().is_empty());
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    assert_eq!(
        workspace
            .activate_saved(&mut plan, parent, parent)
            .unwrap_err()
            .code,
        ErrorCode::WorkspaceConflict
    );
    assert_eq!(workspace.export_bundle().unwrap(), before);
    plan.discard();
    assert_eq!(
        plan.export_candidate_bundle().unwrap_err().code,
        ErrorCode::InactivePlan
    );
    let mut stale = workspace.prepare_policy_changes(&edits, [3; 16]).unwrap();
    activate(
        &mut workspace,
        &choice("EMAIL", "May", Some(ChoiceDisposition::NotSensitive)),
        4,
    );
    assert_eq!(
        workspace
            .activate_saved(&mut stale, parent, next)
            .unwrap_err()
            .code,
        ErrorCode::WorkspaceConflict
    );
    let pending = workspace.prepare_policy_changes(&edits, [5; 16]).unwrap();
    workspace.close();
    assert_eq!(
        pending.policy_preview().unwrap_err().code,
        ErrorCode::ClosedWorkspace
    );
}

#[test]
fn validation_is_bounded_atomic_and_rejects_duplicate_normalized_edits() {
    let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let before = workspace.export_bundle().unwrap();
    let mut edits = choice(
        "PERSON",
        "Jane Smith",
        Some(ChoiceDisposition::NotSensitive),
    );
    edits.choices.push(ValueChoiceEdit {
        entity_type: "PERSON".into(),
        value: "JANE\u{a0}SMITH".into(),
        disposition: None,
    });
    assert_eq!(
        workspace
            .prepare_policy_changes(&edits, [3; 16])
            .err()
            .unwrap()
            .code,
        ErrorCode::InvalidPolicy
    );
    for value in ["", " \t", "[[PERSON_ABC123]]"] {
        assert!(
            workspace
                .prepare_policy_changes(
                    &choice("PERSON", value, Some(ChoiceDisposition::NotSensitive)),
                    [3; 16]
                )
                .is_err()
        );
    }
    assert!(
        workspace
            .prepare_policy_changes(
                &choice("person", "Jane", Some(ChoiceDisposition::NotSensitive)),
                [3; 16]
            )
            .is_err()
    );
    assert_eq!(workspace.export_bundle().unwrap(), before);
}

fn crafted_snapshot(profile: &str, rules: &[(&str, u8)], choices: &[(&str, &str, u8)]) -> Vec<u8> {
    let mut e = minicbor::Encoder::new(Vec::new());
    e.map(8)
        .unwrap()
        .u8(0)
        .unwrap()
        .u8(2)
        .unwrap()
        .u8(1)
        .unwrap()
        .bytes(&[1; 16])
        .unwrap()
        .u8(2)
        .unwrap()
        .array(2)
        .unwrap()
        .u8(1)
        .unwrap()
        .bytes(&[3; 16])
        .unwrap()
        .u8(3)
        .unwrap()
        .array(0)
        .unwrap()
        .u8(4)
        .unwrap()
        .array(0)
        .unwrap()
        .u8(5)
        .unwrap()
        .array(0)
        .unwrap()
        .u8(6)
        .unwrap()
        .map(2)
        .unwrap()
        .u8(0)
        .unwrap()
        .str(profile)
        .unwrap()
        .u8(1)
        .unwrap()
        .array(rules.len() as u64)
        .unwrap();
    for (label, rule) in rules {
        e.array(3)
            .unwrap()
            .str(label)
            .unwrap()
            .u8(*rule)
            .unwrap()
            .bytes(&[3; 16])
            .unwrap();
    }
    e.u8(7).unwrap().array(choices.len() as u64).unwrap();
    for (label, value, disposition) in choices {
        e.array(4)
            .unwrap()
            .str(label)
            .unwrap()
            .str(value)
            .unwrap()
            .u8(*disposition)
            .unwrap()
            .bytes(&[3; 16])
            .unwrap();
    }
    e.into_writer()
}

#[test]
fn policy_decoder_rejects_forged_permissions_duplicate_identities_and_noncanonical_data() {
    let valid = crafted_snapshot("protected_v1", &[], &[("PERSON", "May", 1)]);
    assert_eq!(
        Workspace::open_bundle(&valid)
            .unwrap()
            .export_bundle()
            .unwrap(),
        valid
    );
    for index in 0..valid.len() {
        assert!(Workspace::open_bundle(&valid[..index]).is_err());
    }
    for bytes in [
        crafted_snapshot("unknown", &[], &[]),
        crafted_snapshot("protected_v1", &[("PERSON", 1)], &[]),
        crafted_snapshot("protected_v1", &[], &[("PERSON", "May", 0)]),
        crafted_snapshot("protected_v1", &[("CUSTOM", 2)], &[]),
        crafted_snapshot("protected_v1", &[], &[("CUSTOM", "May", 2)]),
        crafted_snapshot("protected_v1", &[("CUSTOM", 0), ("CUSTOM", 0)], &[]),
        crafted_snapshot(
            "protected_v1",
            &[],
            &[
                ("PERSON", "Jane Smith", 1),
                ("PERSON", "JANE\u{a0}SMITH", 1),
            ],
        ),
        crafted_snapshot("protected_v1", &[], &[("PERSON", "", 1)]),
        crafted_snapshot("protected_v1", &[], &[("person", "Jane", 1)]),
        crafted_snapshot("protected_v1", &[], &[("PERSON", "[[PERSON_ABC123]]", 1)]),
    ] {
        assert!(Workspace::open_bundle(&bytes).is_err());
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(Workspace::open_bundle(&trailing).is_err());
    let mut nonminimal = valid.clone();
    nonminimal.splice(1..2, [0x18, 0]);
    assert!(Workspace::open_bundle(&nonminimal).is_err());
    let mut duplicate_key = valid.clone();
    let pos = duplicate_key.iter().rposition(|byte| *byte == 7).unwrap();
    duplicate_key[pos] = 6;
    assert!(Workspace::open_bundle(&duplicate_key).is_err());
    let mut indefinite = valid.clone();
    indefinite[0] = 0xbf;
    assert!(Workspace::open_bundle(&indefinite).is_err());
}

#[test]
fn legacy_v1_read_and_noop_are_byte_exact_until_a_saved_mutation_upgrades() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    let hex = fixtures["high"]["bundleHex"].as_str().unwrap();
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let mut workspace = Workspace::open_bundle(&bytes).unwrap();
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    let no_op = workspace.prepare("ordinary", &[], [201; 16]).unwrap();
    assert_eq!(no_op.export_candidate_bundle().unwrap(), bytes);
    let no_settings = workspace
        .prepare_policy_changes(&choice("PERSON", "unknown", None), [201; 16])
        .unwrap();
    assert_eq!(no_settings.export_candidate_bundle().unwrap(), bytes);
    let plan = workspace
        .prepare_policy_changes(
            &choice("PERSON", "Jane", Some(ChoiceDisposition::NotSensitive)),
            [201; 16],
        )
        .unwrap();
    let candidate = plan.export_candidate_bundle().unwrap();
    assert_eq!(candidate[0], 0xa8);
    assert_eq!(candidate[2], 2);
    let imported = Workspace::open_bundle(candidate).unwrap();
    assert_eq!(
        imported
            .restore(fixtures["high"]["marker"].as_str().unwrap())
            .unwrap(),
        "Jane"
    );
    assert!(imported.recall("Jane").unwrap().findings.is_empty());
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    drop(plan);
    activate(
        &mut workspace,
        &choice("PERSON", "Jane", Some(ChoiceDisposition::NotSensitive)),
        201,
    );
    assert_eq!(workspace.export_bundle().unwrap()[2], 2);
}

#[test]
fn recovery_preserves_settings_and_permanent_aliases_with_a_fresh_head() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let marker = commit_value(&mut workspace, "PERSON", "Jane", 3);
    activate(
        &mut workspace,
        &choice("PERSON", "Jane", Some(ChoiceDisposition::NotSensitive)),
        4,
    );
    let saved = workspace.export_bundle().unwrap();
    let mut restored = Workspace::open_bundle(&saved).unwrap();
    let mut recovery = restored.prepare_recovery([5; 16]).unwrap();
    let parent = recovery.expected_head().unwrap();
    let candidate = recovery.candidate_head().unwrap();
    let backup = Workspace::open_bundle(recovery.export_candidate_bundle().unwrap()).unwrap();
    assert_eq!(
        backup.policy().unwrap().choices[0].disposition,
        ChoiceDisposition::NotSensitive
    );
    assert!(backup.recall("Jane").unwrap().findings.is_empty());
    assert_eq!(backup.restore(&marker).unwrap(), "Jane");
    restored
        .activate_saved(&mut recovery, parent, candidate)
        .unwrap();
    assert_eq!(restored.policy().unwrap().choices.len(), 1);
}
