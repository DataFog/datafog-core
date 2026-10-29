use datafog_core::{
    TextRange,
    workspace::{ErrorCode, Workspace, WorkspaceFinding},
};
use minicbor::Decoder;
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/workspace/backup-current-fields.json"
    ))
    .unwrap()
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn counters(bytes: &[u8]) -> Vec<(String, u64)> {
    let mut decoder = Decoder::new(bytes);
    let length = decoder.map().unwrap().unwrap();
    for _ in 0..length {
        let key = decoder.u8().unwrap();
        if key == 4 {
            let length = decoder.array().unwrap().unwrap();
            return (0..length)
                .map(|_| {
                    assert_eq!(decoder.array().unwrap(), Some(2));
                    (decoder.str().unwrap().to_owned(), decoder.u64().unwrap())
                })
                .collect();
        }
        decoder.skip().unwrap();
    }
    panic!("counter field missing")
}
#[test]
fn representative_current_snapshot_preserves_every_supported_field_and_live_identity() {
    let fixture = fixture();
    let current = &fixture["current"];
    let bytes = unhex(current["bundleHex"].as_str().unwrap());
    let mut workspace = Workspace::open_bundle(&bytes).unwrap();
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    assert_eq!(
        workspace.head().unwrap().revision.to_string(),
        current["head"]["revision"]
    );
    assert_eq!(workspace.namespace().unwrap(), [41; 16]);
    let mut decoder = Decoder::new(&bytes);
    assert_eq!(decoder.map().unwrap(), Some(8));
    for declared in current["fields"].as_array().unwrap() {
        assert_eq!(
            u64::from(decoder.u8().unwrap()),
            declared["key"].as_u64().unwrap()
        );
        assert_eq!(
            decoder.position() as u64,
            declared["valueStart"].as_u64().unwrap()
        );
        decoder.skip().unwrap();
        assert_eq!(
            decoder.position() as u64,
            declared["valueEnd"].as_u64().unwrap()
        );
    }
    assert_eq!(decoder.position(), bytes.len());
    let view = serde_json::to_value(workspace.policy().unwrap()).unwrap();
    for key in ["snapshotVersion", "defaultRule", "rules", "choices"] {
        assert_eq!(view[key], current["policy"][key], "{key}");
    }
    for alias in current["aliases"].as_array().unwrap() {
        assert_eq!(
            workspace
                .restore(alias["marker"].as_str().unwrap())
                .unwrap(),
            alias["value"]
        );
    }
    assert_eq!(
        counters(&bytes),
        vec![("ACCOUNT_NUMBER".into(), 1), ("PERSON".into(), 2)]
    );
    let mut recovery = workspace.prepare_recovery([47; 16]).unwrap();
    let parent = recovery.expected_head().unwrap();
    let saved = recovery.candidate_head().unwrap();
    assert_ne!(parent, saved);
    workspace
        .activate_saved(&mut recovery, parent, saved)
        .unwrap();
    let recovered = workspace.export_bundle().unwrap();
    assert_eq!(counters(&recovered), counters(&bytes));
    assert_eq!(
        serde_json::to_value(workspace.policy().unwrap()).unwrap(),
        view
    );
    for operation in [43, 44, 45, 46] {
        assert_eq!(
            workspace
                .prepare_recovery([operation; 16])
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidId,
            "currently retained issuance/policy operation {operation}"
        );
    }
    let mut next = workspace
        .prepare(
            "Third Person",
            &[WorkspaceFinding::new(
                "PERSON",
                "Third Person",
                TextRange { start: 0, end: 12 },
            )],
            [48; 16],
        )
        .unwrap();
    let next_bytes = next.export_candidate_bundle().unwrap();
    assert_eq!(
        counters(next_bytes),
        vec![("ACCOUNT_NUMBER".into(), 1), ("PERSON".into(), 3)]
    );
    next.discard();
    assert_eq!(workspace.export_bundle().unwrap(), recovered);
}
#[test]
fn missing_supported_fields_and_nonempty_reserved_merge_state_never_silently_import() {
    let fixture = fixture();
    let mut names = BTreeSet::new();
    for case in fixture["rejections"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        assert!(names.insert(id));
        let failure = Workspace::open_bundle(&unhex(case["bundleHex"].as_str().unwrap()))
            .err()
            .unwrap();
        assert_eq!(failure.code.as_str(), case["code"], "{id}");
        assert_eq!(failure.to_string(), "invalid_snapshot");
        assert!(failure.marker_issues.is_empty() && failure.review_conflicts.is_empty());
    }
    for key in 0..8 {
        assert!(names.contains(format!("missing_top_key_{key}").as_str()));
    }
    assert!(names.contains("reserved_key5_nonempty_array"));
    assert!(names.contains("reserved_key5_oversized_array"));
}
#[test]
fn legacy_snapshot_and_reserved_empty_slot_stay_byte_exact_until_saved_upgrade() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/workspace/counters.json")).unwrap();
    let legacy = &fixture["high"];
    let bytes = unhex(legacy["bundleHex"].as_str().unwrap());
    let workspace = Workspace::open_bundle(&bytes).unwrap();
    assert_eq!(workspace.export_bundle().unwrap(), bytes);
    assert_eq!(
        workspace
            .restore(legacy["marker"].as_str().unwrap())
            .unwrap(),
        "Jane"
    );
    assert_eq!(workspace.policy().unwrap().snapshot_version, 1);
    let mut noop = workspace.prepare("ordinary", &[], [49; 16]).unwrap();
    assert_eq!(noop.export_candidate_bundle().unwrap(), bytes);
    noop.discard();
    assert_eq!(
        counters(&bytes),
        vec![("PERSON".into(), 9_007_199_254_740_993)]
    );
    let state = json!({"revision":workspace.head().unwrap().revision.to_string()});
    assert_eq!(state["revision"], legacy["revision"]);
}
