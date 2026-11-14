//! `cargo dfv fuzz`: the only path that builds and runs the fuzz targets.
//!
//! `crates/core/fuzz` is deliberately outside the root workspace, so nothing
//! else in the verification CLI compiles it. `check` verifies it stays
//! declared (see `reach.rs`); this module verifies it actually builds and runs.
use crate::report::workspace_root;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

const FUZZ_MANIFEST: &str = "crates/core/fuzz/Cargo.toml";

fn nightly_available() -> Option<String> {
    let output = Command::new("rustup")
        .args(["run", "nightly", "rustc", "--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn cargo_fuzz_available() -> Option<String> {
    let output = Command::new("cargo-fuzz")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `build` only needs the nightly toolchain; `run` also needs cargo-fuzz.
fn check_environment(needs_cargo_fuzz: bool) -> Result<(), String> {
    if nightly_available().is_none() {
        return Err("nightly toolchain missing (fix: rustup toolchain install nightly)".to_owned());
    }
    if needs_cargo_fuzz && cargo_fuzz_available().is_none() {
        return Err(
            "cargo-fuzz missing (fix: cargo install cargo-fuzz --locked --version 0.13.2)"
                .to_owned(),
        );
    }
    Ok(())
}

fn target_names(root: &std::path::Path) -> Vec<String> {
    let manifest = root.join("crates/core/fuzz/Cargo.toml");
    let Ok(text) = fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    let mut in_bin = false;
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.trim() == "[[bin]]" {
            in_bin = true;
            continue;
        }
        if line.starts_with('[') {
            in_bin = false;
            continue;
        }
        if in_bin {
            if let Some(name) = line.trim().strip_prefix("name = \"") {
                targets.push(name.trim_end_matches('"').to_owned());
            }
        }
    }
    targets
}

fn print_environment_help() {
    eprintln!("The fuzz targets need a nightly toolchain and cargo-fuzz:");
    eprintln!("  rustup toolchain install nightly");
    eprintln!("  cargo install cargo-fuzz --locked --version 0.13.2");
}

pub fn run_fuzz(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let sub = args.first().map(String::as_str);

    let root = workspace_root();
    let manifest_path: PathBuf = root.join(FUZZ_MANIFEST);

    match (sub, args.get(1).map(String::as_str)) {
        (Some("build"), _) => {
            if let Err(message) = check_environment(false) {
                eprintln!("{message}");
                print_environment_help();
                return ExitCode::FAILURE;
            }
            let mut format = Command::new("cargo");
            format
                .args(["fmt", "--check"])
                .arg("--manifest-path")
                .arg(&manifest_path);
            if !json_output {
                println!("Checking fuzz crate formatting...");
            }
            let format_status = format.status();
            if !format_status.as_ref().is_ok_and(|s| s.success()) {
                eprintln!("✗ fuzz fmt check failed");
                return ExitCode::FAILURE;
            }
            let mut command = Command::new("cargo");
            command
                .args(["+nightly", "build", "--release", "--bins"])
                .arg("--manifest-path")
                .arg(&manifest_path);
            run_and_report(command, "fuzz build", dry_run, json_output)
        }
        (Some("run"), Some(target)) if target_names(&root).contains(&target.to_owned()) => {
            if let Err(message) = check_environment(true) {
                eprintln!("{message}");
                print_environment_help();
                return ExitCode::FAILURE;
            }
            let mut command = Command::new("cargo");
            // cargo-fuzz takes its own crate directory, not a manifest path.
            command
                .args(["+nightly", "fuzz", "run", target])
                .arg("--fuzz-dir")
                .arg(root.join("crates/core/fuzz"));
            // Everything after `--` belongs to libFuzzer; keep it verbatim.
            if let Some(position) = args.iter().position(|a| a == "--") {
                command.arg("--").args(&args[position + 1..]);
            }
            run_and_report(command, &format!("fuzz run {target}"), dry_run, json_output)
        }
        (Some("list"), _) => {
            for target in target_names(&root) {
                println!("{target}");
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: cargo dfv fuzz <build | run <target> [-- libfuzzer-args] | list>");
            eprintln!("Targets:");
            for target in target_names(&root) {
                eprintln!("  {target}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_and_report(mut command: Command, label: &str, dry_run: bool, json_output: bool) -> ExitCode {
    if dry_run {
        let rendered = format!(
            "{} {:?}",
            command.get_program().to_string_lossy(),
            command.get_args().collect::<Vec<_>>()
        );
        if json_output {
            println!("{{\"command\": \"{rendered}\", \"pass\": true}}");
        } else {
            println!("Dry run - would execute: {rendered}");
        }
        return ExitCode::SUCCESS;
    }

    let start = Instant::now();
    let status = command.status();
    let duration = start.elapsed().as_secs_f64();
    let pass = status.as_ref().is_ok_and(|s| s.success());

    if json_output {
        println!(
            "{{\"command\": \"{label}\", \"pass\": {pass}, \"duration_seconds\": {duration:.2}}}"
        );
    } else if pass {
        println!("✓ {label} passed in {duration:.1}s");
    } else {
        eprintln!("✗ {label} failed in {duration:.1}s");
    }
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
