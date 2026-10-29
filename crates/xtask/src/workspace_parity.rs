//! Blocking installed-package workspace parity. No SDK may be skipped.
use crate::doctor::run_doctor_internal;
use crate::report::{create_run_directory, workspace_root};
use serde_json::{Value, json};
use std::fs;
use std::process::{Command, ExitCode};

fn failure(category: &str, json_output: bool) -> ExitCode {
    if json_output {
        println!("{}", json!({"pass":false,"error":category}));
    } else {
        eprintln!("Workspace parity failed: {category}");
    }
    ExitCode::FAILURE
}

pub fn run_workspace_parity(args: &[String], json_output: bool) -> ExitCode {
    if args
        .iter()
        .any(|arg| !["--workspace", "--json", "--dry-run"].contains(&arg.as_str()))
    {
        return failure(
            "workspace parity requires the complete four-SDK matrix",
            json_output,
        );
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        println!(
            "{}",
            json!({"pass":false,"dry_run":true,"executed":false,"sdks":["rust","python","node","wasm"]})
        );
        return ExitCode::SUCCESS;
    }
    let root = workspace_root();
    let run = match create_run_directory() {
        Ok(run) => run.join("workspace-parity"),
        Err(_) => return failure("report directory unavailable", json_output),
    };
    if fs::create_dir_all(&run).is_err() {
        return failure("report directory unavailable", json_output);
    }
    let doctor = run_doctor_internal();
    if serde_json::to_vec_pretty(&doctor)
        .ok()
        .and_then(|bytes| fs::write(run.join("doctor.json"), bytes).ok())
        .is_none()
    {
        return failure("doctor receipt unavailable", json_output);
    }
    if !doctor.pass {
        return failure(
            "environment is not ready; run cargo dfv doctor",
            json_output,
        );
    }
    let status = Command::new("node")
        .arg(root.join("scripts/test-workspace-parity.mjs"))
        .arg("--output")
        .arg(&run)
        .current_dir(&root)
        .output();
    let status = match status {
        Ok(status) => status,
        Err(_) => return failure("workspace parity runtime could not start", json_output),
    };
    let report = fs::read(run.join("workspace-parity.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let Some(report) = report else {
        return failure(
            "workspace parity receipt is missing or malformed",
            json_output,
        );
    };
    let pass = status.status.success() && complete_workspace_report(&report);
    if json_output {
        println!("{report}");
    } else if pass {
        println!(
            "Workspace parity passed for Rust, installed Python/Node and browser WASM.\nReport: {}",
            run.display()
        );
    } else {
        eprintln!("Workspace parity failed. Report: {}", run.display());
    }
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn complete_workspace_report(report: &Value) -> bool {
    report["pass"] == true
        && report["sdks"] == json!(["rust", "python", "node", "wasm"])
        && report["profiles"]["foundation"]["profile"] == "launch-workspace-current"
        && ["foundation", "streaming", "operations"]
            .iter()
            .all(|profile| {
                ["cases", "steps"].iter().all(|field| {
                    report["profiles"][profile][field]
                        .as_u64()
                        .is_some_and(|value| value > 0)
                })
            })
        && report["profiles"]["operations"]["profile"] == "workspace-operations-v1"
        && report["profiles"]["streaming"]["profile"] == "workspace-streaming-v1"
        && report["profiles"]["streaming"]["modes"] == json!(["buffered", "progressive"])
        && report["profiles"]["streaming"]["steps"]
            .as_u64()
            .is_some_and(|value| value > 0)
        && report["cases"].as_u64().is_some_and(|value| value > 0)
        && report["steps"].as_u64().is_some_and(|value| value > 0)
        && report["errors"].as_array().is_some_and(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_mandatory_profile_must_be_present_and_executed() {
        let report = json!({"pass":true,"sdks":["rust","python","node","wasm"],"profiles":{
            "foundation":{"profile":"launch-workspace-current","cases":17,"steps":156},
            "streaming":{"profile":"workspace-streaming-v1","modes":["buffered","progressive"],"cases":13,"steps":80},
            "operations":{"profile":"workspace-operations-v1","cases":7,"steps":185}},
            "cases":37,"steps":421,"errors":[]});
        assert!(complete_workspace_report(&report));
        for profile in ["foundation", "streaming", "operations"] {
            let mut missing = report.clone();
            missing["profiles"].as_object_mut().unwrap().remove(profile);
            assert!(!complete_workspace_report(&missing));
        }
        for profile in ["foundation", "streaming", "operations"] {
            for field in ["cases", "steps"] {
                let mut empty = report.clone();
                empty["profiles"][profile][field] = json!(0);
                assert!(!complete_workspace_report(&empty));
            }
        }
    }
}
