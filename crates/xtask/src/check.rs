use crate::doctor::{CheckStatus, run_doctor_internal};
use crate::reach;
use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub pass: bool,
    pub doctor_pass: bool,
    pub tests: HashMap<String, TestSuiteResult>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor_failures: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachability_failures: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteResult {
    pub pass: bool,
    pub duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub failing_fixtures: Vec<String>,
    pub wrapped_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

fn run_rust_tests(run_dir: &Path, dry_run: bool) -> TestSuiteResult {
    let cmd = "cargo test --workspace --all-features";

    if dry_run {
        return TestSuiteResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            failing_fixtures: vec![],
            wrapped_script: None,
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();

    let output = Command::new("cargo")
        .args(["test", "--workspace", "--all-features"])
        .current_dir(&root)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("rust_tests.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                TestSuiteResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    failing_fixtures: vec![],
                    wrapped_script: None,
                    command: Some(cmd.to_string()),
                }
            } else {
                let failing = extract_failing_tests(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: None,
                    command: Some(cmd.to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run cargo test: {e}")),
            failing_fixtures: vec![],
            wrapped_script: None,
            command: Some(cmd.to_string()),
        },
    }
}

fn extract_failing_tests(output: &str) -> Vec<String> {
    let mut failures = Vec::new();
    for line in output.lines() {
        if line.contains("FAILED") || line.contains("test result: FAILED") {
            if let Some(name) = line.split_whitespace().next() {
                if name != "test" && !name.starts_with("---") {
                    failures.push(name.to_string());
                }
            }
        }
        if line.starts_with("---- ") && line.ends_with(" ----") {
            let test_name = line.trim_start_matches("---- ").trim_end_matches(" ----");
            if !test_name.contains("stdout") {
                failures.push(test_name.to_string());
            }
        }
    }
    failures.sort();
    failures.dedup();
    failures
}

fn run_python_tests(run_dir: &Path, dry_run: bool) -> TestSuiteResult {
    let cmd = "python bindings/python/tests/test_installed.py";
    let wrapped = "bindings/python/tests/test_installed.py";

    if dry_run {
        return TestSuiteResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let python_tests = root.join("bindings").join("python").join("tests");

    let venv_dir = root.join("target").join("dfv_venv");
    let python_bin = match crate::python_env::built_wheel(&root)
        .and_then(|wheel| crate::python_env::prepare(&venv_dir, &wheel))
    {
        Ok(python) => python,
        Err(error) => {
            return TestSuiteResult {
                pass: false,
                duration_seconds: start.elapsed().as_secs_f64(),
                output: Some(error.clone()),
                error: Some(error),
                failing_fixtures: vec![],
                wrapped_script: Some(wrapped.into()),
                command: Some(cmd.into()),
            };
        }
    };

    let output = Command::new(&python_bin)
        .arg(python_tests.join("test_installed.py"))
        .current_dir(&root)
        .env("PYTHONPATH", &python_tests)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("python_tests.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                TestSuiteResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    failing_fixtures: vec![],
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            } else {
                let failing = extract_python_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run python tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        },
    }
}

fn extract_python_failures(output: &str) -> Vec<String> {
    let mut failures = Vec::new();
    for line in output.lines() {
        if line.contains("AssertionError") {
            failures.push(line.to_string());
        }
        if line.contains("FAILED") || line.contains("Error:") {
            failures.push(line.trim().to_string());
        }
    }
    failures
}

fn run_node_tests(run_dir: &Path, dry_run: bool) -> TestSuiteResult {
    let cmd = "npm run test:package (in bindings/node)";
    let wrapped = "scripts/test-node-package.mjs";

    if dry_run {
        return TestSuiteResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let node_dir = root.join("bindings").join("node");

    let output = Command::new("npm")
        .args(["run", "test:package"])
        .current_dir(&node_dir)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("node_tests.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                TestSuiteResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    failing_fixtures: vec![],
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            } else {
                let failing = extract_js_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run node tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        },
    }
}

fn run_wasm_tests(run_dir: &Path, dry_run: bool) -> TestSuiteResult {
    let cmd = "npm run test:package (in bindings/wasm)";
    let wrapped = "scripts/test-wasm-package.mjs";

    if dry_run {
        return TestSuiteResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let output = Command::new("npm")
        .args(["run", "test:package"])
        .current_dir(&wasm_dir)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("wasm_tests.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                TestSuiteResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    failing_fixtures: vec![],
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            } else {
                let failing = extract_js_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some(wrapped.to_string()),
                    command: Some(cmd.to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run wasm tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some(wrapped.to_string()),
            command: Some(cmd.to_string()),
        },
    }
}

fn extract_js_failures(output: &str) -> Vec<String> {
    let mut failures = Vec::new();
    for line in output.lines() {
        if line.contains("Error:") || line.contains("AssertionError") {
            failures.push(line.trim().to_string());
        }
    }
    failures
}

fn print_test_result(name: &str, result: &TestSuiteResult) {
    if result.pass {
        println!("  ✓ {name} passed in {:.1}s", result.duration_seconds);
    } else {
        println!(
            "  ✗ {name} failed in {:.1}s: {}",
            result.duration_seconds,
            result.error.as_deref().unwrap_or("unknown")
        );
    }
    if let Some(ref script) = result.wrapped_script {
        println!("    (wraps {script})");
    }
}

pub fn run_check(args: &[String], json_output: bool) -> ExitCode {
    if args.iter().any(|a| a == "--models") {
        let msg = "Error: --models flag is not yet implemented.

The model-based detector tier (GLiNER2/DataFog models) will be added in a follow-up PR.
For now, only rule-based detectors are verified.

Run without --models to verify rule-based detectors:
    cargo dfv check";

        if json_output {
            let err = serde_json::json!({
                "pass": false,
                "error": "models_not_implemented",
                "message": msg
            });
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("{msg}");
        }
        return ExitCode::FAILURE;
    }

    let dry_run = args.iter().any(|a| a == "--dry-run");

    if !json_output && !dry_run {
        println!("Running environment check...");
    }

    let doctor_result = run_doctor_internal();

    if !doctor_result.pass && !dry_run {
        let failures: Vec<String> = doctor_result
            .checks
            .iter()
            .filter(|c| c.required && c.status == CheckStatus::Fail)
            .map(|c| {
                format!(
                    "{}: {} (fix: {})",
                    c.name,
                    c.found.as_deref().unwrap_or("missing"),
                    c.fix_command.as_deref().unwrap_or("see above")
                )
            })
            .collect();

        let run_dir = create_run_directory().ok();

        let check_result = CheckResult {
            pass: false,
            doctor_pass: false,
            tests: HashMap::new(),
            duration_seconds: 0.0,
            run_directory: run_dir.clone().unwrap_or_default(),
            doctor_failures: Some(failures.clone()),
            reachability_failures: None,
            dry_run: None,
        };

        if let Some(ref dir) = run_dir {
            let report_path = dir.join("check.json");
            if let Ok(json) = serde_json::to_string_pretty(&check_result) {
                let _ = fs::write(&report_path, json);
            }
        }

        if json_output {
            println!("{}", serde_json::to_string_pretty(&check_result).unwrap());
        } else {
            eprintln!();
            eprintln!("Environment check failed. Fix the following before running tests:");
            for f in &failures {
                eprintln!("  - {f}");
            }
            eprintln!();
            eprintln!("Run `cargo dfv doctor` for details.");

            if let Some(ref dir) = run_dir {
                eprintln!();
                eprintln!("Report saved to: {}", dir.display());
            }
        }

        return ExitCode::FAILURE;
    }

    if !json_output {
        if dry_run {
            println!("Dry run - would execute:");
        } else {
            println!("Environment OK. Running tests...");
            println!();
        }
    }

    // New crates and test modules are easy to add and easy to forget. Fail
    // before any test runs so the gap is the only thing to fix.
    let reachability_failures = reach::check_reachability(&workspace_root());
    if !reachability_failures.is_empty() && !dry_run {
        let run_dir = create_run_directory().ok();
        let check_result = CheckResult {
            pass: false,
            doctor_pass: true,
            tests: HashMap::new(),
            duration_seconds: 0.0,
            run_directory: run_dir.clone().unwrap_or_default(),
            doctor_failures: None,
            reachability_failures: Some(reachability_failures.clone()),
            dry_run: None,
        };
        if let Some(ref dir) = run_dir {
            if let Ok(json) = serde_json::to_string_pretty(&check_result) {
                let _ = fs::write(dir.join("check.json"), json);
            }
        }
        if json_output {
            println!("{}", serde_json::to_string_pretty(&check_result).unwrap());
        } else {
            eprintln!();
            eprintln!("Verification coverage check failed. New code must be reachable by dfv:");
            for failure in &reachability_failures {
                eprintln!("  - {failure}");
            }
            if let Some(ref dir) = run_dir {
                eprintln!();
                eprintln!("Report saved to: {}", dir.display());
            }
        }
        return ExitCode::FAILURE;
    }

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let mut tests = HashMap::new();

    if !json_output && !dry_run {
        println!("Running Rust tests...");
    }
    let rust_result = run_rust_tests(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!("  rust: {}", rust_result.command.as_deref().unwrap_or(""));
        } else {
            print_test_result("rust", &rust_result);
        }
    }
    tests.insert("rust".to_string(), rust_result);

    if !json_output && !dry_run {
        println!("Running Python tests...");
    }
    let python_result = run_python_tests(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  python: {} (wraps {})",
                python_result.command.as_deref().unwrap_or(""),
                python_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_test_result("python", &python_result);
        }
    }
    tests.insert("python".to_string(), python_result);

    if !json_output && !dry_run {
        println!("Running Node tests...");
    }
    let node_result = run_node_tests(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  node: {} (wraps {})",
                node_result.command.as_deref().unwrap_or(""),
                node_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_test_result("node", &node_result);
        }
    }
    tests.insert("node".to_string(), node_result);

    if !json_output && !dry_run {
        println!("Running WASM tests...");
    }
    let wasm_result = run_wasm_tests(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  wasm: {} (wraps {})",
                wasm_result.command.as_deref().unwrap_or(""),
                wasm_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_test_result("wasm", &wasm_result);
        }
    }
    tests.insert("wasm".to_string(), wasm_result);

    let all_pass = tests.values().all(|t| t.pass);
    let duration = start.elapsed().as_secs_f64();

    let check_result = CheckResult {
        pass: all_pass,
        doctor_pass: true,
        tests,
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        doctor_failures: None,
        reachability_failures: None,
        dry_run: if dry_run { Some(true) } else { None },
    };

    if !dry_run {
        let report_path = run_dir.join("check.json");
        if let Ok(json) = serde_json::to_string_pretty(&check_result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&check_result).unwrap());
    } else if !dry_run {
        println!();
        if all_pass {
            println!("All tests passed in {duration:.1}s");
        } else {
            println!("Some tests failed");
            for (name, result) in &check_result.tests {
                if !result.pass {
                    println!("  - {name}");
                    for fixture in &result.failing_fixtures {
                        println!("      {fixture}");
                    }
                }
            }
        }
        println!();
        println!("Report saved to: {}", run_dir.display());
    }

    if all_pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
