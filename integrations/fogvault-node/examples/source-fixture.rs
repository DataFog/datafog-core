//! Synthetic authenticated-source stand-in; no legacy decryption or origin claim.
use datafog_core::{
    TextRange,
    workspace::{MAX_OPERATION_TIME_MS, OperationContext, Workspace, WorkspaceFinding},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let namespace = [1; 16];
    let mut source = Workspace::create_with_operation(
        namespace,
        OperationContext {
            id: [2; 16],
            occurred_at_ms: MAX_OPERATION_TIME_MS,
        },
    )?;
    let text = "alice@example.test";
    let finding = WorkspaceFinding::new(
        "EMAIL",
        text,
        TextRange {
            start: 0,
            end: text.len(),
        },
    );
    let mut plan = source.prepare(text, &[finding], [3; 16])?;
    let marker = plan.preview()?.text.clone();
    let parent = plan.expected_head()?;
    let saved = plan.candidate_head()?;
    source.activate_saved(&mut plan, parent, saved)?;
    let bundle = source.export_bundle()?;
    // Only replace the one same-width canonical timestamp scalar in a genuine
    // Core-exported fixture. This does not implement an alternate serializer.
    let mut pattern = vec![0x1b];
    pattern.extend_from_slice(&MAX_OPERATION_TIME_MS.to_be_bytes());
    let positions = bundle
        .windows(pattern.len())
        .enumerate()
        .filter_map(|(i, part)| (part == pattern).then_some(i))
        .collect::<Vec<_>>();
    if positions.len() != 1 {
        return Err("synthetic timestamp must occur exactly once".into());
    }
    let offset = positions[0];
    let mut oversized = bundle.clone();
    oversized[offset + 1..offset + 9].copy_from_slice(&u64::MAX.to_be_bytes());
    let mut negative = bundle.clone();
    negative[offset] = 0x3b;
    negative[offset + 1..offset + 9].copy_from_slice(&(i64::MAX as u64).to_be_bytes());
    if Workspace::open_bundle(&oversized).is_ok() || Workspace::open_bundle(&negative).is_ok() {
        return Err("Core admitted an invalid timestamp fixture".into());
    }
    println!(
        "{}",
        serde_json::json!({"synthetic":true,"namespace":namespace,"head":{"revision":saved.revision.to_string(),"commitId":saved.commit_id},"bundle":bundle,"marker":marker,"text":text,"maxOperationTimeMs":MAX_OPERATION_TIME_MS,"invalidTimestampBundles":[{"id":"oversized_u64","bundle":oversized},{"id":"negative_i64","bundle":negative}]})
    );
    Ok(())
}
