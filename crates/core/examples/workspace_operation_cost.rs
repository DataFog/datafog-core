//! Synthetic cost receipt for the unapproved operations proposal, not a launch gate.
use datafog_core::workspace::{
    MAX_OPERATIONS, OperationContext, OperationKind, OperationRecord, Workspace,
};
use minicbor::Encoder;
use serde_json::json;
use std::{hint::black_box, mem::size_of, time::Instant};
fn id(value: u128) -> [u8; 16] {
    value.to_be_bytes()
}
fn snapshot(records: &[OperationRecord]) -> Vec<u8> {
    let last = records.last().unwrap();
    let mut e = Encoder::new(Vec::new());
    e.map(10)
        .unwrap()
        .u8(0)
        .unwrap()
        .u8(3)
        .unwrap()
        .u8(1)
        .unwrap()
        .bytes(&[1; 16])
        .unwrap()
        .u8(2)
        .unwrap()
        .array(2)
        .unwrap()
        .u64(records.len() as u64 - 1)
        .unwrap()
        .bytes(&last.id)
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
        .str("protected_v1")
        .unwrap()
        .u8(1)
        .unwrap()
        .array(0)
        .unwrap()
        .u8(7)
        .unwrap()
        .array(0)
        .unwrap()
        .u8(8)
        .unwrap()
        .array(records.len() as u64)
        .unwrap();
    for record in records {
        e.array(5)
            .unwrap()
            .bytes(&record.id)
            .unwrap()
            .u8(if record.kind == OperationKind::Create {
                0
            } else {
                1
            })
            .unwrap();
        if let Some(time) = record.occurred_at_ms {
            e.u64(time).unwrap();
        } else {
            e.null().unwrap();
        }
        e.u64(record.prepared_inputs)
            .unwrap()
            .u64(record.transformed_spans)
            .unwrap();
    }
    e.u8(10)
        .unwrap()
        .array(5)
        .unwrap()
        .u64(records.len() as u64)
        .unwrap()
        .u64(records.iter().map(|r| r.prepared_inputs).sum())
        .unwrap()
        .u64(records.iter().map(|r| r.transformed_spans).sum())
        .unwrap()
        .u64(0)
        .unwrap()
        .u64(0)
        .unwrap();
    e.into_writer()
}
fn main() {
    let mut results = Vec::new();
    for maximum in [false, true] {
        for count in [
            100, 1000, 9999, 10000, 30000, 99999, 100000, 299999, 300000, 500000,
        ] {
            let records: Vec<_> = (0..count)
                .map(|index| OperationRecord {
                    id: id(index as u128 + 1),
                    kind: if index == 0 {
                        OperationKind::Create
                    } else {
                        OperationKind::Prepare
                    },
                    occurred_at_ms: Some(1_793_290_000_000),
                    prepared_inputs: if index == 0 {
                        0
                    } else if maximum {
                        32
                    } else {
                        1
                    },
                    transformed_spans: if index == 0 {
                        0
                    } else if maximum {
                        10000
                    } else {
                        3
                    },
                })
                .collect();
            let started = Instant::now();
            let bytes = snapshot(&records);
            let encode_ms = started.elapsed().as_secs_f64() * 1000.;
            let started = Instant::now();
            let mut copy = black_box(records.clone());
            let clone_ms = started.elapsed().as_secs_f64() * 1000.;
            let clone_capacity = copy.capacity();
            copy.try_reserve_exact(1).unwrap();
            copy.push(records[0].clone());
            let append_capacity = copy.capacity();
            drop(copy);
            let started = Instant::now();
            let opened = Workspace::open_bundle(&bytes);
            let open_ms = started.elapsed().as_secs_f64() * 1000.;
            let mut prepare_ms = None;
            let mut prepare_error = None;
            let admitted = opened.is_ok();
            if let Ok(workspace) = opened {
                let before = workspace.counts().unwrap();
                let started = Instant::now();
                let change = workspace.prepare_with_operation(
                    "",
                    &[],
                    OperationContext {
                        id: id(u128::MAX),
                        occurred_at_ms: 1_793_290_000_001,
                    },
                );
                prepare_ms = Some(started.elapsed().as_secs_f64() * 1000.);
                prepare_error = change.err().map(|error| error.code.as_str());
                assert_eq!(workspace.counts().unwrap(), before);
            }
            results.push(json!({"profile":if maximum{"maximum_numeric_deltas"}else{"typical_numeric_deltas"},"records":count,
                "wireBytes":bytes.len(),"recordStructBytes":size_of::<OperationRecord>(),
                "recordsCapacity":records.capacity(),"cloneCapacity":clone_capacity,"afterAppendCapacity":append_capacity,
                "recordBackingBytesAfterAppend":append_capacity*size_of::<OperationRecord>(),
                "encodeMs":encode_ms,"vectorCloneMs":clone_ms,"openAdmittedMs":if admitted{Some(open_ms)}else{None},"rejectedOpenAttemptMs":if admitted{None}else{Some(open_ms)},"admittedByExistingDraftCap":admitted,
                "prepareMs":prepare_ms,"prepareError":prepare_error}));
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"scope":"synthetic unapproved v3 profile: only cases flagged admitted were decoded; temporary capacity experiments do not select a launch limit", "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"experimentalRecordCap":MAX_OPERATIONS,"cases":results})).unwrap());
}
