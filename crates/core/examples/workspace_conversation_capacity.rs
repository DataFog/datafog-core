//! Synthetic final-wire graph/heap qualification; instrumentation is example-only.
use datafog_core::workspace::{
    Conversation, ConversationInput, MAX_OPERATIONS, MAX_ORDINARY_OPERATIONS, OperationContext,
    RestoreEncoding, ReviewInput, Workspace,
};
use minicbor::Encoder;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    error::Error,
    fs,
    path::Path,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn added(bytes: usize) {
    let now = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            if size >= layout.size() {
                added(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn id(index: usize) -> [u8; 16] {
    (index as u128).to_be_bytes()
}
fn context(index: usize) -> Conversation {
    let site = format!("thread_{index}");
    let budget = site.len();
    Conversation {
        id: id(1_000_000 + index),
        surface: "synthetic_chat".into(),
        site_id: Some(site),
        site_id_byte_budget: budget,
    }
}
fn fixture(total: usize) -> Result<(Vec<u8>, usize), Box<dyn Error>> {
    let ordinary = total.min(MAX_ORDINARY_OPERATIONS);
    let contexts = (ordinary - 1) / 2;
    let mut e = Encoder::new(Vec::new());
    e.map(11)?
        .u8(0)?
        .u8(3)?
        .u8(1)?
        .bytes(&[1; 16])?
        .u8(2)?
        .array(2)?
        .u64((total - 1) as u64)?
        .bytes(&id(total))?
        .u8(3)?
        .array(0)?
        .u8(4)?
        .array(0)?
        .u8(5)?
        .array(0)?
        .u8(6)?
        .map(2)?
        .u8(0)?
        .str("protected_v1")?
        .u8(1)?
        .array(0)?
        .u8(7)?
        .array(0)?
        .u8(8)?
        .array(total as u64)?;
    let mut prepared = 0;
    for index in 0..total {
        let (kind, inputs) = if index == 0 {
            (0, 0)
        } else if index <= 2 * contexts {
            if index % 2 == 1 { (10, 1) } else { (8, 0) }
        } else if index < ordinary {
            (1, 1)
        } else {
            (2, 0)
        };
        prepared += inputs;
        e.array(5)?.bytes(&id(index + 1))?.u8(kind)?;
        if index == 0 {
            e.null()?;
        } else {
            e.u64(253_402_300_799_999)?;
        }
        e.u64(inputs)?.u64(0)?;
    }
    e.u8(9)?.array(4)?.array(contexts as u64)?;
    for index in 0..contexts {
        let value = context(index);
        e.array(4)?
            .bytes(&value.id)?
            .str(&value.surface)?
            .str(value.site_id.as_deref().ok_or("missing synthetic site")?)?
            .u64(value.site_id_byte_budget as u64)?;
    }
    e.array(0)?.array(contexts as u64)?;
    for index in 0..contexts {
        e.array(6)?
            .bytes(&id(3 + 2 * index))?
            .bytes(&id(2 + 2 * index))?
            .u8(0)?
            .u64(index as u64)?
            .u8(0)?
            .array(0)?;
    }
    e.array(0)?
        .u8(10)?
        .array(5)?
        .u64(total as u64)?
        .u64(prepared)?
        .u64(0)?
        .u64(contexts as u64)?
        .u64(0)?;
    Ok((e.into_writer(), contexts))
}
fn command(program: &str, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err("qualification provenance command failed".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn main() -> Result<(), Box<dyn Error>> {
    check_fixture_shape()?;
    let directory = std::env::args()
        .nth(1)
        .ok_or("usage: workspace_conversation_capacity OUTPUT_DIRECTORY")?;
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    let mut profiles = Vec::new();
    for (name, total, plan_count) in [
        ("near-ordinary", MAX_ORDINARY_OPERATIONS - 2, 32),
        ("hard", MAX_OPERATIONS, 0),
    ] {
        let (bytes, contexts) = fixture(total)?;
        fs::write(directory.join(format!("{name}.cbor")), &bytes)?;
        let baseline = LIVE.load(Ordering::Relaxed);
        PEAK.store(baseline, Ordering::Relaxed);
        let started = Instant::now();
        let workspace = Workspace::open_bundle(&bytes)
            .map_err(|error| format!("{name} graph import: {error}"))?;
        let open_ms = started.elapsed().as_secs_f64() * 1000.;
        let opened = LIVE.load(Ordering::Relaxed);
        let open_peak = PEAK.load(Ordering::Relaxed);
        if workspace.export_bundle()? != bytes
            || workspace.counts()?.confirmed_handoffs != contexts as u64
        {
            return Err("graph/counters/canonical wire mismatch".into());
        }
        let last = context(contexts - 1);
        let before = Instant::now();
        if workspace.conversation_for_site(
            &last.surface,
            last.site_id.as_deref().ok_or("missing site")?,
        )? != last.id
            || workspace.restore_in_conversation("plain", last.id)? != "plain"
        {
            return Err("last context lookup mismatch".into());
        }
        let lookup_ms = before.elapsed().as_secs_f64() * 1000.;
        let before_plans = LIVE.load(Ordering::Relaxed);
        PEAK.store(before_plans, Ordering::Relaxed);
        let started = Instant::now();
        let mut plans = Vec::new();
        for index in 0..plan_count {
            plans.push(workspace.prepare_conversation_send(
                &[ConversationInput {
                    input: ReviewInput {
                        text: "plain",
                        scanner_findings: &[],
                        user_selections: &[],
                        corrections: &[],
                    },
                    conversation: last.clone(),
                }],
                OperationContext {
                    id: id(2_000_000 + index),
                    occurred_at_ms: 0,
                },
            )?);
        }
        let plan_ms = started.elapsed().as_secs_f64() * 1000.;
        let retained = LIVE.load(Ordering::Relaxed);
        let peak = PEAK.load(Ordering::Relaxed);
        drop(plans);
        let after_drop = LIVE.load(Ordering::Relaxed);
        let mut stream =
            workspace.progressive_restore_in_conversation(last.id, RestoreEncoding::Utf8)?;
        let first = stream.feed(b"plain")?;
        let last = stream.finalize()?;
        if format!("{}{}", first.text, last.text) != "plain" || !last.complete {
            return Err("scoped stream mismatch".into());
        }
        stream.close();
        profiles.push(json!({"profile":name,"rows":total,"contexts":contexts,"wireBytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes)),"openMs":open_ms,"openNetRequestedBytes":opened.saturating_sub(baseline),"openPeakAdditionalRequestedBytes":open_peak.saturating_sub(baseline),"lastContextLookupMs":lookup_ms,"pendingPlans":plan_count,"prepareMs":plan_ms,"retainedPlansAdditionalRequestedBytes":retained.saturating_sub(before_plans),"peakPlansAdditionalRequestedBytes":peak.saturating_sub(before_plans),"afterDropAdditionalRequestedBytes":after_drop.saturating_sub(before_plans)}));
    }
    let receipt = json!({"syntheticOnly":true,"sourceCommit":command("git",&["rev-parse","HEAD"] )?,"sourceDirty":!command("git",&["status","--porcelain"] )?.is_empty(),"rust":command("rustc",&["--version"] )?,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"profiles":profiles,"notes":"One optimized native process, example-only counting allocator. Requested bytes exclude allocator overhead. No browser peak/low-hardware SLA, production host admission or actual site-observation qualification is claimed. Full history includes real confirmed member rows with no invented alias-count relationship."});
    fs::write(
        directory.join("native-graph.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
fn check_fixture_shape() -> Result<(), Box<dyn Error>> {
    let operation = |value| OperationContext {
        id: id(value),
        occurred_at_ms: 253_402_300_799_999,
    };
    let mut workspace = Workspace::create([1; 16], id(1))?;
    let mut first = workspace.prepare_conversation_send(
        &[ConversationInput {
            input: ReviewInput {
                text: "plain",
                scanner_findings: &[],
                user_selections: &[],
                corrections: &[],
            },
            conversation: context(0),
        }],
        operation(2),
    )?;
    let intent = first.send_intents()?[0];
    let expected = first.expected_head()?;
    let saved = first.candidate_head()?;
    workspace.activate_saved(&mut first, expected, saved)?;
    let mut confirmation = workspace.prepare_observed_send(
        intent,
        context(0).id,
        "synthetic_chat",
        "thread_0",
        operation(3),
    )?;
    let expected = confirmation.expected_head()?;
    let saved = confirmation.candidate_head()?;
    workspace.activate_saved(&mut confirmation, expected, saved)?;
    let mut ordinary = workspace.prepare_with_operation("plain", &[], operation(4))?;
    let expected = ordinary.expected_head()?;
    let saved = ordinary.candidate_head()?;
    workspace.activate_saved(&mut ordinary, expected, saved)?;
    let actual = workspace.export_bundle()?;
    let generated = fixture(4)?.0;
    if actual != generated {
        let index = actual.iter().zip(&generated).position(|(a, b)| a != b);
        return Err(format!(
            "fixture shape mismatch at {index:?}; actual {} bytes, generated {} bytes",
            actual.len(),
            generated.len()
        )
        .into());
    }
    Ok(())
}
