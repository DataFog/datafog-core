use super::*;

fn descriptor() -> ModelSessionDescriptor {
    ModelSessionDescriptor {
        contract_version: 1,
        configuration_id: "test".into(),
        state: "ready".into(),
        backend: "datafog_native".into(),
        model: "DataFog/pii-en-65m".into(),
        model_version: "0.1.0".into(),
        model_revision: MODEL_REVISION.into(),
        runtime_version: "0.2.0".into(),
        runtime_revision: RUNTIME_REVISION.into(),
        adapter_version: ADAPTER_VERSION.into(),
        manifest_sha256: "a".repeat(64),
        platform: host_platform().unwrap().into(),
        labels: LABELS
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        limits: ResourceLimits {
            max_input_bytes: 10000,
            max_response_bytes: 100000,
            max_findings: 100,
            timeout_ms: 1000,
            cleanup_timeout_ms: 1000,
        },
        device: "cpu".into(),
        intra_threads: 1,
        queue_capacity: 0,
        max_length: 512,
        stride: 42,
        calibration_sha256: "b".repeat(64),
        threshold: 0.5,
    }
}

fn span(start: usize, end: usize) -> NativeSpan {
    NativeSpan {
        native_label: "first_name".into(),
        start,
        end,
        coordinate_unit: CoordinateUnit::Utf8Byte,
        raw_score: Some(RawScore {
            value: 0.9,
            scale_id: "normalized_token_softmax_min".into(),
        }),
        calibrated_confidence: Some(0.8),
        matched_text: None,
    }
}

#[test]
fn unicode_repeated_values_use_supplied_ranges_and_transform() {
    let text = "👋 e\u{301} May May";
    let start = text.rfind("May").unwrap();
    let findings = convert(
        text,
        BackendResult {
            spans: vec![span(start, start + 3)],
        },
        &descriptor(),
    )
    .unwrap();
    let finding = &findings[0].0;
    assert_eq!(finding.matched_text, "May");
    assert_eq!(finding.codepoint_range.start, text[..start].chars().count());
    assert_eq!(
        crate::utf16_range(text, finding.byte_range).unwrap().start,
        text[..start].encode_utf16().count()
    );
    let config = crate::TransformationConfig::new(crate::TransformationStrategy::Redact);
    let result = crate::transform(text, &[finding.clone()], &config).unwrap();
    assert_eq!(result.text, "👋 e\u{301} May [FIRST_NAME]");
}

#[test]
fn codepoint_ranges_are_rebased_without_search() {
    let mut source = span(3, 6);
    source.coordinate_unit = CoordinateUnit::UnicodeCodepoint;
    let findings = convert(
        "👋x May",
        BackendResult {
            spans: vec![source],
        },
        &descriptor(),
    )
    .unwrap();
    assert_eq!(findings[0].0.matched_text, "May");
    assert_eq!(findings[0].0.byte_range.start, 6);
}

