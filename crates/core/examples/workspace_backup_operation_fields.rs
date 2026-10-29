//! Generate synthetic current-v3 backup vectors through the real public Core lifecycle.
use datafog_core::{
    TextRange,
    workspace::{
        ChoiceDisposition, ErrorCode, OperationContext, OperationOutcome, OutcomeKind, PolicyEdits,
        PreparedChange, TypeRule, TypeRuleEdit, ValueChoiceEdit, Workspace, WorkspaceFinding,
        WorkspaceInput,
    },
};
use minicbor::{Decoder, Encoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, path::Path, process::Command};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(value: &str) -> Result<Vec<u8>> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn context(id: u8, time: u64) -> OperationContext {
    OperationContext {
        id: [id; 16],
        occurred_at_ms: time,
    }
}
fn finding(text: &str, value: &str, label: &str) -> Result<WorkspaceFinding> {
    let start = text.find(value).ok_or("fixture finding missing")?;
    Ok(WorkspaceFinding::new(
        label,
        value,
        TextRange {
            start,
            end: start + value.len(),
        },
    ))
}
fn save(workspace: &mut Workspace, plan: &mut PreparedChange) -> Result<()> {
    // Fixture construction validates the candidate independently, not a durable host claim.
    let checked = Workspace::open_bundle(plan.export_candidate_bundle()?)?;
    workspace.activate_saved(plan, plan.expected_head()?, checked.head()?)?;
    Ok(())
}
fn members(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut decoder = Decoder::new(bytes);
    let count = decoder.array()?.ok_or("indefinite fixture array")?;
    let mut values = Vec::new();
    for _ in 0..count {
        let start = decoder.position();
        decoder.skip()?;
        values.push(bytes[start..decoder.position()].to_vec());
    }
    if decoder.position() != bytes.len() {
        return Err("trailing fixture array data".into());
    }
    Ok(values)
}
fn fields(bytes: &[u8]) -> Result<Vec<(u8, Vec<u8>)>> {
    let mut decoder = Decoder::new(bytes);
    let count = decoder.map()?.ok_or("indefinite fixture map")?;
    let mut values = Vec::new();
    for _ in 0..count {
        let key = decoder.u8()?;
        let start = decoder.position();
        decoder.skip()?;
        values.push((key, bytes[start..decoder.position()].to_vec()));
    }
    if decoder.position() != bytes.len() {
        return Err("trailing fixture map data".into());
    }
    Ok(values)
}
fn array(values: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.array(values.len() as u64)?;
    for value in values {
        encoder.writer_mut().extend_from_slice(value);
    }
    Ok(encoder.into_writer())
}
fn map(values: &[(u8, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.map(values.len() as u64)?;
    for (key, value) in values {
        encoder.u8(*key)?;
        encoder.writer_mut().extend_from_slice(value);
    }
    Ok(encoder.into_writer())
}
fn uint(value: u64) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.u64(value)?;
    Ok(encoder.into_writer())
}
fn null() -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.null()?;
    Ok(encoder.into_writer())
}
fn field(bytes: &[u8], key: u8) -> Result<Vec<u8>> {
    fields(bytes)?
        .into_iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
        .ok_or_else(|| "fixture key missing".into())
}
fn replace_field(bytes: &[u8], key: u8, value: Vec<u8>) -> Result<Vec<u8>> {
    let mut values = fields(bytes)?;
    let target = values
        .iter_mut()
        .find(|(candidate, _)| *candidate == key)
        .ok_or("fixture key missing")?;
    target.1 = value;
    map(&values)
}
fn change_row(bytes: &[u8], row: usize, member: usize, value: Vec<u8>) -> Result<Vec<u8>> {
    let mut rows = members(&field(bytes, 8)?)?;
    let mut values = members(rows.get(row).ok_or("fixture row missing")?)?;
    *values.get_mut(member).ok_or("fixture member missing")? = value;
    rows[row] = array(&values)?;
    replace_field(bytes, 8, array(&rows)?)
}
fn change_count(bytes: &[u8], index: usize, value: u64) -> Result<Vec<u8>> {
    let mut counts = members(&field(bytes, 10)?)?;
    *counts.get_mut(index).ok_or("fixture count missing")? = uint(value)?;
    replace_field(bytes, 10, array(&counts)?)
}
fn snapshot(workspace: &Workspace, aliases: Value) -> Result<Value> {
    let bytes = workspace.export_bundle()?;
    let mut decoder = Decoder::new(&bytes);
    let count = decoder.map()?.ok_or("fixture map missing")?;
    let mut spans = Vec::new();
    for _ in 0..count {
        let key = decoder.u8()?;
        let start = decoder.position();
        decoder.skip()?;
        spans.push(json!({"key":key,"valueStart":start,"valueEnd":decoder.position()}));
    }
    let counts = workspace.counts()?;
    let operations: Vec<_> = workspace
        .operations()?
        .iter()
        .map(|record| {
            json!({
                "id":record.id,"kind":record.kind.as_str(),"occurredAtMs":record.occurred_at_ms,
                "preparedInputs":record.prepared_inputs,"transformedSpans":record.transformed_spans,
            })
        })
        .collect();
    Ok(
        json!({"bundleHex":hex(&bytes),"fields":spans,"aliases":aliases,
        "head":{"revision":workspace.head()?.revision.to_string(),"commitId":workspace.head()?.commit_id},
        "policy":workspace.policy()?,"operations":operations,"historyCoverage":workspace.history_coverage()?,
        "counts":{"operations":counts.operations,"preparedInputs":counts.prepared_inputs,"transformedSpans":counts.transformed_spans,
            "confirmedHandoffs":counts.confirmed_handoffs,"reportedBlockedAttempts":counts.reported_blocked_attempts}}),
    )
}
fn current() -> Result<Value> {
    let first = "İ Synthetic Person";
    let second = "Second Synthetic Person";
    let account = "0012345678";
    let a = format!("{first} | {second}");
    let b = format!("{account} / {first}");
    let fa = [
        finding(&a, first, "PERSON")?,
        finding(&a, second, "PERSON")?,
    ];
    let fb = [
        finding(&b, account, "ACCOUNT_NUMBER")?,
        finding(&b, first, "PERSON")?,
    ];
    let mut workspace = Workspace::create_with_operation([80; 16], context(81, 0))?;
    let mut plan = workspace.prepare_batch_with_operation(
        &[
            WorkspaceInput {
                text: &a,
                findings: &fa,
            },
            WorkspaceInput {
                text: &b,
                findings: &fb,
            },
        ],
        context(82, 1000),
    )?;
    let preview = &plan.previews()?[0];
    let marker = |index: usize| -> Result<String> {
        let range = preview
            .transformations
            .get(index)
            .ok_or("fixture transformation missing")?
            .output_byte_range;
        Ok(preview
            .text
            .get(range.start..range.end)
            .ok_or("fixture output range invalid")?
            .to_owned())
    };
    let first_marker = marker(0)?;
    let second_marker = marker(1)?;
    let preview = &plan.previews()?[1];
    let range = preview.transformations[0].output_byte_range;
    let account_marker = preview
        .text
        .get(range.start..range.end)
        .ok_or("fixture account range invalid")?
        .to_owned();
    save(&mut workspace, &mut plan)?;
    let edits = PolicyEdits {
        rules: vec![
            TypeRuleEdit {
                entity_type: "OPAQUE_TAG".into(),
                rule: Some(TypeRule::Hide),
            },
            TypeRuleEdit {
                entity_type: "PRODUCT_CODE".into(),
                rule: Some(TypeRule::Keep),
            },
        ],
        choices: vec![
            ValueChoiceEdit {
                entity_type: "PERSON".into(),
                value: "Synthetic False Positive".into(),
                disposition: Some(ChoiceDisposition::NotSensitive),
            },
            ValueChoiceEdit {
                entity_type: "PRODUCT_CODE".into(),
                value: "Keep exactly".into(),
                disposition: Some(ChoiceDisposition::Keep),
            },
        ],
    };
    let mut plan = workspace.prepare_policy_changes(&edits, [83; 16])?;
    save(&mut workspace, &mut plan)?;
    let variant = "i\u{307}  SYNTHETIC\tPERSON";
    let mut plan = workspace.prepare_with_operation(
        variant,
        &[finding(variant, variant, "PERSON")?],
        context(84, 253402300799999),
    )?;
    if plan.preview()?.text != first_marker {
        return Err("fixture normalized alias changed".into());
    }
    save(&mut workspace, &mut plan)?;
    for (id, kind, issue_count) in [
        (85, OutcomeKind::CopyConfirmed, 0),
        (86, OutcomeKind::UserCancelled, 0),
        (87, OutcomeKind::BlockedInput, 3),
        (88, OutcomeKind::RestoreBlocked, 2),
    ] {
        let mut plan =
            workspace.prepare_outcome(OperationOutcome { kind, issue_count }, context(id, 5))?;
        save(&mut workspace, &mut plan)?;
    }
    let mut plan = workspace.prepare_recovery([89; 16])?;
    save(&mut workspace, &mut plan)?;
    let counts = workspace.counts()?;
    if (
        counts.operations,
        counts.prepared_inputs,
        counts.transformed_spans,
        counts.confirmed_handoffs,
        counts.reported_blocked_attempts,
    ) != (9, 3, 5, 0, 2)
    {
        return Err("fixture action accounting changed".into());
    }
    snapshot(
        &workspace,
        json!([{"marker":first_marker,"value":first},{"marker":second_marker,"value":second},{"marker":account_marker,"value":account}]),
    )
}
fn upgraded(root: &Path) -> Result<Value> {
    let old: Value = serde_json::from_slice(&std::fs::read(
        root.join("fixtures/workspace/policy-v2.json"),
    )?)?;
    let mut workspace = Workspace::open_bundle(&unhex(
        old["bundleHex"].as_str().ok_or("legacy fixture missing")?,
    )?)?;
    let marker = old["marker"]
        .as_str()
        .ok_or("legacy alias missing")?
        .to_owned();
    let value = workspace.restore(&marker)?;
    let mut plan = workspace.prepare_outcome(
        OperationOutcome {
            kind: OutcomeKind::UserCancelled,
            issue_count: 0,
        },
        context(101, 123),
    )?;
    save(&mut workspace, &mut plan)?;
    snapshot(&workspace, json!([{"marker":marker,"value":value}]))
}
fn rejections(bytes: &[u8]) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let mut add = |id: &str, candidate: Vec<u8>, encrypted: bool| -> Result<()> {
        let failure = Workspace::open_bundle(&candidate)
            .err()
            .ok_or("negative fixture unexpectedly accepted")?;
        if failure.code != ErrorCode::InvalidSnapshot {
            return Err("negative fixture category changed".into());
        }
        result.push(json!({"id":id,"bundleHex":hex(&candidate),"code":failure.code.as_str(),"encryptedImport":encrypted}));
        Ok(())
    };
    for key in [8, 10] {
        let values: Vec<_> = fields(bytes)?
            .into_iter()
            .filter(|(candidate, _)| *candidate != key)
            .collect();
        add(&format!("missing_top_key_{key}"), map(&values)?, true)?;
    }
    add(
        "operations_empty",
        replace_field(bytes, 8, array(&[])?)?,
        false,
    )?;
    add(
        "operations_wrong_type",
        replace_field(bytes, 8, uint(0)?)?,
        false,
    )?;
    let rows = members(&field(bytes, 8)?)?;
    for member in 0..5 {
        let mut changed = rows.clone();
        let mut values = members(&changed[1])?;
        values.remove(member);
        changed[1] = array(&values)?;
        add(
            &format!("missing_operation_member_{member}"),
            replace_field(bytes, 8, array(&changed)?)?,
            false,
        )?;
    }
    add(
        "duplicate_operation_id",
        change_row(bytes, 3, 0, members(&rows[1])?[0].clone())?,
        false,
    )?;
    add(
        "unknown_operation_kind",
        change_row(bytes, 1, 1, uint(255)?)?,
        false,
    )?;
    add(
        "operation_time_above_bound",
        change_row(bytes, 1, 2, uint(253402300800000)?)?,
        false,
    )?;
    add(
        "outcome_unknown_time",
        change_row(bytes, 4, 2, null()?)?,
        true,
    )?;
    add(
        "prepare_zero_inputs",
        change_row(bytes, 1, 3, uint(0)?)?,
        false,
    )?;
    add(
        "prepare_too_many_inputs",
        change_row(bytes, 1, 3, uint(33)?)?,
        false,
    )?;
    add(
        "prepare_too_many_spans",
        change_row(bytes, 1, 4, uint(10001)?)?,
        false,
    )?;
    add(
        "nonprepare_nonzero_delta",
        change_row(bytes, 4, 3, uint(1)?)?,
        false,
    )?;
    let mut changed = rows.clone();
    let mut values = members(&changed[1])?;
    values.push(uint(0)?);
    changed[1] = array(&values)?;
    add(
        "extra_operation_member",
        replace_field(bytes, 8, array(&changed)?)?,
        false,
    )?;
    let counts = members(&field(bytes, 10)?)?;
    for member in 0..5 {
        let mut changed = counts.clone();
        changed.remove(member);
        add(
            &format!("missing_count_member_{member}"),
            replace_field(bytes, 10, array(&changed)?)?,
            false,
        )?;
    }
    for (index, name, value) in [
        (0, "operation_total_mismatch", 10),
        (1, "input_total_mismatch", 4),
        (2, "span_total_mismatch", 6),
        (3, "unimplemented_handoff_total", 1),
        (4, "blocked_attempt_total_mismatch", 5),
    ] {
        add(name, change_count(bytes, index, value)?, index == 3)?;
    }
    add(
        "counts_wrong_type",
        replace_field(bytes, 10, uint(0)?)?,
        false,
    )?;
    let mut declared = Encoder::new(Vec::new());
    declared.array(100001)?;
    add(
        "operation_count_above_bound",
        replace_field(bytes, 8, declared.into_writer())?,
        false,
    )?;
    // Keep aggregate totals consistent so this vector reaches alias/row cross-checks.
    let candidate = change_count(&change_row(bytes, 1, 4, uint(0)?)?, 2, 1)?;
    add("mapping_issuance_exceeds_recorded_spans", candidate, true)?;
    let mut head = members(&field(bytes, 2)?)?;
    head[0] = uint(9)?;
    add(
        "complete_history_revision_mismatch",
        replace_field(bytes, 2, array(&head)?)?,
        false,
    )?;
    Ok(result)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err("fixture source identity unavailable".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--", "crates/core/src"])
        .current_dir(&root)
        .status()?
        .success()
    {
        return Err("fixture Core runtime source must match its declared commit".into());
    }
    let current = current()?;
    let bytes = unhex(
        current["bundleHex"]
            .as_str()
            .ok_or("fixture encoding missing")?,
    )?;
    let generator =
        std::fs::read(root.join("crates/core/examples/workspace_backup_operation_fields.rs"))?;
    let fixture = json!({"schemaVersion":1,"syntheticOnly":true,"source":{"commit":git(&root,&["rev-parse","HEAD"])? ,"coreRuntimeTree":git(&root,&["rev-parse","HEAD:crates/core/src"])? ,"generatorSha256":hex(&Sha256::digest(generator))},
        "current":current,"legacyUpgraded":upgraded(&root)?,"rejections":rejections(&bytes)?});
    println!("{}", serde_json::to_string_pretty(&fixture)?);
    Ok(())
}
