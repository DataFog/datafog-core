//! Reproducible synthetic boundary fixtures and requested-heap observations.
//! This allocator exists only in this example, never in a shipped SDK.
use datafog_core::{
    TextRange,
    workspace::{
        MAX_OPERATIONS, MAX_ORDINARY_OPERATIONS, OperationContext, OperationRecord,
        RESERVED_RECOVERY_OPERATIONS, Workspace, WorkspaceFinding,
    },
};
use minicbor::Encoder;
use serde_json::{Value, json};
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
fn add(size: usize) {
    let now = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(now, Ordering::Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Forward the allocator contract unchanged; counters allocate no memory.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            add(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            add(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            if size >= layout.size() {
                add(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        replacement
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(index: usize) -> [u8; 16] {
    match index {
        0 => [2; 16],
        1 => [3; 16],
        _ => ((index + 1000) as u128).to_be_bytes(),
    }
}
fn marker() -> Result<String, Box<dyn Error>> {
    let workspace = Workspace::create([1; 16], [2; 16])?;
    let change = workspace.prepare(
        "Jane",
        &[WorkspaceFinding::new(
            "PERSON",
            "Jane",
            TextRange { start: 0, end: 4 },
        )],
        [3; 16],
    )?;
    Ok(change.preview()?.text.clone())
}
fn snapshot(count: usize, marker: &str, maximum: bool) -> Result<Vec<u8>, Box<dyn Error>> {
    let code = marker
        .strip_prefix("[[PERSON_")
        .and_then(|value| value.strip_suffix("]]"))
        .ok_or("unexpected synthetic marker")?;
    let mut e = Encoder::new(Vec::new());
    e.map(11)?
        .u8(0)?
        .u8(3)?
        .u8(1)?
        .bytes(&[1; 16])?
        .u8(2)?
        .array(2)?
        .u64((count - 1) as u64)?
        .bytes(&id(count - 1))?
        .u8(3)?
        .array(1)?
        .array(5)?
        .str("PERSON")?
        .str("Jane")?
        .str(code)?
        .bytes(&[3; 16])?
        .u64(1)?
        .u8(4)?
        .array(1)?
        .array(2)?
        .str("PERSON")?
        .u64(1)?
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
        .array(count as u64)?;
    for index in 0..count {
        let recovery = index >= MAX_ORDINARY_OPERATIONS;
        e.array(5)?.bytes(&id(index))?.u8(if index == 0 {
            0
        } else if recovery {
            2
        } else {
            1
        })?;
        if index < 2 {
            e.null()?;
        } else {
            e.u64(if maximum { 253_402_300_799_999 } else { 0 })?;
        }
        let inputs = if index == 0 || recovery {
            0
        } else if maximum {
            32
        } else {
            1
        };
        let spans = if index == 0 || recovery {
            0
        } else if maximum {
            10_000
        } else {
            u64::from(index == 1)
        };
        e.u64(inputs)?.u64(spans)?;
    }
    e.u8(9)?.array(4)?.array(0)?.array(0)?.array(0)?.array(0)?;
    e.u8(10)?
        .array(5)?
        .u64(count as u64)?
        .u64((count.min(MAX_ORDINARY_OPERATIONS) as u64 - 1) * if maximum { 32 } else { 1 })?
        .u64(if maximum {
            (count.min(MAX_ORDINARY_OPERATIONS) as u64 - 1) * 10_000
        } else {
            1
        })?
        .u64(0)?
        .u64(0)?;
    let bytes = e.into_writer();
    let checked = Workspace::open_bundle(&bytes)?;
    if checked.export_bundle()? != bytes || checked.restore(marker)? != "Jane" {
        return Err("capacity fixture failed Core canonical/alias check".into());
    }
    Ok(bytes)
}
fn git(args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("git").args(args).output()?;
    if !out.status.success() {
        return Err("git receipt failed".into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().into())
}
fn rss() -> Option<usize> {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()?
        .checked_mul(1024)
}
fn measure(marker: &str, maximum: bool) -> Result<Value, Box<dyn Error>> {
    let bytes = snapshot(MAX_ORDINARY_OPERATIONS - 1, marker, maximum)?;
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let start = Instant::now();
    let workspace = Workspace::open_bundle(&bytes)?;
    let open_ms = start.elapsed().as_secs_f64() * 1000.;
    let opened = LIVE.load(Ordering::Relaxed);
    let open_peak = PEAK.load(Ordering::Relaxed);
    let mut exported = workspace.export_bundle()?;
    let export_capacity = exported.capacity();
    exported.fill(0);
    drop(exported);
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let rss_before = rss();
    let mut plans = Vec::with_capacity(32);
    let start = Instant::now();
    for index in 0..32 {
        plans.push(workspace.prepare_with_operation(
            "",
            &[],
            OperationContext {
                id: (u128::MAX - index).to_be_bytes(),
                occurred_at_ms: 253_402_300_799_999,
            },
        )?);
    }
    let prepare_ms = start.elapsed().as_secs_f64() * 1000.;
    let retained = LIVE.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed);
    let rss_retained = rss();
    let payload: usize = plans
        .iter()
        .map(|plan| plan.export_candidate_bundle().map(<[u8]>::len))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    drop(plans);
    let after_drop = LIVE.load(Ordering::Relaxed);
    if workspace.operations()?.len() != MAX_ORDINARY_OPERATIONS - 1
        || workspace.export_bundle()? != bytes
    {
        return Err("benchmark changed saved history".into());
    }
    Ok(
        json!({"profile":if maximum {"maximum_numeric_deltas"} else {"compact_deltas"},"records":MAX_ORDINARY_OPERATIONS-1,"recordStructBytes":std::mem::size_of::<OperationRecord>(),
        "wireBytes":bytes.len(),"exportedBundleCapacityBytes":export_capacity,
        "openMs":open_ms,"openNetRequestedBytes":opened.saturating_sub(baseline),"openPeakAdditionalRequestedBytes":open_peak.saturating_sub(baseline),
        "pendingPlans":32,"prepare32Ms":prepare_ms,"candidatePayloadBytes":payload,
        "retained32AdditionalRequestedBytes":retained.saturating_sub(before),"peak32AdditionalRequestedBytes":peak.saturating_sub(before),
        "requestedBytesAfterDropAboveBaseline":after_drop.saturating_sub(before),"processRssBeforeBytes":rss_before,"processRssWithPlansBytes":rss_retained,
        "notes":"Single optimized synthetic native process with counting allocator. Requested bytes exclude allocator overhead; RSS includes process/runtime and caching. Timing includes instrumentation. No browser/low-hardware SLA or global host admission guarantee."}),
    )
}
fn plan_pressure_cleanup() -> Result<Value, Box<dyn Error>> {
    let workspace = Workspace::create([1; 16], [2; 16])?;
    let mut source = String::with_capacity(256 * 1024);
    for index in 0..4096 {
        source.push_str(&digest(format!("native-plan-pressure:{index}").as_bytes()));
    }
    workspace
        .prepare_with_operation(
            &source,
            &[],
            OperationContext {
                id: [3; 16],
                occurred_at_ms: 0,
            },
        )?
        .discard();
    let before = workspace.export_bundle()?;
    let mut plans = Vec::with_capacity(32);
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    for index in 0..32 {
        plans.push(workspace.prepare_with_operation(
            &source,
            &[],
            OperationContext {
                id: ((index + 100) as u128).to_be_bytes(),
                occurred_at_ms: 0,
            },
        )?);
    }
    let held = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    for plan in &mut plans {
        plan.discard();
    }
    let after = LIVE.load(Ordering::Relaxed);
    if after != baseline || workspace.export_bundle()? != before {
        return Err("discard retained candidate allocations or mutated saved state".into());
    }
    Ok(
        json!({"sourceBytes":source.len(),"sourceSha256":digest(source.as_bytes()),"retainedPlanStructs":plans.len(),"requestedBytesWhilePending":held,"peakAdditionalRequestedBytes":peak,"additionalRequestedBytesAfterDiscard":after-baseline,
        "scope":"Exact example-only shared-Core requested allocations, not PyO3/NAPI registries, RSS, a Core plan limit or global host admission policy."}),
    )
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: workspace_operation_capacity OUTPUT_DIRECTORY | --plan-pressure")?;
    if output == "--plan-pressure" {
        println!("{}", serde_json::to_string(&plan_pressure_cleanup()?)?);
        return Ok(());
    }
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    let marker = marker()?;
    let mut fixtures = serde_json::Map::new();
    for (name, count) in [
        ("ordinary", MAX_ORDINARY_OPERATIONS),
        ("last-recovery", MAX_OPERATIONS - 1),
        ("hard", MAX_OPERATIONS),
    ] {
        let bytes = snapshot(count, &marker, true)?;
        let file = format!("{name}.cbor");
        fs::write(output.join(&file), &bytes)?;
        fixtures.insert(name.into(),json!({"file":file,"records":count,"bytes":bytes.len(),"sha256":digest(&bytes),"firstId":id(0),"lastId":id(count-1)}));
    }
    let inputs = [
        "crates/core/src/workspace.rs",
        "crates/core/src/workspace/codec.rs",
        "crates/core/src/workspace/operations.rs",
        "crates/core/examples/workspace_operation_capacity.rs",
    ];
    let mut hashes = serde_json::Map::new();
    for path in inputs {
        hashes.insert(path.to_owned(), Value::String(digest(&fs::read(path)?)));
    }
    let manifest = json!({"syntheticOnly":true,"sourceCommit":git(&["rev-parse","HEAD"] )?,"sourceDirty":!git(&["status","--porcelain"] )?.is_empty(),
        "maxOperations":MAX_OPERATIONS,"maxOrdinaryOperations":MAX_ORDINARY_OPERATIONS,"reservedRecoveryOperations":RESERVED_RECOVERY_OPERATIONS,
        "marker":marker,"fixtures":fixtures,"inputs":hashes,"measurements":[measure(&marker,false)?,measure(&marker,true)?]});
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}
