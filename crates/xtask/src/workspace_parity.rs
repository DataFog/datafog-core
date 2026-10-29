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
    if !args.iter().any(|arg| arg == "--workspace") {
        return failure("memory diagnostic requires --workspace", json_output);
    }
    if args.iter().any(|arg| {
        ![
            "--workspace",
            "--json",
            "--dry-run",
            "--plan-memory-diagnostic",
        ]
        .contains(&arg.as_str())
    }) {
        return failure(
            "workspace parity requires the complete four-SDK matrix",
            json_output,
        );
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        println!(
            "{}",
            json!({"pass":false,"dry_run":true,"executed":false,"planMemoryDiagnosticRequested":args.iter().any(|arg|arg=="--plan-memory-diagnostic"),"sdks":["rust","python","node","wasm"]})
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
    let mut command = Command::new("node");
    command
        .arg(root.join("scripts/test-workspace-parity.mjs"))
        .arg("--output")
        .arg(&run);
    if args.iter().any(|arg| arg == "--plan-memory-diagnostic") {
        command.arg("--plan-memory-diagnostic");
    }
    let status = command.current_dir(&root).output();
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
            "Workspace SDK parity passed for Rust, installed Python/Node and browser WASM.\nNative server profile passed separately for Python/Node encrypted SQLite hosts only.\nReport: {}",
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
    let four_sdk_total = |field: &str| {
        ["foundation", "streaming", "operations", "conversations"]
            .iter()
            .try_fold(0_u64, |sum, profile| {
                sum.checked_add(report["profiles"][profile][field].as_u64()?)
            })
    };
    report["pass"] == true
        && report["sdks"] == json!(["rust", "python", "node", "wasm"])
        && report["profiles"]["foundation"]["profile"] == "launch-workspace-current"
        && report["profiles"]["nativeServer"]["profile"] == "native-server-cas-v1"
        && report["profiles"]["nativeServer"]["sdks"] == json!(["python", "node"])
        && report["profiles"]["nativeServer"]["oracleSdk"] == "rust"
        && report["profiles"]["nativeServer"]["planPressure"]["pass"] == true
        && report["profiles"]["nativeServer"]["planPressure"]["profile"]
            == "native-plan-pressure-v1"
        && report["profiles"]["nativeServer"]["planPressure"]["sdks"] == json!(["python", "node"])
        && ["foundation", "streaming", "operations", "conversations"]
            .iter()
            .all(|profile| {
                report["profiles"][profile]["sdks"] == json!(["rust", "python", "node", "wasm"])
            })
        && ["foundation", "streaming", "operations", "conversations", "nativeServer"]
            .iter()
            .all(|profile| {
                ["cases", "steps"].iter().all(|field| {
                    report["profiles"][profile][field]
                        .as_u64()
                        .is_some_and(|value| value > 0)
                })
            })
        && report["profiles"]["conversations"]["profile"] == "workspace-conversations-v1"
        && report["profiles"]["nativeServer"]["conversationHost"]["profile"] == "native-conversation-host-v1"
        && report["profiles"]["nativeServer"]["conversationHost"]["sdks"] == json!(["python", "node"])
        && report["profiles"]["nativeServer"]["conversationHost"]["oracleSdk"] == "rust"
        && ["cases", "steps"].iter().all(|key| report["profiles"]["nativeServer"]["conversationHost"][key].as_u64().is_some_and(|value| value > 0))
        && report["profiles"]["nativeServer"]["conversationHost"]["existingSuites"]["python"]["passed"] == true
        && report["profiles"]["nativeServer"]["conversationHost"]["existingSuites"]["python"]["tests"].as_u64().is_some_and(|value| value >= 27)
        && report["profiles"]["nativeServer"]["conversationHost"]["existingSuites"]["node"]["passed"] == true
        && report["profiles"]["nativeServer"]["conversationHost"]["existingSuites"]["node"]["checks"].as_u64().is_some_and(|value| value >= 17)
        && report["profiles"]["operations"]["profile"] == "workspace-operations-v1"
        && report["profiles"]["streaming"]["profile"] == "workspace-streaming-v1"
        && report["profiles"]["streaming"]["modes"] == json!(["buffered", "progressive"])
        && report["profiles"]["streaming"]["steps"]
            .as_u64()
            .is_some_and(|value| value > 0)
        && report["cases"]
            .as_u64()
            .is_some_and(|value| Some(value) == four_sdk_total("cases"))
        && report["steps"]
            .as_u64()
            .is_some_and(|value| Some(value) == four_sdk_total("steps"))
        && report["errors"].as_array().is_some_and(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_mandatory_profile_must_be_present_and_executed() {
        let report = json!({"pass":true,"sdks":["rust","python","node","wasm"],"profiles":{
            "foundation":{"sdks":["rust","python","node","wasm"],"profile":"launch-workspace-current","cases":17,"steps":156},
            "streaming":{"sdks":["rust","python","node","wasm"],"profile":"workspace-streaming-v1","modes":["buffered","progressive"],"cases":13,"steps":80},
            "operations":{"sdks":["rust","python","node","wasm"],"profile":"workspace-operations-v1","cases":7,"steps":185},
            "conversations":{"sdks":["rust","python","node","wasm"],"profile":"workspace-conversations-v1","cases":8,"steps":179},
            "nativeServer":{"profile":"native-server-cas-v1","sdks":["python","node"],"oracleSdk":"rust","planPressure":{"pass":true,"profile":"native-plan-pressure-v1","sdks":["python","node"]},"conversationHost":{"profile":"native-conversation-host-v1","sdks":["python","node"],"oracleSdk":"rust","cases":16,"steps":66,"existingSuites":{"python":{"passed":true,"tests":27},"node":{"passed":true,"checks":17}}},"cases":9,"steps":28}},
            "cases":45,"steps":600,"errors":[]});
        assert!(complete_workspace_report(&report));
        let mut folded = report.clone();
        folded["cases"] = json!(70);
        folded["steps"] = json!(694);
        assert!(!complete_workspace_report(&folded));
        let mut wrong_scope = report.clone();
        wrong_scope["profiles"]["nativeServer"]["sdks"] = json!(["rust", "python", "node", "wasm"]);
        assert!(!complete_workspace_report(&wrong_scope));
        for key in [
            "profile",
            "sdks",
            "oracleSdk",
            "cases",
            "steps",
            "existingSuites",
        ] {
            let mut missing = report.clone();
            missing["profiles"]["nativeServer"]["conversationHost"]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(!complete_workspace_report(&missing));
        }
        for key in ["cases", "steps"] {
            let mut empty = report.clone();
            empty["profiles"]["nativeServer"]["conversationHost"][key] = json!(0);
            assert!(!complete_workspace_report(&empty));
        }
        let mut wrong_host_scope = report.clone();
        wrong_host_scope["profiles"]["nativeServer"]["conversationHost"]["sdks"] =
            json!(["rust", "python", "node", "wasm"]);
        assert!(!complete_workspace_report(&wrong_host_scope));
        let mut skipped_original = report.clone();
        skipped_original["profiles"]["nativeServer"]["conversationHost"]["existingSuites"]["python"]
            ["tests"] = json!(26);
        assert!(!complete_workspace_report(&skipped_original));
        for profile in [
            "foundation",
            "streaming",
            "operations",
            "conversations",
            "nativeServer",
        ] {
            let mut missing = report.clone();
            missing["profiles"].as_object_mut().unwrap().remove(profile);
            assert!(!complete_workspace_report(&missing));
        }
        for profile in [
            "foundation",
            "streaming",
            "operations",
            "conversations",
            "nativeServer",
        ] {
            for field in ["cases", "steps"] {
                let mut empty = report.clone();
                empty["profiles"][profile][field] = json!(0);
                assert!(!complete_workspace_report(&empty));
            }
        }
    }
}
