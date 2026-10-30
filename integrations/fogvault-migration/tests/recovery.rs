use datafog_core::workspace::{ErrorCode, Head, SendIntentId, Workspace};
use datafog_fogvault_migration::{MigrationError, stage_authenticated_bundle};
use serde_json::Value;

fn unhex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn check_source_unchanged(raw: &[u8]) {
    let source = Workspace::open_bundle(raw).unwrap();
    let before = source.export_bundle().unwrap();
    let parent = source.head().unwrap();
    let candidate =
        stage_authenticated_bundle(raw, source.namespace().unwrap(), parent, [201; 16]).unwrap();
    assert_eq!(source.export_bundle().unwrap(), before);
    assert_eq!(before, raw);
    assert_eq!(candidate.source_head(), parent);
    assert_eq!(candidate.namespace(), source.namespace().unwrap());
    assert_eq!(candidate.candidate_head().revision, parent.revision + 1);
    assert_eq!(candidate.candidate_head().commit_id, [201; 16]);
    let recovered = Workspace::open_bundle(candidate.bundle()).unwrap();
    assert_eq!(recovered.export_bundle().unwrap(), candidate.bundle());
    assert_eq!(recovered.head().unwrap(), candidate.candidate_head());
}

#[test]
fn v1_source_stays_exact_while_recovery_upgrades_through_core() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    let raw = unhex(fixture["high"]["bundleHex"].as_str().unwrap());
    check_source_unchanged(&raw);
    let source = Workspace::open_bundle(&raw).unwrap();
    let candidate = stage_authenticated_bundle(
        &raw,
        source.namespace().unwrap(),
        source.head().unwrap(),
        [201; 16],
    )
    .unwrap();
    assert_eq!(candidate.snapshot_format().current_version, 1);
    assert_eq!(candidate.snapshot_format().candidate_version, 3);
    let recovered = Workspace::open_bundle(candidate.bundle()).unwrap();
    assert_eq!(
        recovered
            .restore(fixture["high"]["marker"].as_str().unwrap())
            .unwrap(),
        "Jane"
    );
    assert_eq!(candidate.source_head().revision, 9_007_199_254_740_993);
}

#[test]
fn v2_policy_and_aliases_keep_their_saved_meaning() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/workspace/backup-current-fields.json"
    ))
    .unwrap();
    let gold = &fixture["current"];
    let raw = unhex(gold["bundleHex"].as_str().unwrap());
    check_source_unchanged(&raw);
    let source = Workspace::open_bundle(&raw).unwrap();
    let candidate = stage_authenticated_bundle(
        &raw,
        source.namespace().unwrap(),
        source.head().unwrap(),
        [201; 16],
    )
    .unwrap();
    let recovered = Workspace::open_bundle(candidate.bundle()).unwrap();
    let mut policy = serde_json::to_value(recovered.policy().unwrap()).unwrap();
    let source_policy = serde_json::to_value(source.policy().unwrap()).unwrap();
    policy["snapshotVersion"] = source_policy["snapshotVersion"].clone();
    assert_eq!(policy, source_policy);
    for alias in gold["aliases"].as_array().unwrap() {
        assert_eq!(
            recovered
                .restore(alias["marker"].as_str().unwrap())
                .unwrap(),
            alias["value"]
        );
    }
}

