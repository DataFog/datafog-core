use core::{Head, WorkspaceFinding};
use datafog_core::TextRange;
use datafog_fogvault_native::{Binding, Error, Owner, core, vault};
use std::path::PathBuf;
use vault::{Host, KdfPolicy, Limits, Vault};

const PASSWORD: &[u8] = b"synthetic-only native integration password";
fn id() -> [u8; 16] {
    let mut out = [0; 16];
    getrandom::fill(&mut out).unwrap();
    out
}
fn limits() -> Limits {
    Limits {
        max_payload_bytes: core::MAX_BUNDLE_BYTES,
        max_metadata_bytes: 88,
        max_records: 4,
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    host: Host,
    path: PathBuf,
}
impl Fixture {
    fn new() -> (Self, Vault, vault::SessionToken) {
        Self::new_with_limits(limits())
    }
    fn new_with_limits(admitted_limits: Limits) -> (Self, Vault, vault::SessionToken) {
        let temp = tempfile::tempdir().unwrap();
        // Synthetic namespaces do not qualify real OS-level control isolation.
        let host = Host::create_new(temp.path().join("control")).unwrap();
        let path = temp.path().join("vault.sqlite");
        let mut vault = host
            .create(&path, PASSWORD, admitted_limits, KdfPolicy::baseline())
            .unwrap();
        let token = vault.unlock(PASSWORD).unwrap();
        (
            Self {
                _temp: temp,
                host,
                path,
            },
            vault,
            token,
        )
    }
    fn open(&self) -> (Vault, vault::SessionToken) {
        let mut vault = self
            .host
            .open_vault(&self.path, limits(), KdfPolicy::baseline())
            .unwrap();
        let token = vault.unlock(PASSWORD).unwrap();
        (vault, token)
    }
}
fn finding(value: &str) -> WorkspaceFinding {
    WorkspaceFinding::new(
        "EMAIL",
        value,
        TextRange {
            start: 0,
            end: value.len(),
        },
    )
}
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn shared_binding_vectors_are_exact_and_u64_lossless() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/fogvault/record-binding-v1.json"
    ))
    .unwrap();
    for vector in v["valid"].as_array().unwrap() {
        let bytes = hex(vector["metadataHex"].as_str().unwrap());
        let binding = Binding::decode(&bytes).unwrap();
        let expected_id = |value: &serde_json::Value| -> [u8; 16] {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        };
        let expected = Binding {
            namespace: expected_id(&vector["namespace"]),
            record_id: expected_id(&vector["recordId"]),
            parent: Head {
                revision: vector["parent"]["revision"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
                commit_id: expected_id(&vector["parent"]["commitId"]),
            },
            saved: Head {
                revision: vector["saved"]["revision"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
                commit_id: expected_id(&vector["saved"]["commitId"]),
            },
        };
        assert_eq!(binding, expected);
        assert_eq!(expected.encode().unwrap().as_slice(), bytes);
        assert!(
            Binding {
                namespace: [0; 16],
                ..expected
            }
            .encode()
            .is_err()
        );
        assert!(
            Binding {
                record_id: [0; 16],
                ..expected
            }
            .encode()
            .is_err()
        );
        assert_eq!(
            binding.parent.revision.to_string(),
            vector["parent"]["revision"].as_str().unwrap()
        );
        assert_eq!(
            binding.saved.revision.to_string(),
            vector["saved"]["revision"].as_str().unwrap()
        );
        let array = |key: &str| {
            vector[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        };
        assert_eq!(binding.namespace.as_slice(), array("namespace"));
        assert_eq!(binding.record_id.as_slice(), array("recordId"));
    }
    for vector in v["invalid"].as_array().unwrap() {
        assert!(matches!(
            Binding::decode(&hex(vector["metadataHex"].as_str().unwrap())),
            Err(Error::BindingMismatch)
        ));
    }
}

#[test]
fn create_approve_save_lock_reopen_restore_and_lifetime() {
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let mut owner = Owner::create(vault, token, record, namespace, id(), id()).unwrap();
    let source = "alice@example.test";
    let mut change = owner.prepare(source, &[finding(source)], id()).unwrap();
    assert!(matches!(
        owner.commit(&mut change, false, id()),
        Err(Error::ApprovalRequired)
    ));
    assert_eq!(owner.record_revision().unwrap(), 1);
    let preview = change.previews().unwrap()[0].text.clone();
    assert_ne!(preview, source);
    let committed = owner.commit(&mut change, true, id()).unwrap();
    assert_eq!(committed.previews, vec![preview.clone()]);
    assert!(change.previews().is_err());
    assert_eq!(owner.restore(&preview).unwrap(), source);
    let mut pending = owner
        .prepare("next@example.test", &[finding("next@example.test")], id())
        .unwrap();
    let mut stream = owner.buffered_restore(core::RestoreEncoding::Utf8).unwrap();
    owner.lock();
    assert!(pending.previews().is_err());
    assert!(stream.feed(preview.as_bytes()).is_err());
    assert!(matches!(
        owner.commit(&mut pending, true, id()),
        Err(Error::ReopenRequired)
    ));
    drop(owner);
    let (vault, token) = fixture.open();
    let owner = Owner::open(vault, token, record, namespace, Some(committed.head)).unwrap();
    assert_eq!(owner.restore(&preview).unwrap(), source);
    assert_eq!(owner.record_revision().unwrap(), 2);
}

#[test]
fn foreign_changes_reject_before_storage_and_conflicting_writer_closes_owner() {
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let mut first = Owner::create(vault, token, record, namespace, id(), id()).unwrap();
    let (vault, token) = fixture.open();
    let mut second = Owner::open(vault, token, record, namespace, None).unwrap();
    let source = "alice@example.test";
    let mut change = first.prepare(source, &[finding(source)], id()).unwrap();
    assert!(matches!(
        second.commit(&mut change, true, id()),
        Err(Error::ForeignChange)
    ));
    assert_eq!(second.record_revision().unwrap(), 1);
    let mut stale = second
        .prepare("bob@example.test", &[finding("bob@example.test")], id())
        .unwrap();
    first.commit(&mut change, true, id()).unwrap();
    assert!(matches!(
        second.commit(&mut stale, true, id()),
        Err(Error::Vault(vault::Error::Conflict))
    ));
    assert!(stale.previews().is_err());
    assert!(matches!(second.head(), Err(Error::ReopenRequired)));
}

#[test]
fn authenticated_record_metadata_cannot_rebind_workspace_identity_or_head() {
    let (fixture, mut vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let workspace = core::Workspace::create(namespace, id()).unwrap();
    let head = workspace.head().unwrap();
    let binding = Binding {
        namespace,
        record_id: id(),
        parent: head,
        saved: head,
    };
    vault
        .save(
            &token,
            record,
            None,
            &binding.encode().unwrap(),
            &workspace.export_bundle().unwrap(),
            id(),
        )
        .unwrap();
    assert!(matches!(
        Owner::open(vault, token, record, namespace, None),
        Err(Error::BindingMismatch)
    ));
    let (mut vault, token) = fixture.open();
    let correct = Binding {
        record_id: record,
        ..binding
    };
    vault
        .save(
            &token,
            record,
            Some(1),
            &correct.encode().unwrap(),
            &workspace.export_bundle().unwrap(),
            id(),
        )
        .unwrap();
    assert!(matches!(
        Owner::open(
            vault,
            token,
            record,
            namespace,
            Some(Head {
                commit_id: id(),
                ..head
            })
        ),
        Err(Error::BindingMismatch)
    ));
    let (vault, token) = fixture.open();
    assert!(matches!(
        Owner::open(vault, token, record, id(), None),
        Err(Error::BindingMismatch)
    ));
    let (mut vault, token) = fixture.open();
    let forged = Binding {
        saved: Head {
            commit_id: id(),
            ..head
        },
        ..correct
    };
    vault
        .save(
            &token,
            record,
            Some(2),
            &forged.encode().unwrap(),
            &workspace.export_bundle().unwrap(),
            id(),
        )
        .unwrap();
    assert!(matches!(
        Owner::open(vault, token, record, namespace, None),
        Err(Error::BindingMismatch)
    ));
}

#[test]
fn published_beta3_and_candidate_bundles_interoperate_through_registered_vault() {
    let (fixture, mut vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let mut old = published_beta3::workspace::Workspace::create(namespace, id()).unwrap();
    let source = "alice@example.test";
    let f = published_beta3::workspace::WorkspaceFinding::new(
        "EMAIL",
        source,
        published_beta3::TextRange {
            start: 0,
            end: source.len(),
        },
    );
    let mut plan = old.prepare(source, &[f], id()).unwrap();
    let marker = plan.preview().unwrap().text.clone();
    let parent = plan.expected_head().unwrap();
    let saved = plan.candidate_head().unwrap();
    let binding = Binding {
        namespace,
        record_id: record,
        parent: Head {
            revision: parent.revision,
            commit_id: parent.commit_id,
        },
        saved: Head {
            revision: saved.revision,
            commit_id: saved.commit_id,
        },
    };
    vault
        .save(
            &token,
            record,
            None,
            &binding.encode().unwrap(),
            plan.export_candidate_bundle().unwrap(),
            id(),
        )
        .unwrap();
    old.activate_saved(&mut plan, parent, saved).unwrap();
    let mut owner = Owner::open(vault, token, record, namespace, None).unwrap();
    assert_eq!(owner.restore(&marker).unwrap(), source);
    let second_source = "bob@example.test";
    let mut change = owner
        .prepare(second_source, &[finding(second_source)], id())
        .unwrap();
    let second_marker = owner
        .commit(&mut change, true, id())
        .unwrap()
        .previews
        .remove(0);
    drop(owner);
    let (mut vault, token) = fixture.open();
    let saved = vault.read(&token, record).unwrap();
    let reopened_old = published_beta3::workspace::Workspace::open_bundle(&saved.payload).unwrap();
    assert_eq!(reopened_old.restore(&marker).unwrap(), source);
    assert_eq!(reopened_old.restore(&second_marker).unwrap(), second_source);
}

#[test]
fn conversation_pending_and_observed_send_survive_saved_reopen() {
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let mut owner = Owner::create(vault, token, record, namespace, id(), id()).unwrap();
    let logical_id = id();
    let source = "alice@example.test";
    let findings = [finding(source)];
    let mut plan = owner
        .prepare_conversation_send(
            &[core::ConversationInput {
                input: core::ReviewInput {
                    text: source,
                    scanner_findings: &findings,
                    user_selections: &[],
                    corrections: &[],
                },
                conversation: core::Conversation {
                    id: logical_id,
                    surface: "synthetic-local".into(),
                    site_id: None,
                    site_id_byte_budget: 40,
                },
            }],
            core::OperationContext {
                id: id(),
                occurred_at_ms: 1,
            },
        )
        .unwrap();
    let intent = plan.send_intents().unwrap()[0];
    let marker = owner
        .commit(&mut plan, true, id())
        .unwrap()
        .previews
        .remove(0);
    assert!(owner.restore_in_conversation(&marker, logical_id).is_err());
    assert!(
        matches!(owner.restore_report_in_conversation(&marker, logical_id),
        Err(Error::Core(error)) if error.code == core::ErrorCode::ConversationPending)
    );
    drop(owner);
    let (vault, token) = fixture.open();
    let mut owner = Owner::open(vault, token, record, namespace, None).unwrap();
    assert_eq!(
        owner.send_intent_status(intent).unwrap(),
        core::SendStatus::Pending
    );
    // Synthetic trusted-host observation only; no provider is contacted.
    let mut observed = owner
        .prepare_observed_send(
            intent,
            logical_id,
            "synthetic-local",
            "synthetic-chat",
            core::OperationContext {
                id: id(),
                occurred_at_ms: 2,
            },
        )
        .unwrap();
    owner.commit(&mut observed, true, id()).unwrap();
    assert_eq!(
        owner.restore_in_conversation(&marker, logical_id).unwrap(),
        source
    );
    assert_eq!(
        owner
            .restore_report_in_conversation(&marker, logical_id)
            .unwrap()
            .text
            .as_deref(),
        Some(source)
    );
    assert!(owner.restore_in_conversation(&marker, id()).is_err());
}

#[test]
fn recovery_candidate_publishes_new_head_without_overwriting_destination() {
    let source_namespace = id();
    let mut source = core::Workspace::create(source_namespace, id()).unwrap();
    let text = "alice@example.test";
    let mut change = source.prepare(text, &[finding(text)], id()).unwrap();
    let marker = change.preview().unwrap().text.clone();
    let parent = change.expected_head().unwrap();
    let saved = change.candidate_head().unwrap();
    source.activate_saved(&mut change, parent, saved).unwrap();
    let candidate = datafog_fogvault_migration::stage_authenticated_bundle(
        &source.export_bundle().unwrap(),
        source_namespace,
        saved,
        id(),
    )
    .unwrap();
    assert_ne!(candidate.candidate_head(), candidate.source_head());
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let owner = Owner::import_recovered(vault, token, record, &candidate, true, id()).unwrap();
    assert_eq!(owner.head().unwrap(), candidate.candidate_head());
    assert_eq!(owner.restore(&marker).unwrap(), text);
    drop(owner);
    let (vault, token) = fixture.open();
    assert!(matches!(
        Owner::import_recovered(vault, token, record, &candidate, true, id()),
        Err(Error::DestinationOccupied)
    ));
    let (mut vault, token) = fixture.open();
    let record = vault.read(&token, record).unwrap();
    let binding = Binding::decode(&record.metadata).unwrap();
    assert_eq!(binding.parent, candidate.source_head());
    assert_eq!(binding.saved, candidate.candidate_head());
    assert_eq!(record.revision, 1);
}

#[test]
fn create_rejects_identical_request_replay_into_existing_workspace() {
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let operation = id();
    let request = id();
    let owner = Owner::create(vault, token, record, namespace, operation, request).unwrap();
    drop(owner);
    let (vault, token) = fixture.open();
    assert!(matches!(
        Owner::create(vault, token, record, namespace, operation, request),
        Err(Error::DestinationOccupied)
    ));
    let (vault, token) = fixture.open();
    let owner = Owner::open(vault, token, record, namespace, None).unwrap();
    assert_eq!(owner.record_revision().unwrap(), 1);
}

#[test]
fn native_facade_delegates_preserve_core_reports_history_and_stream_lifetimes() {
    let (fixture, vault, token) = Fixture::new();
    let record = id();
    let namespace = id();
    let op = |occurred_at_ms| core::OperationContext {
        id: id(),
        occurred_at_ms,
    };
    let mut owner =
        Owner::create_with_operation(vault, token, record, namespace, op(1), id()).unwrap();
    assert_eq!(owner.counts().unwrap().operations, 1);
    assert_eq!(owner.operations().unwrap()[0].occurred_at_ms, Some(1));
    assert!(owner.history_coverage().unwrap().recorded_history_only);
    assert_eq!(
        owner.history_coverage().unwrap().pre_upgrade_history,
        core::PreUpgradeHistory::NotApplicable
    );
    let source = "alice@example.test";
    let mut change = owner
        .prepare_with_operation(source, &[finding(source)], op(2))
        .unwrap();
    assert_eq!(change.snapshot_format().unwrap().current_version, 3);
    assert_eq!(change.snapshot_format().unwrap().candidate_version, 3);
    assert!(change.policy_preview().unwrap().is_none());
    let marker = owner
        .commit(&mut change, true, id())
        .unwrap()
        .previews
        .remove(0);
    assert!(change.snapshot_format().is_err());
    assert_eq!(owner.counts().unwrap().operations, 2);
    assert_eq!(
        owner.operations_page(1, 1).unwrap().records[0].occurred_at_ms,
        Some(2)
    );
    assert_eq!(
        owner.restore_report(&marker).unwrap().text.as_deref(),
        Some(source)
    );
    let blocked = owner
        .restore_report(&format!("{marker} [[WRONG_000000]]"))
        .unwrap();
    assert!(blocked.text.is_none());
    assert!(!blocked.issues.is_empty());
    // An actual no-op policy save retains Core's same-head child lifetime,
    // even though FogVault persisted a newer enclosing record revision.
    let before = owner.head().unwrap();
    let record_revision = owner.record_revision().unwrap();
    let mut buffered = owner.buffered_restore(core::RestoreEncoding::Utf8).unwrap();
    let mut no_op = owner
        .prepare_policy_changes(
            &core::PolicyEdits {
                rules: vec![core::TypeRuleEdit {
                    entity_type: "EMAIL".into(),
                    rule: None,
                }],
                choices: vec![],
            },
            id(),
        )
        .unwrap();
    assert!(no_op.policy_preview().unwrap().is_some());
    owner.commit(&mut no_op, true, id()).unwrap();
    assert_eq!(owner.head().unwrap(), before);
    assert_eq!(owner.record_revision().unwrap(), record_revision + 1);
    buffered.feed(b"plain text").unwrap();
    assert_eq!(
        buffered.finalize().unwrap().text.as_deref(),
        Some("plain text")
    );
    let mut stale = owner
        .progressive_restore(core::RestoreEncoding::Utf8)
        .unwrap();
    let second = "bob@example.test";
    let findings = [finding(second)];
    let mut batch = owner
        .prepare_batch_with_operation(
            &[core::WorkspaceInput {
                text: second,
                findings: &findings,
            }],
            op(3),
        )
        .unwrap();
    owner.commit(&mut batch, true, id()).unwrap();
    assert!(
        matches!(stale.feed(b"plain"), Err(error) if error.error.code == core::ErrorCode::WorkspaceConflict)
    );
    let mut recovered = owner.prepare_recovery_with_operation(op(4)).unwrap();
    owner.commit(&mut recovered, true, id()).unwrap();
    let mut outcome = owner
        .prepare_outcome(
            core::OperationOutcome {
                kind: core::OutcomeKind::BlockedInput,
                issue_count: 1,
            },
            op(5),
        )
        .unwrap();
    owner.commit(&mut outcome, true, id()).unwrap();
    assert_eq!(owner.counts().unwrap().operations, 5);
    assert_eq!(owner.counts().unwrap().reported_blocked_attempts, 1);
    assert!(
        matches!(owner.prepare_outcome(core::OperationOutcome {kind:core::OutcomeKind::CopyConfirmed, issue_count:1}, op(6)), Err(Error::Core(error)) if error.code == core::ErrorCode::InvalidOperation)
    );
    let mut progressive = owner
        .progressive_restore(core::RestoreEncoding::Utf8)
        .unwrap();
    let input = format!("{marker} ");
    let chunk = progressive.feed(input.as_bytes()).unwrap();
    let final_chunk = progressive.finalize().unwrap();
    assert_eq!(
        format!("{}{}", chunk.text, final_chunk.text),
        format!("{source} ")
    );
    owner.lock();
    assert!(progressive.reset().is_err());
    assert!(matches!(
        owner.restore_report(&marker),
        Err(Error::ReopenRequired)
    ));
    drop(owner);
    let (vault, token) = fixture.open();
    let owner = Owner::open(vault, token, record, namespace, None).unwrap();
    assert_eq!(owner.counts().unwrap().operations, 5);
    assert_eq!(owner.restore(&marker).unwrap(), source);
}

#[test]
fn native_payload_admission_failure_preserves_saved_state_without_handoff() {
    let admitted = Limits {
        max_payload_bytes: 512,
        max_metadata_bytes: 88,
        max_records: 4,
    };
    let (fixture, vault, token) = Fixture::new_with_limits(admitted);
    let record = id();
    let namespace = id();
    let mut owner = Owner::create(vault, token, record, namespace, id(), id()).unwrap();
    let source = "alice@example.test";
    let mut initial = owner.prepare(source, &[finding(source)], id()).unwrap();
    let saved = owner.commit(&mut initial, true, id()).unwrap();
    let marker = saved.previews[0].clone();
    let prior_revision = owner.record_revision().unwrap();
    let huge = format!("{}@example.test", "a".repeat(2048));
    let mut oversized = owner.prepare(&huge, &[finding(&huge)], id()).unwrap();
    assert!(oversized.candidate_bytes().unwrap() > admitted.max_payload_bytes);
    assert!(matches!(
        owner.commit(&mut oversized, true, id()),
        Err(Error::Vault(vault::Error::ResourceLimit))
    ));
    assert!(oversized.previews().is_err());
    assert!(matches!(owner.head(), Err(Error::ReopenRequired)));
    drop(owner);
    let mut vault = fixture
        .host
        .open_vault(&fixture.path, admitted, KdfPolicy::baseline())
        .unwrap();
    let token = vault.unlock(PASSWORD).unwrap();
    let reopened = Owner::open(vault, token, record, namespace, Some(saved.head)).unwrap();
    assert_eq!(reopened.record_revision().unwrap(), prior_revision);
    assert_eq!(reopened.restore(&marker).unwrap(), source);
}
