//! Proposal-only CBOR growth experiment. Current Core does not accept key 9.
//! This carries an unchanged synthetic payload around real Core recovery exports;
//! it proves byte growth, not conversation semantics or decoder acceptance.
use datafog_core::workspace::{MAX_OPERATION_TIME_MS, OperationContext, Workspace};
use minicbor::{Decoder, Encoder};
use serde_json::json;
use std::{error::Error, fs, path::Path};

fn payload(code: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut encoder = Encoder::new(Vec::new());
    // [contexts, active intents, terminal resolutions, issued memberships]
    encoder
        .array(4)?
        .array(1)?
        .array(2)?
        .str("synthetic_chat")?
        .str("thread_a")?
        .array(1)?
        .array(4)?
        .bytes(&[3; 16])?
        .u8(0)?
        .u8(0)?
        .array(1)?
        .array(2)?
        .str("PERSON")?
        .str(code)?
        .array(0)?
        .array(0)?;
    Ok(encoder.into_writer())
}

fn carry_key9(bytes: &[u8], payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut decoder = Decoder::new(bytes);
    if decoder.map()? != Some(10) {
        return Err("experiment requires the committed operations map10".into());
    }
    let mut output = vec![0xab]; // map11 has the same one-byte header width.
    for expected in [0, 1, 2, 3, 4, 5, 6, 7, 8, 10] {
        let start = decoder.position();
        if decoder.u8()? != expected {
            return Err("unexpected operations map key".into());
        }
        decoder.skip()?;
        if expected == 10 {
            output.push(9);
            output.extend_from_slice(payload);
        }
        output.extend_from_slice(&bytes[start..decoder.position()]);
    }
    if decoder.position() != bytes.len() {
        return Err("trailing operations bytes".into());
    }
    Ok(output)
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: conversation_wire_growth CAPACITY_DIRECTORY")?;
    let directory = Path::new(&directory);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    let marker = manifest["marker"].as_str().ok_or("missing marker")?;
    let code = marker
        .strip_prefix("[[PERSON_")
        .and_then(|value| value.strip_suffix("]]"))
        .ok_or("unexpected synthetic marker")?;
    let payload = payload(code)?;
    let mut observations = Vec::new();
    for name in ["ordinary", "last-recovery"] {
        let before = fs::read(directory.join(format!("{name}.cbor")))?;
        let workspace = Workspace::open_bundle(&before)?;
        let plan = workspace.prepare_recovery_with_operation(OperationContext {
            id: [255; 16],
            occurred_at_ms: MAX_OPERATION_TIME_MS,
        })?;
        let after = plan.export_candidate_bundle()?;
        let before9 = carry_key9(&before, &payload)?;
        let after9 = carry_key9(after, &payload)?;
        let core_growth = after
            .len()
            .checked_sub(before.len())
            .ok_or("negative recovery growth")?;
        let carried_growth = after9
            .len()
            .checked_sub(before9.len())
            .ok_or("negative carried growth")?;
        if core_growth != carried_growth || before9.len() - before.len() != 1 + payload.len() {
            return Err("unchanged key9 altered recovery growth".into());
        }
        // Prevent this experiment from being mistaken for runtime acceptance.
        if Workspace::open_bundle(&before9).is_ok() {
            return Err("current Core unexpectedly accepts the proposed conversation wire".into());
        }
        observations.push(
            json!({"profile":name,"beforeRecords":workspace.counts()?.operations,
            "beforeBytes":before.len(),"afterBytes":after.len(),"coreRecoveryGrowth":core_growth,
            "carriedKey9RecoveryGrowth":carried_growth,"key9AdditionalGrowth":0,
            "carriedBeforeBytes":before9.len(),"carriedAfterBytes":after9.len(),
            "currentDecoderRejectsProposedMap":true}),
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"scope":"synthetic encoder growth only; not conversation runtime, admission, or decoder qualification",
        "baseFixtureCommit":manifest["sourceCommit"],"baseFixtureDirty":manifest["sourceDirty"],
        "payloadBytes":payload.len(),"keyAndPayloadBytes":1+payload.len(),"observations":observations,
        "remaining":"Final conversation encoder/decoder, nonempty encrypted backup, resource admission, and all SDK semantics must execute before release."})
        )?
    );
    Ok(())
}
