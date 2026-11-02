use datafog_core::{
    TextRange,
    workspace::{ErrorCode, MarkerIssueKind, OperationKind, Workspace, WorkspaceFinding},
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
fn commit(workspace: &mut Workspace, label: &str, text: &str, operation: u8) -> String {
    let mut plan = workspace
        .prepare(text, &[finding(label, text)], [operation; 16])
        .unwrap();
    let marker = plan.preview().unwrap().text.clone();
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, next).unwrap();
    marker
}
fn forget(workspace: &mut Workspace, markers: &[&str], operation: u8) {
    let codes: Vec<String> = markers.iter().map(|marker| code(marker)).collect();
    let mut plan = workspace.prepare_forget(&codes, [operation; 16]).unwrap();
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, next).unwrap();
}
fn code(marker: &str) -> String {
    marker
        .trim_start_matches("[[")
        .trim_end_matches("]]")
        .rsplit_once('_')
        .unwrap()
        .1
        .to_owned()
}

#[test]
fn forgotten_marker_reports_expired_and_others_still_restore() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    let john = commit(&mut workspace, "PERSON", "John Doe", 4);
    forget(&mut workspace, &[&jane], 5);

    let report = workspace
        .restore_report(&format!("{jane} met {john}"))
        .unwrap();
    assert!(report.text.is_none());
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].kind, MarkerIssueKind::Expired);
    assert_eq!(report.issues[0].kind.as_str(), "expired");
    assert_eq!(workspace.restore(&john).unwrap(), "John Doe");
}

#[test]
fn forgotten_value_gets_a_new_marker_and_its_code_is_never_reissued() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let first = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    forget(&mut workspace, &[&first], 4);
    let second = commit(&mut workspace, "PERSON", "Jane Roe", 5);
    assert_ne!(code(&first), code(&second));
    assert_eq!(workspace.restore(&second).unwrap(), "Jane Roe");
    assert_eq!(
        workspace.restore_report(&first).unwrap().issues[0].kind,
        MarkerIssueKind::Expired
    );
}

#[test]
fn forget_records_an_operation_and_round_trips_as_version_4() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    let mut plan = workspace.prepare_forget(&[code(&jane)], [4; 16]).unwrap();
    let format = plan.snapshot_format().unwrap();
    assert_eq!((format.current_version, format.candidate_version), (3, 4));
    let parent = plan.expected_head().unwrap();
    let next = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, next).unwrap();
    let plan = workspace
        .prepare("Ann Lee", &[finding("PERSON", "Ann Lee")], [5; 16])
        .unwrap();
    let format = plan.snapshot_format().unwrap();
    assert_eq!((format.current_version, format.candidate_version), (4, 4));
    assert_eq!(
        workspace.operations().unwrap().last().unwrap().kind,
        OperationKind::Forget
    );

    let bundle = workspace.export_bundle().unwrap();
    let reopened = Workspace::open_bundle(&bundle).unwrap();
    assert_eq!(reopened.export_bundle().unwrap(), bundle);
    assert_eq!(
        reopened.restore_report(&jane).unwrap().issues[0].kind,
        MarkerIssueKind::Expired
    );
}

#[test]
fn forgetting_every_value_of_a_label_keeps_a_valid_snapshot() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    let email = commit(&mut workspace, "EMAIL", "jane@example.test", 4);
    forget(&mut workspace, &[&jane, &email], 5);
    let mut reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
    let again = commit(&mut reopened, "PERSON", "Jane Roe", 6);
    assert_ne!(code(&again), code(&jane));
}

#[test]
fn forget_rejects_unknown_duplicate_empty_and_retired_codes() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    let before = workspace.export_bundle().unwrap();
    let jane_code = code(&jane);
    for codes in [
        vec![],
        vec!["ZZZZZZ".to_owned()],
        vec![jane_code.clone(), jane_code.clone()],
    ] {
        let failure = workspace.prepare_forget(&codes, [4; 16]).err().unwrap();
        assert_eq!(failure.code, ErrorCode::InvalidMarker);
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
    forget(&mut workspace, &[&jane], 5);
    let failure = workspace
        .prepare_forget(&[jane_code], [6; 16])
        .err()
        .unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidMarker);
}

#[test]
fn forget_rejects_a_reused_operation_id() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    let failure = workspace
        .prepare_forget(&[code(&jane)], [3; 16])
        .err()
        .unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidId);
}

#[test]
fn source_text_with_a_forgotten_marker_still_blocks_preparation() {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let jane = commit(&mut workspace, "PERSON", "Jane Roe", 3);
    forget(&mut workspace, &[&jane], 4);
    let text = format!("{jane} and Ann Lee");
    let failure = workspace
        .prepare(
            &text,
            &[WorkspaceFinding::new(
                "PERSON",
                "Ann Lee",
                TextRange {
                    start: jane.len() + 5,
                    end: text.len(),
                },
            )],
            [5; 16],
        )
        .err()
        .unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidMarker);
}
