use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub pass: bool,
    pub sdks: HashMap<String, SdkBuildResult>,
    pub duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_directory: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkBuildResult {
    pub pass: bool,
    pub duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

fn build_rust(dry_run: bool) -> SdkBuildResult {
    let cmd = "cargo build --package datafog-core --release";

    if dry_run {
        return SdkBuildResult {
            pass: true,
            duration_seconds: 0.0,
            error: None,
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();

    let status = Command::new("cargo")
        .args(["build", "--package", "datafog-core", "--release"])
        .current_dir(&root)
        .status();

    let duration = start.elapsed().as_secs_f64();

    match status {
        Ok(s) if s.success() => SdkBuildResult {
            pass: true,
            duration_seconds: duration,
            error: None,
            command: Some(cmd.to_string()),
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("cargo build failed with exit code {:?}", s.code())),
            command: Some(cmd.to_string()),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run cargo: {e}")),
            command: Some(cmd.to_string()),
        },
    }
}

fn build_python(dry_run: bool) -> SdkBuildResult {
    let cmd = "python3 -m maturin build --manifest-path bindings/python/Cargo.toml --release";

    if dry_run {
        return SdkBuildResult {
            pass: true,
            duration_seconds: 0.0,
            error: None,
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();

    let status = Command::new("python3")
        .args([
            "-m",
            "maturin",
            "build",
            "--manifest-path",
            "bindings/python/Cargo.toml",
            "--release",
        ])
        .current_dir(&root)
        .status();

    let duration = start.elapsed().as_secs_f64();

    match status {
        Ok(s) if s.success() => SdkBuildResult {
            pass: true,
            duration_seconds: duration,
            error: None,
            command: Some(cmd.to_string()),
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "maturin build failed with exit code {:?}",
                s.code()
            )),
            command: Some(cmd.to_string()),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run maturin: {e}")),
            command: Some(cmd.to_string()),
        },
    }
}

fn build_node(dry_run: bool) -> SdkBuildResult {
    let cmd = "npm ci && npm run build (in bindings/node)";

    if dry_run {
        return SdkBuildResult {
            pass: true,
            duration_seconds: 0.0,
            error: None,
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let node_dir = root.join("bindings").join("node");

    let npm_ci = Command::new("npm")
        .args(["ci"])
        .current_dir(&node_dir)
        .status();

    if !npm_ci.map(|s| s.success()).unwrap_or(false) {
        return SdkBuildResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            error: Some("npm ci failed".to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&node_dir)
        .status();

    let duration = start.elapsed().as_secs_f64();

    match status {
        Ok(s) if s.success() => SdkBuildResult {
            pass: true,
            duration_seconds: duration,
            error: None,
            command: Some(cmd.to_string()),
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "npm run build failed with exit code {:?}",
                s.code()
            )),
            command: Some(cmd.to_string()),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run npm: {e}")),
            command: Some(cmd.to_string()),
        },
    }
}

fn build_wasm(dry_run: bool) -> SdkBuildResult {
    let cmd = "npm ci && npm run build (in bindings/wasm)";

    if dry_run {
        return SdkBuildResult {
            pass: true,
            duration_seconds: 0.0,
            error: None,
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let npm_ci = Command::new("npm")
        .args(["ci"])
        .current_dir(&wasm_dir)
        .status();

    if !npm_ci.map(|s| s.success()).unwrap_or(false) {
        return SdkBuildResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            error: Some("npm ci failed".to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&wasm_dir)
        .status();

    let duration = start.elapsed().as_secs_f64();

    match status {
        Ok(s) if s.success() => SdkBuildResult {
            pass: true,
            duration_seconds: duration,
            error: None,
            command: Some(cmd.to_string()),
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "npm run build failed with exit code {:?}",
                s.code()
            )),
            command: Some(cmd.to_string()),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run npm: {e}")),
            command: Some(cmd.to_string()),
        },
    }
}

pub fn run_build(args: &[String], json_output: bool) -> ExitCode {
    let start = Instant::now();
    let dry_run = args.iter().any(|a| a == "--dry-run");

    let sdk_filter: Option<&str> = args
        .iter()
        .position(|a| a == "--sdk")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let sdks_to_build: Vec<&str> = match sdk_filter {
        Some("all") | None => vec!["rust", "python", "node", "wasm"],
        Some(sdk) => vec![sdk],
    };

    let mut results = HashMap::new();

    for sdk in &sdks_to_build {
        if !json_output {
            if dry_run {
                println!("Would build {sdk}:");
            } else {
                println!("Building {sdk}...");
            }
        }

        let result = match *sdk {
            "rust" => build_rust(dry_run),
            "python" => build_python(dry_run),
            "node" => build_node(dry_run),
            "wasm" => build_wasm(dry_run),
            _ => SdkBuildResult {
                pass: false,
                duration_seconds: 0.0,
                error: Some(format!("unknown SDK: {sdk}")),
                command: None,
            },
        };

        if !json_output {
            if dry_run {
                if let Some(ref cmd) = result.command {
                    println!("  {cmd}");
                }
            } else if result.pass {
                println!("  ✓ {sdk} built in {:.1}s", result.duration_seconds);
            } else {
                println!(
                    "  ✗ {sdk} failed: {}",
                    result.error.as_deref().unwrap_or("unknown error")
                );
            }
        }

        results.insert(sdk.to_string(), result);
    }

    let all_pass = results.values().all(|r| r.pass);
    let duration = start.elapsed().as_secs_f64();

    let run_dir = if !dry_run {
        create_run_directory().ok()
    } else {
        None
    };

    let build_result = BuildResult {
        pass: all_pass,
        sdks: results,
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        dry_run: if dry_run { Some(true) } else { None },
    };

    if let Some(ref dir) = run_dir {
        let report_path = dir.join("build.json");
        if let Ok(json) = serde_json::to_string_pretty(&build_result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&build_result).unwrap());
    } else if !dry_run {
        println!();
        if all_pass {
            println!("All builds passed in {duration:.1}s");
        } else {
            println!("Some builds failed");
        }

        if let Some(ref dir) = run_dir {
            println!("Report saved to: {}", dir.display());
        }
    }

    if all_pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
