use datafog_core::{
    TextRange,
    workspace::{
        ErrorCode, MAX_BUNDLE_BYTES, MarkerIssueKind, RestoreEncoding, RestoreProgress, Workspace,
        WorkspaceFinding,
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
fn progress(text: &str, markers: usize) -> RestoreProgress {
    RestoreProgress {
        bytes: text.len(),
        codepoints: text.chars().count(),
        utf16: text.encode_utf16().count(),
        restored_markers: markers,
    }
}
#[test]
fn every_byte_and_utf16_split_restores_valid_responses_without_state_mutation() {
    let (workspace, marker) = seed();
    let before = workspace.export_bundle().unwrap();
    for source in [
        format!("Hello {marker}!"),
        format!("{marker}{marker}"),
        format!("👋 e\u{301} {marker} plus {marker}"),
        marker.to_lowercase(),
        "ordinary [ [code] [[plain]] text".into(),
        "".into(),
        "x".repeat(1000),
    ] {
        let expected = workspace.restore_report(&source).unwrap();
        let expected_text = expected.text.as_deref().unwrap();
        for split in 0..=source.len() {
            let mut response = workspace
                .progressive_restore(RestoreEncoding::Utf8)
                .unwrap();
            let mut text = String::new();
            for bytes in [&source.as_bytes()[..split], &source.as_bytes()[split..]] {
                let chunk = response.feed(bytes).unwrap();
                assert!(!chunk.complete);
                text.push_str(&chunk.text);
                assert_eq!(
                    chunk.delivered,
                    progress(&text, chunk.delivered.restored_markers)
                );
            }
            let end = response.finalize().unwrap();
            assert!(end.complete);
            text.push_str(&end.text);
            assert_eq!(text, expected_text, "source {source} at byte {split}");
            assert_eq!(end.delivered, progress(&text, expected.restored_markers));
        }
        let units: Vec<_> = source.encode_utf16().collect();
        for split in 0..=units.len() {
            let mut response = workspace
                .progressive_restore(RestoreEncoding::Utf16)
                .unwrap();
            let mut text = String::new();
            for part in [&units[..split], &units[split..]] {
                text.push_str(&response.feed_utf16(part).unwrap().text);
            }
            let end = response.finalize().unwrap();
            text.push_str(&end.text);
            assert_eq!(text, expected_text);
            assert_eq!(end.delivered, progress(&text, expected.restored_markers));
        }
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
#[test]
fn late_failure_retains_only_previous_call_counts_and_original_unicode_ranges() {
    let (workspace, marker) = seed();
    let before = workspace.export_bundle().unwrap();
    for invalid in ["[[PERSON_BAD]]", "[[PERSON_ZZZZZZ]]"] {
        let prefix = format!("👋 e\u{301} {marker} ");
        let mut stream = workspace
            .progressive_restore(RestoreEncoding::Utf8)
            .unwrap();
        let delivered = stream.feed(prefix.as_bytes()).unwrap();
        assert_eq!(delivered.text, "👋 e\u{301} Jane ");
        let error = stream
            .feed(format!("discard this call {invalid}").as_bytes())
            .err()
            .unwrap();
        assert_eq!(error.error.code, ErrorCode::InvalidMarker);
        assert!(!error.complete);
        assert_eq!(error.delivered, delivered.delivered);
        let full = format!("{prefix}discard this call {invalid}");
        let report = workspace.restore_report(&full).unwrap();
        assert_eq!(error.error.marker_issues, report.issues);
        assert_eq!(
            stream.feed(b"tail").err().unwrap().error.code,
            ErrorCode::InactiveStream
        );
        stream.reset().unwrap();
        assert_eq!(
            stream.feed(b"new response.").unwrap().delivered,
            progress("new response.", 0)
        );
    }
    assert_eq!(workspace.export_bundle().unwrap(), before);
}
#[test]
fn committed_bare_words_fail_without_releasing_the_word_but_substrings_are_ordinary() {
    let (workspace, marker) = seed();
    let bare = &marker[2..marker.len() - 2];
    for source in [
        bare.to_string(),
        format!("👋 {bare}!"),
        format!("[{bare}]"),
        format!("[[{bare} x"),
        format!("[[{bare}"),
        format!("[[{bare}]"),
    ] {
        let expected = workspace.restore_report(&source).unwrap();
        assert_eq!(expected.issues.len(), 1);
        for split in 0..=source.len() {
            let mut stream = workspace
                .progressive_restore(RestoreEncoding::Utf8)
                .unwrap();
            let mut failure = None;
            let mut emitted = String::new();
            for part in [&source.as_bytes()[..split], &source.as_bytes()[split..]] {
                match stream.feed(part) {
                    Ok(chunk) => emitted.push_str(&chunk.text),
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            if failure.is_none() {
                failure = stream.finalize().err();
            }
            let failure = failure.unwrap();
            assert_eq!(failure.error.marker_issues, expected.issues);
            assert!(!emitted.contains(bare));
        }
    }
    for source in [
        format!("x{bare}"),
        format!("{bare}x"),
        format!("{bare}_extra"),
        format!("x{bare}x"),
        "order_ABC123".into(),
    ] {
        let mut stream = workspace
            .progressive_restore(RestoreEncoding::Utf8)
            .unwrap();
        let first = stream.feed(source.as_bytes()).unwrap();
        let end = stream.finalize().unwrap();
        assert_eq!(format!("{}{}", first.text, end.text), source);
    }
}
#[test]
fn unfinished_and_oversized_prefixes_stop_progressive_only_and_never_flush_on_eof() {
    let (workspace, _) = seed();
    for suffix in ["[", "[[", "[[P", "[[PERSON_", "[[PERSON_BAD]", "[[plain"] {
        let mut stream = workspace
            .progressive_restore(RestoreEncoding::Utf8)
            .unwrap();
        let prefix = stream.feed("👋 safe ".as_bytes()).unwrap();
        let held = stream.feed(suffix.as_bytes()).unwrap();
        assert_eq!(held.text, "");
        let failure = stream.finalize().err().unwrap();
        assert_eq!(failure.error.code, ErrorCode::InvalidMarker);
        assert_eq!(failure.delivered, prefix.delivered);
        let issue = &failure.error.marker_issues[0];
        assert_eq!(issue.kind, MarkerIssueKind::Damaged);
        assert_eq!(
            issue.byte_range,
            TextRange {
                start: "👋 safe ".len(),
                end: "👋 safe ".len() + suffix.len()
            }
        );
        assert_eq!(
            issue.utf16_range,
            TextRange {
                start: 8,
                end: 8 + suffix.len()
            }
        );
        let mut strict = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
        strict.feed(suffix.as_bytes()).unwrap();
        assert_eq!(strict.finalize().unwrap().text.as_deref(), Some(suffix));
        assert_eq!(workspace.restore(suffix).unwrap(), suffix);
    }
    let mut stream = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    stream.feed(b"safe ").unwrap();
    assert_eq!(
        stream
            .feed(format!("[[PERSON_{}", "A".repeat(100)).as_bytes())
            .err()
            .unwrap()
            .error
            .code,
        ErrorCode::InvalidMarker
    );
}
#[test]
fn encoding_cancel_reset_stale_noop_and_close_are_explicit() {
    let (mut workspace, marker) = seed();
    let mut stream = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    stream.feed(b"safe ").unwrap();
    stream.feed(&[0xf0]).unwrap();
    assert_eq!(
        stream.finalize().err().unwrap().error.code,
        ErrorCode::InvalidEncoding
    );
    stream.reset().unwrap();
    assert_eq!(
        stream.feed(&[0xff]).err().unwrap().error.code,
        ErrorCode::InvalidEncoding
    );
    stream.reset().unwrap();
    assert_eq!(
        stream.feed_utf16(&[65]).err().unwrap().error.code,
        ErrorCode::InvalidEncoding
    );
    let mut utf16 = workspace
        .progressive_restore(RestoreEncoding::Utf16)
        .unwrap();
    utf16.feed_utf16(&[0xd83d]).unwrap();
    assert_eq!(
        utf16.feed_utf16(&[65]).err().unwrap().error.code,
        ErrorCode::InvalidEncoding
    );
    utf16.reset().unwrap();
    assert_eq!(
        utf16.feed(b"x").err().unwrap().error.code,
        ErrorCode::InvalidEncoding
    );
    stream.reset().unwrap();
    stream.feed(marker.as_bytes()).unwrap();
    let mut noop = workspace.prepare("ordinary", &[], [4; 16]).unwrap();
    let parent = noop.expected_head().unwrap();
    let saved = noop.candidate_head().unwrap();
    workspace.activate_saved(&mut noop, parent, saved).unwrap();
    assert!(stream.finalize().unwrap().complete);
    stream.reset().unwrap();
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
    let parent = change.expected_head().unwrap();
    let saved = change.candidate_head().unwrap();
    workspace
        .activate_saved(&mut change, parent, saved)
        .unwrap();
    assert_eq!(
        stream.feed(b"later").err().unwrap().error.code,
        ErrorCode::WorkspaceConflict
    );
    let mut cancelled = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    cancelled.feed(b"[").unwrap();
    cancelled.close();
    cancelled.close();
    assert_eq!(
        cancelled.finalize().err().unwrap().error.code,
        ErrorCode::InactiveStream
    );
    assert_eq!(
        cancelled.reset().err().unwrap().error.code,
        ErrorCode::InactiveStream
    );
    let mut closed = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    workspace.close();
    assert_eq!(
        closed.feed(b"tail").err().unwrap().error.code,
        ErrorCode::ClosedWorkspace
    );
}
#[test]
fn response_limit_is_cumulative_and_failed_call_does_not_increment_delivery() {
    let (workspace, _) = seed();
    let mut stream = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    let previous = stream.feed(b"safe.").unwrap().delivered;
    let error = stream.feed(&vec![b'x'; MAX_BUNDLE_BYTES]).err().unwrap();
    assert_eq!(error.error.code, ErrorCode::ResourceLimit);
    assert_eq!(error.delivered, previous);
    stream.reset().unwrap();
    assert_eq!(
        stream.feed(b"fresh.").unwrap().delivered,
        progress("fresh.", 0)
    );
}
#[test]
fn bounded_deterministic_grammar_corpus_agrees_with_r2_on_complete_recognized_responses() {
    let (workspace, marker) = seed();
    let bare = &marker[2..marker.len() - 2];
    let mut random = 7u64;
    let parts = [
        "a",
        "0",
        "_",
        "[",
        "]",
        "[[",
        "]]",
        " ",
        "👋",
        "é",
        &marker,
        bare,
        "[[PERSON_BAD]]",
        "[[PERSON_ZZZZZZ]]",
    ];
    for _ in 0..4000 {
        let mut source = String::new();
        for _ in 0..8 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push_str(parts[(random >> 32) as usize % parts.len()]);
        }
        source.push('!');
        let expected = workspace.restore_report(&source).unwrap();
        let mut stream = workspace
            .progressive_restore(RestoreEncoding::Utf8)
            .unwrap();
        match stream.feed(source.as_bytes()) {
            Ok(first) => match stream.finalize() {
                Ok(end) => assert_eq!(
                    Some(format!("{}{}", first.text, end.text)),
                    expected.text,
                    "source {source}"
                ),
                Err(_) => assert!(expected.text.is_none(), "source {source}"),
            },
            Err(_) => assert!(expected.text.is_none(), "source {source}"),
        }
    }
}

#[test]
fn shared_streaming_command_fixture_matches_actual_core_results() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/workspace/streaming-contract-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["schemaVersion"], 1);
    for case in fixture["cases"].as_array().unwrap() {
        let (mut workspace, marker) = seed();
        let bare = &marker[2..marker.len() - 2];
        let encoding = if case["encoding"] == "utf16" {
            RestoreEncoding::Utf16
        } else {
            RestoreEncoding::Utf8
        };
        let mut stream = workspace.progressive_restore(encoding).unwrap();
        for step in case["steps"].as_array().unwrap() {
            let op = step["op"].as_str().unwrap();
            let result = match op {
                "feed" => {
                    let bytes = if let Some(bytes) = step["bytes"].as_array() {
                        bytes
                            .iter()
                            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
                            .collect::<Vec<_>>()
                    } else {
                        step["text"]
                            .as_str()
                            .unwrap()
                            .replace("{marker}", &marker)
                            .replace("{bareMarker}", bare)
                            .replace(
                                "{oversizedPrefix}",
                                &format!("[[PERSON_{}", "A".repeat(100)),
                            )
                            .into_bytes()
                    };
                    stream.feed(&bytes)
                }
                "feedUtf16" => stream.feed_utf16(
                    &step["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                        .collect::<Vec<_>>(),
                ),
                "finalize" => stream.finalize(),
                "reset" => {
                    stream.reset().unwrap();
                    continue;
                }
                "closeStream" => {
                    stream.close();
                    continue;
                }
                "closeWorkspace" => {
                    workspace.close();
                    continue;
                }
                "advanceWorkspace" => {
                    let mut plan = workspace
                        .prepare(
                            "Bob",
                            &[WorkspaceFinding::new(
                                "PERSON",
                                "Bob",
                                TextRange { start: 0, end: 3 },
                            )],
                            [4; 16],
                        )
                        .unwrap();
                    let parent = plan.expected_head().unwrap();
                    let saved = plan.candidate_head().unwrap();
                    workspace.activate_saved(&mut plan, parent, saved).unwrap();
                    continue;
                }
                other => panic!("unsupported fixture operation {other}"),
            };
            if let Some(expected) = step.get("expect") {
                let chunk = result.unwrap();
                assert_eq!(
                    chunk.text,
                    expected["text"].as_str().unwrap(),
                    "{}",
                    case["id"]
                );
                assert_eq!(chunk.complete, expected["complete"].as_bool().unwrap());
                assert_eq!(
                    chunk.delivered,
                    progress(
                        expected["deliveredText"].as_str().unwrap(),
                        expected["restoredMarkers"].as_u64().unwrap() as usize
                    )
                );
            } else {
                let expected = &step["expectError"];
                let failure = result.err().unwrap();
                assert_eq!(
                    failure.error.code.as_str(),
                    expected["code"].as_str().unwrap(),
                    "{}",
                    case["id"]
                );
                assert!(!failure.complete);
                assert_eq!(
                    failure.delivered,
                    progress(
                        expected["deliveredText"].as_str().unwrap(),
                        expected["restoredMarkers"].as_u64().unwrap() as usize
                    )
                );
                if let Some(issues) = expected.get("markerIssues") {
                    let actual:Vec<_>=failure.error.marker_issues.iter().map(|issue|serde_json::json!({"kind":issue.kind.as_str(),"byteRange":issue.byte_range,"codepointRange":issue.codepoint_range,"utf16Range":issue.utf16_range})).collect();
                    assert_eq!(serde_json::json!(actual), *issues);
                }
                if let Some(kind) = expected["issueKind"].as_str() {
                    assert_eq!(failure.error.marker_issues[0].kind.as_str(), kind);
                }
            }
        }
        if let Some(strict) = case["strictR2"].as_str() {
            assert_eq!(workspace.restore(strict).unwrap(), strict);
            let mut atomic = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
            atomic.feed(strict.as_bytes()).unwrap();
            assert_eq!(atomic.finalize().unwrap().text.as_deref(), Some(strict));
        }
    }
}

#[test]
fn restored_output_limit_counts_expansion_across_successful_calls() {
    let display = "Z".repeat(4096);
    let mut workspace = Workspace::create([10; 16], [11; 16]).unwrap();
    let mut plan = workspace
        .prepare(
            &display,
            &[WorkspaceFinding::new(
                "PERSON",
                &display,
                TextRange {
                    start: 0,
                    end: display.len(),
                },
            )],
            [12; 16],
        )
        .unwrap();
    let marker = plan.preview().unwrap().text.clone();
    let parent = plan.expected_head().unwrap();
    let saved = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, saved).unwrap();
    let mut stream = workspace
        .progressive_restore(RestoreEncoding::Utf8)
        .unwrap();
    let first = stream.feed(marker.as_bytes()).unwrap();
    assert_eq!(first.text, display);
    let failure = stream
        .feed(marker.repeat(MAX_BUNDLE_BYTES / display.len()).as_bytes())
        .err()
        .unwrap();
    assert_eq!(failure.error.code, ErrorCode::ResourceLimit);
    assert_eq!(failure.delivered, first.delivered);
    assert_eq!(
        stream.finalize().err().unwrap().error.code,
        ErrorCode::InactiveStream
    );
}
