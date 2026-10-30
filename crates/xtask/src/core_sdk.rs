//! Bounded installed SDK qualification, separate from extension product acceptance.
use crate::doctor::run_doctor_internal;
use crate::report::{create_run_directory, workspace_root};
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

pub fn run(args: &[String]) -> ExitCode {
    if args
        .iter()
        .any(|arg| !matches!(arg.as_str(), "--core-sdk" | "--json" | "--dry-run"))
    {
        eprintln!("Core SDK qualification accepts --core-sdk, --json and --dry-run only.");
        return ExitCode::FAILURE;
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        println!("{{\"pass\":false,\"executed\":false,\"dry_run\":true,\"scope\":\"core-sdk\"}}");
        return ExitCode::SUCCESS;
    }
    if !run_doctor_internal().pass {
        eprintln!("Environment is not ready; run cargo dfv doctor and apply its repairs.");
        return ExitCode::FAILURE;
    }
    let directory = match create_run_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Cannot retain Core SDK qualification evidence: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = workspace_root();
    let json_output = args.iter().any(|arg| arg == "--json");
    for (tool, script) in [
        ("python3", "scripts/check-core-contract.py"),
        ("python3", "scripts/test-core-contract.py"),
        ("node", "scripts/qualify-core-sdk.mjs"),
    ] {
        let result = Command::new(tool).arg(script).current_dir(&root).output();
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                eprintln!("Cannot start {script}: {error}");
                return ExitCode::FAILURE;
            }
        };
        let name = script.rsplit('/').next().unwrap_or("qualification");
        let mut bytes = output.stdout.clone();
        bytes.extend_from_slice(&output.stderr);
        if fs::write(directory.join(format!("{name}.log")), bytes).is_err() {
            eprintln!("Cannot retain {script} evidence.");
            return ExitCode::FAILURE;
        }
        if !json_output {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.status.success() {
            eprintln!("{script} failed; see {}", directory.display());
            return ExitCode::FAILURE;
        }
        if script.ends_with("qualify-core-sdk.mjs") {
            let summary = String::from_utf8_lossy(&output.stdout)
                .lines()
                .last()
                .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok());
            if !summary
                .as_ref()
                .is_some_and(|summary| passing_summary(&root, summary))
            {
                eprintln!("Core SDK qualification omitted its executed passing report.");
                return ExitCode::FAILURE;
            }
            if json_output {
                println!("{}", summary.unwrap_or_default());
            }
        }
    }
    ExitCode::SUCCESS
}

// Reject dry-run, partial, unrelated or out-of-scope reports even if the child
// process happened to exit zero. Qualification evidence stays in its owned tree.
fn passing_summary(root: &Path, summary: &serde_json::Value) -> bool {
    if summary["pass"].as_bool() != Some(true)
        || summary["executed"].as_bool() != Some(true)
        || summary["dry_run"].as_bool() == Some(true)
    {
        return false;
    }
    let Some(path) = summary["report"]
        .as_str()
        .and_then(|p| fs::canonicalize(p).ok())
    else {
        return false;
    };
    let Ok(evidence) = fs::canonicalize(root.join("target/core-sdk-qualification")) else {
        return false;
    };
    if !path.starts_with(evidence) || path.file_name().is_none_or(|n| n != "core-sdk-gate.json") {
        return false;
    }
    let Some(value) = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return false;
    };
    let digest = |name: &str| {
        value[name].as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    };
    value["passed"].as_bool() == Some(true)
        && value["scope"].as_str() == Some("Core SDK contracts and explicit-approved-value recall")
        && value["extensionAcceptance"].as_bool() == Some(false)
        && value["accuracyThresholdApplied"].as_bool() == Some(false)
        && value["recordsMeasured"].as_u64().is_some_and(|n| n > 0)
        && value["exactAccuracy"].is_object()
        && value["recallPhases"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
        && digest("sourceInputsSha256")
        && digest("receiptSha256")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn zero_exit_or_passing_flag_without_owned_complete_evidence_cannot_qualify() {
        let root = std::env::temp_dir().join(format!(
            "dfv-core-sdk-report-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let evidence = root.join("target/core-sdk-qualification/current");
        fs::create_dir_all(&evidence).unwrap();
        let path = evidence.join("core-sdk-gate.json");
        let report = json!({"passed":true,"scope":"Core SDK contracts and explicit-approved-value recall",
            "extensionAcceptance":false,"accuracyThresholdApplied":false,"recordsMeasured":64,
            "exactAccuracy":{},"recallPhases":[{}],"sourceInputsSha256":"a".repeat(64),"receiptSha256":"b".repeat(64)});
        let summary = json!({"pass":true,"executed":true,"report":path});
        fs::write(&path, report.to_string()).unwrap();
        assert!(passing_summary(&root, &summary));
        for partial in [
            json!({}),
            json!({"pass":true}),
            json!({"pass":true,"executed":false,"report":path}),
            json!({"pass":true,"executed":true,"dry_run":true,"report":path}),
        ] {
            assert!(!passing_summary(&root, &partial));
        }
        for (key, wrong) in [
            ("passed", json!(false)),
            ("scope", json!("extension acceptance")),
            ("extensionAcceptance", json!(true)),
            ("accuracyThresholdApplied", json!(true)),
            ("recordsMeasured", json!(0)),
            ("recallPhases", json!([])),
            ("receiptSha256", json!("missing")),
        ] {
            let mut wrong_report = report.clone();
            wrong_report[key] = wrong;
            fs::write(&path, wrong_report.to_string()).unwrap();
            assert!(!passing_summary(&root, &summary), "{key}");
        }
        fs::write(&path, "not JSON").unwrap();
        assert!(!passing_summary(&root, &summary));
        let outside = root.join("core-sdk-gate.json");
        fs::write(&outside, report.to_string()).unwrap();
        assert!(!passing_summary(
            &root,
            &json!({"pass":true,"executed":true,"report":outside})
        ));
        fs::remove_file(&path).unwrap();
        assert!(!passing_summary(&root, &summary));
        fs::remove_dir_all(root).unwrap();
    }
}
