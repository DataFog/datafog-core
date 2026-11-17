//! Synthetic application-vault observations. Timers exclude executable startup.
use datafog_core::TextRange;
use datafog_fogvault_native::{
    application::{ApplicationVault, Options},
    core,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn id() -> Result<[u8; 16]> {
    let mut id = [0; 16];
    getrandom::fill(&mut id)?;
    Ok(id)
}
fn options(path: &Path) -> Options {
    let mut options = Options::new("synthetic-application-benchmark");
    options.directory = Some(path.to_owned());
    options.key = Some(zeroize::Zeroizing::new([7; 32]));
    options
}
fn open(path: &Path) -> Result<ApplicationVault> {
    Ok(ApplicationVault::open(options(path))?)
}
fn prompt() -> (String, Vec<core::WorkspaceFinding>) {
    let value = "synthetic00000@example.test";
    let mut text = value.to_owned();
    text.extend(std::iter::repeat_n(' ', 4096 - text.len()));
    (
        text,
        vec![core::WorkspaceFinding::new(
            "EMAIL",
            value,
            TextRange {
                start: 0,
                end: value.len(),
            },
        )],
    )
}
fn mutation(path: &Path) -> Result<f64> {
    let (text, findings) = prompt();
    let operation = core::OperationContext {
        id: id()?,
        occurred_at_ms: 0,
    };
    let start = Instant::now();
    let mut owner = open(path)?;
    let committed = owner.prepare_and_commit_with_operation(&text, &findings, operation)?;
    if committed.texts.len() != 1 {
        return Err("benchmark commit failed".into());
    }
    Ok(start.elapsed().as_secs_f64() * 1000.)
}
fn summary(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let at = |percent: usize| samples[(samples.len() * percent).div_ceil(100).saturating_sub(1)];
    json!({"samples":samples.len(),"p50_ms":at(50),"p95_ms":at(95)})
}
fn seed(path: &Path, entries: usize) -> Result<String> {
    let mut owner = open(path)?;
    let mut marker = None;
    for offset in (0..entries).step_by(10_000) {
        let mut source = String::new();
        let mut findings = Vec::new();
        for index in offset..(offset + 10_000).min(entries) {
            let value = format!("synthetic{index:05}@example.test");
            let start = source.len();
            source.push_str(&value);
            findings.push(core::WorkspaceFinding::new(
                "EMAIL",
                &value,
                TextRange {
                    start,
                    end: source.len(),
                },
            ));
            source.push(' ');
        }
        let committed = owner.prepare_and_commit(&source, &findings, id()?)?;
        if offset == 0 {
            marker = committed
                .texts
                .first()
                .and_then(|t| t.split_whitespace().next())
                .map(str::to_owned);
        }
    }
    marker.ok_or_else(|| "benchmark seeding failed".into())
}
fn contention(path: &Path, iterations: usize) -> Result<Value> {
    let executable = std::env::current_exe()?;
    let barrier = path.join("start");
    let mut children = Vec::new();
    for worker in 0..4 {
        children.push(
            Command::new(&executable)
                .arg("--worker")
                .arg(path)
                .arg(iterations.to_string())
                .arg(worker.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
    }
    let start = Instant::now();
    while (0..4).any(|i| !path.join(format!("ready-{i}")).exists()) {
        if start.elapsed() > Duration::from_secs(30) {
            for child in &mut children {
                let _ = child.kill();
            }
            return Err("benchmark worker readiness timeout".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::write(&barrier, b"start")?;
    let mut samples = Vec::new();
    for child in children {
        let result = child.wait_with_output()?;
        if !result.status.success() {
            return Err("benchmark worker failed".into());
        }
        samples.extend(serde_json::from_slice::<Vec<f64>>(&result.stdout)?);
    }
    Ok(summary(samples))
}
fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|a| a == "--worker") {
        if arguments.len() != 4 {
            return Err("invalid worker arguments".into());
        }
        let path = Path::new(&arguments[1]);
        let samples: usize = arguments[2].parse()?;
        std::fs::write(path.join(format!("ready-{}", arguments[3])), b"ready")?;
        let start = Instant::now();
        while !path.join("start").exists() {
            if start.elapsed() > Duration::from_secs(30) {
                return Err("benchmark barrier timeout".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let times = (0..samples)
            .map(|_| mutation(path))
            .collect::<Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string(&times)?);
        return Ok(());
    }
    let samples: usize = match arguments.as_slice() {
        [] => 20,
        [flag, value] if flag == "--samples" => value.parse()?,
        _ => return Err("expected --samples N".into()),
    };
    if !(2..=1000).contains(&samples) {
        return Err("samples must be between 2 and 1000".into());
    }
    let mut measurements = Vec::new();
    for entries in [1000, 10000, 20000] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().canonicalize()?.join("vault");
        let mut marker = seed(&path, entries)?;
        marker.extend(std::iter::repeat_n(' ', 4096 - marker.len()));
        let mut restores = Vec::new();
        let mut writes = Vec::new();
        for _ in 0..samples {
            let start = Instant::now();
            let mut owner = open(&path)?;
            let restored = owner.restore(&marker)?;
            if !restored.starts_with("synthetic00000@example.test") {
                return Err("benchmark restore failed".into());
            }
            restores.push(start.elapsed().as_secs_f64() * 1000.);
            writes.push(mutation(&path)?);
        }
        let restore = summary(restores);
        let commit = summary(writes);
        let proposed_met = [&restore, &commit].iter().all(|s| {
            s["p50_ms"].as_f64().is_some_and(|v| v <= 50.)
                && s["p95_ms"].as_f64().is_some_and(|v| v <= 100.)
        });
        measurements.push(json!({"entries":entries,"open_restore":restore,"open_prepare_save_activate":commit,"four_process_writers":contention(&path,5)?,"proposed_1k_targets_met":if entries==1000 {Some(proposed_met)}else{None}}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":"datafog-application-vault-benchmark/v1","os":std::env::consts::OS,"arch":std::env::consts::ARCH,"prompt_bytes":4096,"interpreter_startup_included":false,"key_mode":"external_key","synthetic_only":true,"proposed_1k_targets":{"p50_ms":50,"p95_ms":100,"enforced":false},"four_writer_samples_each":5,"measurements":measurements})
        )?
    );
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("application vault benchmark failed");
        std::process::exit(1);
    }
}
