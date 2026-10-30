//! Advisory synthetic end-to-end observations; no latency or memory SLA.
use datafog_fogvault_native::{Owner, core, vault};
use std::{error::Error, time::Instant};
fn id() -> [u8; 16] {
    let mut out = [0; 16];
    getrandom::fill(&mut out).expect("synthetic IDs need OS randomness");
    out
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut measurements = vec![];
    for count in [1, 100, 1000, 5000] {
        let temp = tempfile::tempdir()?;
        let host = vault::Host::create_new(temp.path().join("control"))?;
        let path = temp.path().join("vault.sqlite");
        let limits = vault::Limits {
            max_payload_bytes: core::MAX_BUNDLE_BYTES,
            max_metadata_bytes: 88,
            max_records: 1,
        };
        let password = b"synthetic advisory native resource fixture";
        let start = Instant::now();
        let mut vault = host.create(&path, password, limits, vault::KdfPolicy::baseline())?;
        let token = vault.unlock(password)?;
        let record = id();
        let namespace = id();
        let mut owner = Owner::create(vault, token, record, namespace, id(), id())?;
        let create_unlock_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut source = String::new();
        let mut findings = vec![];
        for index in 0..count {
            let value = format!("synthetic{index}@example.test");
            let start = source.len();
            source.push_str(&value);
            findings.push(core::WorkspaceFinding::new(
                "EMAIL",
                &value,
                datafog_core::TextRange {
                    start,
                    end: source.len(),
                },
            ));
            source.push(' ');
        }
        let start = Instant::now();
        let mut change = owner.prepare(&source, &findings, id())?;
        let prepare_ms = start.elapsed().as_secs_f64() * 1000.;
        let candidate_bytes = change.candidate_bytes()?;
        let start = Instant::now();
        let committed = owner.commit(&mut change, true, id())?;
        let save_activate_ms = start.elapsed().as_secs_f64() * 1000.;
        owner.lock();
        drop(owner);
        let start = Instant::now();
        let mut vault = host.open_vault(&path, limits, vault::KdfPolicy::baseline())?;
        let token = vault.unlock(password)?;
        let owner = Owner::open(vault, token, record, namespace, Some(committed.head))?;
        let unlock_reopen_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let restored = owner.restore(&committed.previews[0])?;
        let restore_ms = start.elapsed().as_secs_f64() * 1000.;
        if restored != source {
            return Err("synthetic native roundtrip mismatch".into());
        }
        measurements.push(serde_json::json!({"entries":count,"source_bytes":source.len(),"candidate_bytes":candidate_bytes,"create_unlock_ms":create_unlock_ms,"prepare_ms":prepare_ms,"save_activate_ms":save_activate_ms,"unlock_reopen_ms":unlock_reopen_ms,"restore_ms":restore_ms}));
    }
    println!(
        "{}",
        serde_json::json!({"schema":"datafog-fogvault-native-measurement/v1","advisory":true,"memory_measured":false,"allocator_and_foreign_copies_included":false,"measurements":measurements})
    );
    Ok(())
}
