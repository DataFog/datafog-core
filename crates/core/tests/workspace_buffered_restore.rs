use datafog_core::{
    TextRange,
    workspace::{
        ErrorCode, MAX_BUNDLE_BYTES, RestoreEncoding, RestoreReport, Workspace, WorkspaceFinding,
    },
};
fn seed() -> (Workspace, String) {
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let mut plan = workspace
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
    let marker = plan.preview().unwrap().text.clone();
    let expected = plan.expected_head().unwrap();
    let saved = plan.candidate_head().unwrap();
    workspace
        .activate_saved(&mut plan, expected, saved)
        .unwrap();
    (workspace, marker)
}
fn same(actual: RestoreReport, expected: &RestoreReport) {
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.restored_markers, expected.restored_markers);
    assert_eq!(actual.issues, expected.issues);
}
#[test]
fn every_byte_and_utf16_split_matches_existing_strict_r2_with_original_reports() {
    let (workspace, marker) = seed();
    let before = workspace.export_bundle().unwrap();
    let sources = [
        format!("Hello {marker}!"),
        format!("{marker}{marker}"),
        format!("👋 e\u{301} {marker} plus {marker}"),
        marker.to_lowercase(),
        format!("👋 {marker} [[PERSON_BAD]]"),
        "👋 [[PERSON_ZZZZZZ]]".into(),
        format!("Hello {}!", &marker[2..marker.len() - 2]),
        "Hello [[PERSON_".into(),
        "ordinary [ [code] text".into(),
        "".into(),
    ];
    for source in sources {
        let expected = workspace.restore_report(&source).unwrap();
        for split in 0..=source.len() {
            let mut response = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
            response.feed(&source.as_bytes()[..split]).unwrap();
            response.feed(&source.as_bytes()[split..]).unwrap();
            same(response.finalize().unwrap(), &expected);
        }
        let units: Vec<_> = source.encode_utf16().collect();
        for split in 0..=units.len() {
            let mut response = workspace.buffered_restore(RestoreEncoding::Utf16).unwrap();
            response.feed_utf16(&units[..split]).unwrap();
            response.feed_utf16(&units[split..]).unwrap();
            same(response.finalize().unwrap(), &expected);
        }
        let mut one_byte = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
        for byte in source.as_bytes() {
            one_byte.feed(&[*byte]).unwrap();
        }
        same(one_byte.finalize().unwrap(), &expected);
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
#[test]
fn unicode_errors_do_not_replace_input_or_return_partial_text_and_reset_is_explicit() {
    let (workspace, marker) = seed();
    let mut utf8 = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
    utf8.feed(marker.as_bytes()).unwrap();
    utf8.feed(&[0xF0]).unwrap();
    assert_eq!(
        utf8.finalize().err().unwrap().code,
        ErrorCode::InvalidEncoding
    );
    assert_eq!(
        utf8.feed(b"tail").err().unwrap().code,
        ErrorCode::InactiveStream
    );
    utf8.reset().unwrap();
    utf8.feed(b"fresh").unwrap();
    assert_eq!(utf8.finalize().unwrap().text.as_deref(), Some("fresh"));
    assert_eq!(
        utf8.finalize().err().unwrap().code,
        ErrorCode::InactiveStream
    );
    let mut utf16 = workspace.buffered_restore(RestoreEncoding::Utf16).unwrap();
    utf16.feed_utf16(&[0xD83D]).unwrap();
    utf16.feed_utf16(&[]).unwrap();
    utf16.feed_utf16(&[0xDC4B]).unwrap();
    assert_eq!(utf16.finalize().unwrap().text.as_deref(), Some("👋"));
    for units in [&[0xD83D][..], &[0xDC4B][..], &[0xD83D, 0x0041][..]] {
        utf16.reset().unwrap();
        match utf16.feed_utf16(units) {
            Ok(()) => assert_eq!(
                utf16.finalize().err().unwrap().code,
                ErrorCode::InvalidEncoding
            ),
            Err(error) => assert_eq!(error.code, ErrorCode::InvalidEncoding),
        }
    }
    utf16.reset().unwrap();
    assert_eq!(
        utf16.feed(b"not UTF16 units").err().unwrap().code,
        ErrorCode::InvalidEncoding
    );
    utf16.reset().unwrap();
    utf16.feed_utf16(&[65]).unwrap();
    assert_eq!(utf16.finalize().unwrap().text.as_deref(), Some("A"));
}
#[test]
fn close_cancel_stale_head_and_noop_activation_obey_committed_lifetime() {
    let (mut workspace, marker) = seed();
    let mut response = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
    response.feed(marker.as_bytes()).unwrap();
    let mut noop = workspace.prepare("ordinary", &[], [4; 16]).unwrap();
    let expected = noop.expected_head().unwrap();
    let saved = noop.candidate_head().unwrap();
    workspace
        .activate_saved(&mut noop, expected, saved)
        .unwrap();
    assert_eq!(response.finalize().unwrap().text.as_deref(), Some("Jane"));
    response.reset().unwrap();
    response.feed(b"unfinished ").unwrap();
    let mut change = workspace
        .prepare(
            "Bob",
            &[WorkspaceFinding::new(
                "PERSON",
                "Bob",
                TextRange { start: 0, end: 3 },
            )],
            [5; 16],
        )
        .unwrap();
    // A candidate and failed/nonexistent host save do not advance the pinned head.
    response.feed(marker.as_bytes()).unwrap();
    let expected = change.expected_head().unwrap();
    let saved = change.candidate_head().unwrap();
    workspace
        .activate_saved(&mut change, expected, saved)
        .unwrap();
    assert_eq!(
        response.finalize().err().unwrap().code,
        ErrorCode::WorkspaceConflict
    );
    let mut cancelled = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
    cancelled.feed(b"never released").unwrap();
    cancelled.close();
    cancelled.close();
    assert_eq!(
        cancelled.finalize().err().unwrap().code,
        ErrorCode::InactiveStream
    );
    assert_eq!(
        cancelled.reset().err().unwrap().code,
        ErrorCode::InactiveStream
    );
    let mut closed = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
    closed.feed(marker.as_bytes()).unwrap();
    workspace.close();
    assert_eq!(
        closed.finalize().err().unwrap().code,
        ErrorCode::ClosedWorkspace
    );
}
#[test]
fn input_limits_stop_whole_response_without_retaining_a_partial_success() {
    let (workspace, _) = seed();
    let before = workspace.export_bundle().unwrap();
    let mut response = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
    assert_eq!(
        response
            .feed(&vec![b'x'; MAX_BUNDLE_BYTES + 1])
            .err()
            .unwrap()
            .code,
        ErrorCode::ResourceLimit
    );
    assert_eq!(
        response.finalize().err().unwrap().code,
        ErrorCode::InactiveStream
    );
    response.reset().unwrap();
    response.feed(b"okay").unwrap();
    assert_eq!(response.finalize().unwrap().text.as_deref(), Some("okay"));
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
