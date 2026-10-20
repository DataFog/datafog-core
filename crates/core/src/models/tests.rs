use super::*;

pub(super) fn descriptor() -> ModelSessionDescriptor {
    ModelSessionDescriptor {
        backend: "datafog_native".into(),
        model: "DataFog/pii-en-65m".into(),
        adapter_version: ADAPTER_VERSION.into(),
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
        score_scale: "normalized_token_softmax_min".into(),
        thresholds: BTreeMap::new(),
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
fn results_keep_scores_and_reindex_duplicate_evidence() {
    let session = ModelSession {
        descriptor: descriptor(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
        admitted: Arc::new(AtomicBool::new(false)),
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
    entries.push(entries[0].clone());
    let result = session.compose(entries).unwrap();
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.evidence[0].sources.len(), 2);
    assert_eq!(result.evidence[0].finding_index, 0);
    assert_eq!(result.model, "DataFog/pii-en-65m");
    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(session.state.load(Ordering::Acquire), 3);
    assert_eq!(
        session.scan("", &Cancellation::default()).unwrap_err().code,
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
    for removed in ["rules", "structured"] {
        let mut bad = valid.clone();
        bad[removed] = serde_json::json!({});
        let error = parse_config(&bad).err().unwrap();
        assert_eq!(error.reason, "unknown_field");
        assert_eq!(error.path, format!("/{removed}"));
    }
}

#[test]
fn busy_and_oversized_calls_fail_without_rule_only_fallback() {
    let session = ModelSession {
        descriptor: descriptor(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
        admitted: Arc::new(AtomicBool::new(false)),
    };
    let held = session.inner.lock().unwrap();
    assert_eq!(
        session
            .scan("may@example.test", &Cancellation::default())
            .unwrap_err()
            .code,
        "model_busy"
    );
    drop(held);
    let text = "x".repeat(session.descriptor.limits.max_input_bytes + 1);
    assert_eq!(
        session
            .scan(&text, &Cancellation::default())
            .unwrap_err()
            .reason,
        "limit_exceeded"
    );
    assert_eq!(session.state.load(Ordering::Acquire), 1);
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

#[test]
fn admission_precedes_scheduling_and_expired_or_cancelled_requests_do_not_infer() {
    let session = ModelSession {
        descriptor: descriptor(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
        admitted: Arc::new(AtomicBool::new(false)),
    };
    let mut request = session.prepare_request().unwrap();
    assert_eq!(session.prepare_request().err().unwrap().code, "model_busy");
    request.deadline = Instant::now();
    assert_eq!(
        session
            .scan_model_request(&request, "secret", &Cancellation::default())
            .unwrap_err()
            .code,
        "model_timeout"
    );
    // Queued work never touched the worker; the session remains reusable.
    assert_eq!(session.state.load(Ordering::Acquire), 1);
    drop(request);
    let request = session.prepare_request().unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        session
            .scan_model_request(&request, "secret", &cancellation)
            .unwrap_err()
            .code,
        "model_cancelled"
    );
    drop(request);
    let next = session.prepare_request().unwrap();
    session.request_close();
    assert_eq!(
        session
            .scan_model_request(&next, "", &Cancellation::default())
            .unwrap_err()
            .code,
        "model_cancelled"
    );
    drop(next);
    session.close().unwrap();
}

#[test]
fn native_seven_label_mapping_preserves_names_and_identifier_meaning() {
    for (native, canonical) in [
        ("first_name", "FIRST_NAME"),
        ("last_name", "LAST_NAME"),
        ("email", "EMAIL"),
        ("phone_number", "PHONE"),
        ("street_address", "STREET_ADDRESS"),
        ("customer_id", "CUSTOMER_ID"),
        ("account_number", "ACCOUNT_NUMBER"),
    ] {
        let mut candidate = span(0, 6);
        candidate.native_label = native.into();
        let result = convert(
            "sample",
            BackendResult {
                spans: vec![candidate],
            },
            &descriptor(),
        )
        .unwrap();
        assert_eq!(result[0].0.entity_type, canonical);
        assert_eq!(result[0].1.native_label.as_deref(), Some(native));
    }
}

fn idle_session() -> ModelSession {
    ModelSession {
        descriptor: descriptor(),
        inner: Mutex::new(SessionInner { worker: None }),
        closing: AtomicBool::new(false),
        state: AtomicU8::new(1),
        cleanup_failed: Arc::new(AtomicBool::new(false)),
        admitted: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn routed_rule_entities_never_touch_the_model_worker() {
    let session = idle_session();
    let config = session
        .routed_config(&serde_json::json!({"entities": ["CREDIT_CARD", "EMAIL"]}))
        .unwrap();
    assert!(!config.needs_model());
    let text = "Card 4111 1111 1111 1111, mail may@example.test, call (212) 555-0100.";
    // Another request holds the worker and the admission flag; rule-only
    // routing neither waits for them nor reports `model_busy`.
    let held = session.inner.lock().unwrap();
    let request = session.prepare_request().unwrap();
    let result = session
        .scan_routed(text, &config, &Cancellation::default())
        .unwrap();
    let found: Vec<_> = result
        .findings
        .iter()
        .map(|finding| (finding.entity_type.as_str(), finding.matched_text.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("CREDIT_CARD", "4111 1111 1111 1111"),
            ("EMAIL", "may@example.test")
        ]
    );
    assert_eq!(result.evidence.len(), 2);
    for (index, evidence) in result.evidence.iter().enumerate() {
        assert_eq!(evidence.finding_index, index);
        assert!(
            evidence.sources[0]
                .detector_name
                .starts_with("datafog-core/")
        );
        assert!(evidence.sources[0].raw_score.is_none());
    }
    assert_eq!(result.model, "DataFog/pii-en-65m");
    drop(request);
    drop(held);
}

#[test]
fn routed_scans_keep_the_session_lifecycle_and_limits() {
    let session = idle_session();
    let config = session
        .routed_config(&serde_json::json!({"entities": ["EMAIL"]}))
        .unwrap();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert_eq!(
        session
            .scan_routed("may@example.test", &config, &cancelled)
            .unwrap_err()
            .code,
        "model_cancelled"
    );
    let oversized = "x".repeat(session.descriptor.limits.max_input_bytes + 1);
    assert_eq!(
        session
            .scan_routed(&oversized, &config, &Cancellation::default())
            .unwrap_err()
            .reason,
        "limit_exceeded"
    );
    session.state.store(2, Ordering::Release);
    assert_eq!(
        session
            .scan_routed("may@example.test", &config, &Cancellation::default())
            .unwrap_err()
            .code,
        "model_session_failed"
    );
    let session = idle_session();
    session.close().unwrap();
    assert_eq!(
        session
            .scan_routed("may@example.test", &config, &Cancellation::default())
            .unwrap_err()
            .code,
        "model_session_closed"
    );
    // A model-routed configuration must arrive with its reserved request.
    let session = idle_session();
    let model = session
        .routed_config(&serde_json::json!({"entities": ["FIRST_NAME"]}))
        .unwrap();
    assert_eq!(
        session
            .scan_routed_request(None, "Jane", &model, &Cancellation::default())
            .unwrap_err()
            .path,
        "/request"
    );
}

#[test]
fn routed_configuration_splits_entities_between_rules_and_model() {
    let session = idle_session();
    let mixed = session
        .routed_config(&serde_json::json!({"entities": ["FIRST_NAME", "EMAIL", "CREDIT_CARD"]}))
        .unwrap();
    assert!(mixed.needs_model());
    assert_eq!(
        mixed.model_entities,
        BTreeSet::from(["FIRST_NAME".to_owned()])
    );
    assert_eq!(
        mixed.rules.as_ref().and_then(ScanConfig::entities),
        Some(&BTreeSet::from([
            "CREDIT_CARD".to_owned(),
            "EMAIL".to_owned()
        ]))
    );
    // Shared labels such as EMAIL and PHONE go to rules; without a selection
    // the model returns only the labels rules cannot produce.
    let everything = session.routed_config(&serde_json::json!({})).unwrap();
    assert!(everything.rules.as_ref().unwrap().entities().is_none());
    assert_eq!(
        everything.model_entities,
        BTreeSet::from(
            [
                "ACCOUNT_NUMBER",
                "CUSTOMER_ID",
                "FIRST_NAME",
                "LAST_NAME",
                "STREET_ADDRESS"
            ]
            .map(String::from)
        )
    );
    // Omitting the configuration routes exactly like an empty configuration.
    let default = session.default_routed_config();
    assert_eq!(default.model_entities, everything.model_entities);
    assert_eq!(default.rules, everything.rules);
    let model_only = session
        .routed_config(&serde_json::json!({"entities": ["STREET_ADDRESS"], "locale": "de"}))
        .unwrap();
    assert!(model_only.rules.is_none());
    for (config, reason, path) in [
        (serde_json::json!([]), "invalid_type", ""),
        (
            serde_json::json!({"entities": []}),
            "empty_value",
            "/entities",
        ),
        (
            serde_json::json!({"entities": "EMAIL"}),
            "invalid_type",
            "/entities",
        ),
        (
            serde_json::json!({"entities": ["EMAIL", 1]}),
            "invalid_type",
            "/entities/1",
        ),
        (
            serde_json::json!({"entities": ["EMAIL", "EMAIL"]}),
            "duplicate_value",
            "/entities/1",
        ),
        (
            serde_json::json!({"entities": ["EMAIL", "CARD_NUMBER"]}),
            "invalid_value",
            "/entities/1",
        ),
        (
            serde_json::json!({"entities": ["STREET_ADDRESS"], "locale": "xx"}),
            "invalid_value",
            "/locale",
        ),
        (
            serde_json::json!({"entities": ["EMAIL"], "unknown": true}),
            "unknown_field",
            "/unknown",
        ),
    ] {
        let error = session.routed_config(&config).unwrap_err();
        assert_eq!(error.code, "invalid_configuration", "{config}");
        assert_eq!(
            (error.reason.as_str(), error.path.as_str()),
            (reason, path),
            "{config}"
        );
    }
}
