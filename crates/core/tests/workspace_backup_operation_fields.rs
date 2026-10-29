use datafog_core::workspace::{ErrorCode, OperationContext, OperationKind, Workspace};
use minicbor::Decoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/workspace/backup-operation-fields.json"
    ))
    .unwrap()
}
fn bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fields(bytes: &[u8]) -> BTreeMap<u8, Vec<u8>> {
    let mut decoder = Decoder::new(bytes);
    let count = decoder.map().unwrap().unwrap();
    let mut result = BTreeMap::new();
    for _ in 0..count {
        let key = decoder.u8().unwrap();
        let start = decoder.position();
        decoder.skip().unwrap();
        assert!(
            result
                .insert(key, bytes[start..decoder.position()].to_vec())
                .is_none()
        );
    }
    assert_eq!(decoder.position(), bytes.len());
    result
}
fn rows(workspace: &Workspace) -> Value {
    json!(workspace.operations().unwrap().iter().map(|record|json!({"id":record.id,"kind":record.kind.as_str(),"occurredAtMs":record.occurred_at_ms,
        "preparedInputs":record.prepared_inputs,"transformedSpans":record.transformed_spans})).collect::<Vec<_>>())
}
fn counts(workspace: &Workspace) -> Value {
    let value = workspace.counts().unwrap();
    json!({"operations":value.operations,"preparedInputs":value.prepared_inputs,"transformedSpans":value.transformed_spans,
        "confirmedHandoffs":value.confirmed_handoffs,"reportedBlockedAttempts":value.reported_blocked_attempts})
}
#[test]
fn current_v3_gold_is_actual_nonempty_history_with_derived_coverage_and_original_fields() {
    let fixture = fixture();
    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(fixture["syntheticOnly"], true);
    let hash: String = Sha256::digest(include_bytes!(
        "../examples/workspace_backup_operation_fields.rs"
    ))
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
    assert_eq!(fixture["source"]["generatorSha256"], hash);
    for name in ["current", "legacyUpgraded"] {
        let gold = &fixture[name];
        let raw = bytes(gold["bundleHex"].as_str().unwrap());
        let workspace = Workspace::open_bundle(&raw).unwrap();
        assert_eq!(workspace.export_bundle().unwrap(), raw);
        assert_eq!(workspace.policy().unwrap().snapshot_version, 3);
        assert_eq!(rows(&workspace), gold["operations"]);
        assert_eq!(counts(&workspace), gold["counts"]);
        assert_eq!(
            serde_json::to_value(workspace.history_coverage().unwrap()).unwrap(),
            gold["historyCoverage"]
        );
        let mut decoder = Decoder::new(&raw);
        assert_eq!(decoder.map().unwrap(), Some(10));
        let mut keys = Vec::new();
        for span in gold["fields"].as_array().unwrap() {
            let key = decoder.u8().unwrap();
            keys.push(key);
            assert_eq!(u64::from(key), span["key"].as_u64().unwrap());
            assert_eq!(
                decoder.position() as u64,
                span["valueStart"].as_u64().unwrap()
            );
            decoder.skip().unwrap();
            assert_eq!(
                decoder.position() as u64,
                span["valueEnd"].as_u64().unwrap()
            );
        }
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 10]);
        assert_eq!(decoder.position(), raw.len());
        for alias in gold["aliases"].as_array().unwrap() {
            assert_eq!(
                workspace
                    .restore(alias["marker"].as_str().unwrap())
                    .unwrap(),
                alias["value"]
            );
        }
    }
    let current = &fixture["current"];
    assert_eq!(
        current["counts"],
        json!({"operations":9,"preparedInputs":3,"transformedSpans":5,"confirmedHandoffs":0,"reportedBlockedAttempts":2})
    );
    let kinds: BTreeSet<_> = current["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from([
            "create",
            "prepare",
            "policy_change",
            "copy_confirmed",
            "user_cancelled",
            "blocked_input",
            "restore_blocked",
            "recovery"
        ])
    );
    assert_eq!(current["operations"][0]["occurredAtMs"], 0);
    assert!(current["operations"][2]["occurredAtMs"].is_null());
    assert_eq!(current["operations"][3]["occurredAtMs"], 253402300799999u64);
    assert!(current["operations"][8]["occurredAtMs"].is_null());
    assert_eq!(fixture["legacyUpgraded"]["counts"]["operations"], 1);
    assert_eq!(
        fixture["legacyUpgraded"]["historyCoverage"]["preUpgradeHistory"],
        "unknown"
    );
}
#[test]
fn recovery_retains_every_row_delta_and_legacy_coverage_without_activating_preview() {
    let fixture = fixture();
    for name in ["current", "legacyUpgraded"] {
        let gold = &fixture[name];
        let raw = bytes(gold["bundleHex"].as_str().unwrap());
        let mut workspace = Workspace::open_bundle(&raw).unwrap();
        let old_rows = workspace.operations().unwrap().to_vec();
        let old_counts = workspace.counts().unwrap();
        let old_head = workspace.head().unwrap();
        let coverage = workspace.history_coverage().unwrap();
        let old_policy = serde_json::to_value(workspace.policy().unwrap()).unwrap();
        let mut abandoned = workspace.prepare_recovery([111; 16]).unwrap();
        abandoned.export_candidate_bundle().unwrap();
        abandoned.discard();
        assert_eq!(workspace.export_bundle().unwrap(), raw);
        assert_eq!(workspace.counts().unwrap(), old_counts);
        let mut recovery = workspace.prepare_recovery([111; 16]).unwrap();
        assert!(recovery.previews().unwrap().is_empty());
        let expected = recovery.expected_head().unwrap();
        let saved = recovery.candidate_head().unwrap();
        assert_eq!(expected, old_head);
        assert_eq!(saved.revision, old_head.revision + 1);
        assert_ne!(saved.commit_id, old_head.commit_id);
        let staged = Workspace::open_bundle(recovery.export_candidate_bundle().unwrap()).unwrap();
        assert_eq!(staged.operations().unwrap()[..old_rows.len()], old_rows);
        assert_eq!(workspace.export_bundle().unwrap(), raw);
        assert_eq!(workspace.counts().unwrap(), old_counts);
        assert_eq!(
            workspace
                .activate_saved(&mut recovery, expected, old_head)
                .unwrap_err()
                .code,
            ErrorCode::WorkspaceConflict
        );
        assert_eq!(workspace.export_bundle().unwrap(), raw);
        workspace
            .activate_saved(&mut recovery, expected, saved)
            .unwrap();
        let recovered = workspace.export_bundle().unwrap();
        let before_fields = fields(&raw);
        let after_fields = fields(&recovered);
        for key in [0, 1, 3, 4, 5, 6, 7] {
            assert_eq!(
                after_fields[&key], before_fields[&key],
                "{name} retained field {key}"
            );
        }
        assert_eq!(workspace.operations().unwrap()[..old_rows.len()], old_rows);
        let last = workspace.operations().unwrap().last().unwrap();
        assert_eq!(last.kind, OperationKind::Recovery);
        assert_eq!(last.id, saved.commit_id);
        assert_eq!(last.occurred_at_ms, None);
        let after_counts = workspace.counts().unwrap();
        assert_eq!(after_counts.operations, old_counts.operations + 1);
        assert_eq!(after_counts.prepared_inputs, old_counts.prepared_inputs);
        assert_eq!(after_counts.transformed_spans, old_counts.transformed_spans);
        assert_eq!(
            after_counts.confirmed_handoffs,
            old_counts.confirmed_handoffs
        );
        assert_eq!(
            after_counts.reported_blocked_attempts,
            old_counts.reported_blocked_attempts
        );
        assert_eq!(workspace.history_coverage().unwrap(), coverage);
        assert_eq!(
            serde_json::to_value(workspace.policy().unwrap()).unwrap(),
            old_policy
        );
        let retained = old_rows
            .iter()
            .find(|row| row.kind == OperationKind::UserCancelled)
            .unwrap()
            .id;
        assert_eq!(
            workspace
                .prepare("ordinary", &[], retained)
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidId
        );
        assert_eq!(
            workspace
                .prepare_with_operation(
                    "ordinary",
                    &[],
                    OperationContext {
                        id: retained,
                        occurred_at_ms: 0
                    }
                )
                .err()
                .unwrap()
                .code,
            ErrorCode::OperationReused
        );
        let reopened = Workspace::open_bundle(&recovered).unwrap();
        assert_eq!(rows(&reopened), rows(&workspace));
        assert_eq!(counts(&reopened), counts(&workspace));
        for alias in gold["aliases"].as_array().unwrap() {
            assert_eq!(
                reopened.restore(alias["marker"].as_str().unwrap()).unwrap(),
                alias["value"]
            );
        }
    }
}
#[test]
fn missing_or_inconsistent_operation_fields_never_import_as_empty_success() {
    let fixture = fixture();
    let mut names = BTreeSet::new();
    let valid = bytes(fixture["current"]["bundleHex"].as_str().unwrap());
    let workspace = Workspace::open_bundle(&valid).unwrap();
    for vector in fixture["rejections"].as_array().unwrap() {
        let name = vector["id"].as_str().unwrap();
        assert!(names.insert(name));
        let error = Workspace::open_bundle(&bytes(vector["bundleHex"].as_str().unwrap()))
            .err()
            .unwrap();
        assert_eq!(error.code.as_str(), vector["code"], "{name}");
        assert_eq!(error.to_string(), "invalid_snapshot");
        assert!(error.marker_issues.is_empty() && error.review_conflicts.is_empty());
        assert!(error.input_index.is_none() && error.finding_index.is_none());
    }
    for member in 0..5 {
        assert!(names.contains(format!("missing_operation_member_{member}").as_str()));
        assert!(names.contains(format!("missing_count_member_{member}").as_str()));
    }
    for required in [
        "missing_top_key_8",
        "missing_top_key_10",
        "duplicate_operation_id",
        "unimplemented_handoff_total",
        "mapping_issuance_exceeds_recorded_spans",
        "operation_count_above_bound",
    ] {
        assert!(names.contains(required));
    }
    assert_eq!(names.len(), 32);
    assert_eq!(workspace.export_bundle().unwrap(), valid);
}