#[test]
fn malformed_ranges_labels_text_and_scores_fail_atomically() {
    let text = "👋 May";
    let valid = span(5, 8);
    let mut cases = Vec::new();
    for (start, end) in [(1, 4), (5, 5), (8, 5), (5, 9), (usize::MAX, usize::MAX)] {
        let mut invalid = valid.clone();
        invalid.start = start;
        invalid.end = end;
        cases.push(invalid);
    }
    for score in [f64::NAN, f64::INFINITY, -0.1, 1.4] {
        let mut invalid = valid.clone();
        invalid.calibrated_confidence = None;
        invalid.raw_score.as_mut().unwrap().value = score;
        cases.push(invalid);
        let mut invalid = valid.clone();
        invalid.calibrated_confidence = Some(score);
        cases.push(invalid);
    }
    let mut unknown = valid.clone();
    unknown.native_label = "other".into();
    cases.push(unknown);
    let mut mismatch = valid.clone();
    mismatch.matched_text = Some("secret".into());
    cases.push(mismatch);
    for invalid in cases {
        let error = convert(
            text,
            BackendResult {
                spans: vec![valid.clone(), invalid],
            },
            &descriptor(),
        )
        .unwrap_err();
        assert_eq!(error.finding_index, Some(1));
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn native_error_messages_are_not_forwarded_or_unknown_labels_discarded() {
    let error =
        decode_datafog(br#"{"error":"private@example.test /sensitive/path"}"#, 10).unwrap_err();
    assert!(!format!("{error:?}").contains("private"));
    for invalid in [br#"{}"#.as_slice(), br#"{"findings":[{"label":"first_name","start":true,"end":2,"raw_score":0.9,"confidence":0.8}]}"#] {
        assert!(decode_datafog(invalid, 10).is_err());
    }
    assert!(
        decode_datafog(br#"{"findings":[]}"#, 10)
            .unwrap()
            .spans
            .is_empty()
    );
}

#[test]
fn composition_keeps_provenance_and_reindexes_duplicate_evidence() {
    let session = ModelSession {
        descriptor: descriptor(),
        rules: ScanConfig::default(),
        structured: structured::StructuredScanConfig::default(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
    };
    let text = "May may@example.test";
    let mut entries = convert(
        text,
        BackendResult {
            spans: vec![span(0, 3)],
        },
        &session.descriptor,
    )
    .unwrap();
    entries.extend(crate::scan(text).into_iter().map(rule_entry));
    entries.push(entries[0].clone());
    let result = session.compose(entries, ScanMode::Combined, false).unwrap();
    assert_eq!(result.findings.len(), 2);
    assert_eq!(result.evidence[0].sources.len(), 2);
    assert_eq!(result.evidence[1].finding_index, 1);
    assert!(result.diagnostics.is_none());
    assert_eq!(result.findings[1].confidence, None);
    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(session.inspect().state, "closed");
    assert_eq!(
        session
            .scan("", ScanMode::Combined, false, &Cancellation::default())
            .unwrap_err()
            .code,
        "model_session_closed"
    );
}

#[test]
fn configuration_errors_are_shared_precise_and_value_free() {
    let valid = serde_json::json!({
        "bundle": "/sensitive/local/path", "manifest_sha256": "a".repeat(64),
        "limits": descriptor().limits,
    });
    assert!(parse_config(&valid).is_ok());
    let mut unknown = valid.clone();
    unknown["other"] = "private@example.test".into();
    let error = parse_config(&unknown).err().unwrap();
    assert_eq!(error.reason, "unknown_field");
    assert_eq!(error.path, "/other");
    assert!(!format!("{error:?}").contains("private"));
    let mut missing = valid.clone();
    missing["limits"]
        .as_object_mut()
        .unwrap()
        .remove("timeout_ms");
    let error = parse_config(&missing).err().unwrap();
    assert_eq!(error.reason, "missing_field");
    assert_eq!(error.path, "/limits/timeout_ms");
    let mut malformed = valid.clone();
    malformed["limits"]["timeout_ms"] = true.into();
    assert_eq!(
        parse_config(&malformed).err().unwrap().reason,
        "invalid_type"
    );
    let mut rules = valid.clone();
    rules["rules"] = serde_json::json!({"entities":["STREET_ADDRESS"]});
    let error = ModelSession::open(parse_config(&rules).unwrap())
        .err()
        .unwrap();
    assert_eq!(error.path, "/rules/entities/0");
    assert!(!format!("{error:?}").contains("sensitive"));
}

#[test]
fn busy_and_oversized_calls_fail_without_rule_only_fallback() {
    let session = ModelSession {
        descriptor: descriptor(),
        rules: ScanConfig::default(),
        structured: structured::StructuredScanConfig::default(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
    };
    let held = session.inner.lock().unwrap();
    assert_eq!(
        session
            .scan(
                "may@example.test",
                ScanMode::Combined,
                false,
                &Cancellation::default()
            )
            .unwrap_err()
            .code,
        "model_busy"
    );
    drop(held);
    let text = "x".repeat(session.descriptor.limits.max_input_bytes + 1);
    assert_eq!(
        session
            .scan(&text, ScanMode::Combined, false, &Cancellation::default())
            .unwrap_err()
            .reason,
        "limit_exceeded"
    );
    assert_eq!(session.inspect().state, "ready");
    assert!(bounded_json(&serde_json::json!({"long_key":"x"}), 3, "/data").is_err());
}

#[cfg(unix)]
#[test]
fn process_timeout_cancellation_and_oversized_output_are_reaped() {
    use std::os::unix::fs::PermissionsExt;
    let directory =
        std::env::temp_dir().join(format!("datafog-worker-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join("worker");
    let mut limits = descriptor().limits;
    for body in [
        "while IFS= read -r line; do while :; do :; done; done",
        "while IFS= read -r line; do printf '%s\\n' 'response larger than permitted'; done",
    ] {
        std::fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        limits.max_response_bytes = 4;
        let mut worker = process::Worker::spawn(&executable, &directory, &limits).unwrap();
        let result = worker.exchange("", Instant::now() + Duration::from_millis(30), || false);
        assert!(result.is_err());
        worker.stop().unwrap();
        worker.stop().unwrap();
    }
    let mut worker = process::Worker::spawn(&executable, &directory, &limits).unwrap();
    let result = worker.exchange("", Instant::now() + Duration::from_secs(1), || true);
    assert_eq!(result.unwrap_err().code, "model_cancelled");
    worker.stop().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