#[test]
fn v3_recovery_preserves_history_and_pending_or_confirmed_conversation_authority() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/workspace/backup-operation-fields.json"
    ))
    .unwrap();
    for name in [
        "current",
        "legacyUpgraded",
        "conversationPending",
        "conversationResolved",
    ] {
        let gold = &fixture[name];
        let raw = unhex(gold["bundleHex"].as_str().unwrap());
        check_source_unchanged(&raw);
        let source = Workspace::open_bundle(&raw).unwrap();
        let candidate = stage_authenticated_bundle(
            &raw,
            source.namespace().unwrap(),
            source.head().unwrap(),
            [201; 16],
        )
        .unwrap();
        let recovered = Workspace::open_bundle(candidate.bundle()).unwrap();
        assert_eq!(
            serde_json::to_value(recovered.policy().unwrap()).unwrap(),
            serde_json::to_value(source.policy().unwrap()).unwrap()
        );
        let previous = source.operations().unwrap();
        assert_eq!(&recovered.operations().unwrap()[..previous.len()], previous);
        assert_eq!(recovered.operations().unwrap().len(), previous.len() + 1);
        for alias in gold["aliases"].as_array().unwrap() {
            assert_eq!(
                recovered
                    .restore(alias["marker"].as_str().unwrap())
                    .unwrap(),
                alias["value"]
            );
        }
        if let Some(conversation) = gold.get("conversation") {
            let logical_id = serde_json::from_value(conversation["logicalId"].clone()).unwrap();
            let intent = SendIntentId {
                preparation_id: serde_json::from_value(
                    conversation["intent"]["preparationId"].clone(),
                )
                .unwrap(),
                input_index: conversation["intent"]["inputIndex"].as_u64().unwrap() as usize,
            };
            assert_eq!(
                serde_json::to_value(recovered.conversation(logical_id).unwrap()).unwrap(),
                conversation["state"]
            );
            assert_eq!(
                serde_json::to_value(recovered.send_intent_status(intent).unwrap()).unwrap(),
                conversation["status"]
            );
            if conversation["status"] == "pending" {
                assert_eq!(
                    recovered
                        .restore_in_conversation("plain", logical_id)
                        .unwrap_err()
                        .code,
                    ErrorCode::ConversationPending
                );
            }
        }
    }
}

#[test]
fn namespace_revision_and_commit_id_are_all_required() {
    let source = Workspace::create([1; 16], [2; 16]).unwrap();
    let raw = source.export_bundle().unwrap();
    let head = source.head().unwrap();
    for (namespace, claimed) in [
        ([9; 16], head),
        (
            [1; 16],
            Head {
                revision: 1,
                ..head
            },
        ),
        (
            [1; 16],
            Head {
                commit_id: [9; 16],
                ..head
            },
        ),
    ] {
        assert_eq!(
            stage_authenticated_bundle(&raw, namespace, claimed, [3; 16])
                .err()
                .unwrap(),
            MigrationError::SourceBindingMismatch
        );
    }
    assert_eq!(source.export_bundle().unwrap(), raw);
}

#[test]
fn malformed_and_unsupported_sources_never_become_candidates() {
    let source = Workspace::create([1; 16], [2; 16]).unwrap();
    let raw = source.export_bundle().unwrap();
    let mut future = raw.clone();
    // Canonical map starts with snapshot key 0 and its one-byte version.
    assert_eq!(future[1], 0);
    future[2] = 4;
    for bytes in [
        Vec::new(),
        raw[..raw.len() - 1].to_vec(),
        future,
        br#"{"payload":{"format":"datafog-workspace-backup","version":2}}"#.to_vec(),
        b"age-encryption.org/v1\n-> X25519 synthetic\n".to_vec(),
    ] {
        assert!(matches!(
            stage_authenticated_bundle(&bytes, [1; 16], source.head().unwrap(), [3; 16]),
            Err(MigrationError::Core(_))
        ));
    }
}

#[test]
fn exhausted_or_reused_recovery_authority_fails_without_changing_source() {
    let source = Workspace::create([1; 16], [2; 16]).unwrap();
    let raw = source.export_bundle().unwrap();
    assert!(matches!(
        stage_authenticated_bundle(&raw, [1; 16], source.head().unwrap(), [2; 16]),
        Err(MigrationError::Core(_))
    ));
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    let maximum = unhex(fixture["maximum"]["bundleHex"].as_str().unwrap());
    let exhausted = Workspace::open_bundle(&maximum).unwrap();
    assert!(matches!(
        stage_authenticated_bundle(
            &maximum,
            exhausted.namespace().unwrap(),
            exhausted.head().unwrap(),
            [201; 16]
        ),
        Err(MigrationError::Core(_))
    ));
    assert_eq!(exhausted.export_bundle().unwrap(), maximum);
}
