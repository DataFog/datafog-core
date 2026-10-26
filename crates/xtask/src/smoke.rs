use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeResult {
    pub pass: bool,
    pub tests: HashMap<String, SmokeTestResult>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeTestResult {
    pub pass: bool,
    pub duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub wrapped_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_result: Option<serde_json::Value>,
}

fn run_python_smoke(run_dir: &std::path::Path, dry_run: bool) -> SmokeTestResult {
    let script = "bindings/python/tests/test_installed.py";
    let cmd = "python test_installed.py (in fresh venv with installed wheel)";

    if dry_run {
        return SmokeTestResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let start = Instant::now();
    let root = workspace_root();

    let wheel_dir = root.join("target").join("wheels");
    let wheel = if wheel_dir.exists() {
        fs::read_dir(&wheel_dir).ok().and_then(|mut entries| {
            entries.find_map(|e| {
                e.ok().and_then(|entry| {
                    let name = entry.file_name();
                    if name.to_string_lossy().ends_with(".whl") {
                        Some(entry.path())
                    } else {
                        None
                    }
                })
            })
        })
    } else {
        None
    };

    let Some(wheel_path) = wheel else {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("No wheel found in target/wheels/".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    };

    let venv_dir = run_dir.join("python_smoke_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

    let venv_status = Command::new("python3")
        .args(["-m", "venv"])
        .arg(&venv_dir)
        .status();

    if !venv_status.map(|s| s.success()).unwrap_or(false) {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to create venv".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let pip_install = Command::new(&python_bin)
        .args(["-m", "pip", "install", "--force-reinstall"])
        .arg(&wheel_path)
        .output();

    if !pip_install.map(|o| o.status.success()).unwrap_or(false) {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to install wheel".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let smoke_test_code = r#"
import json
from datafog_core import scan

text = "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com."
findings = scan(text)
result = [{"entity_type": f.entity_type, "matched_text": f.matched_text, 
           "byte_start": f.byte_range.start, "byte_end": f.byte_range.end} for f in findings]
print(json.dumps({"pass": len(findings) > 0, "findings": result}))
"#;

    let output = Command::new(&python_bin)
        .args(["-c", smoke_test_code])
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("python_smoke.log");
            let _ = fs::write(&log_path, &combined);

            let scan_result: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
            let pass = scan_result
                .as_ref()
                .and_then(|v| v.get("pass"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false);

            if out.status.success() && pass {
                SmokeTestResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            } else {
                SmokeTestResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("Smoke test failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            }
        }
        Err(e) => SmokeTestResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("Failed to run smoke test: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        },
    }
}

fn run_node_smoke(run_dir: &std::path::Path, dry_run: bool) -> SmokeTestResult {
    let script = "scripts/test-node-package.mjs";
    let cmd = "npm install <tarball> && node smoke-test.mjs";

    if dry_run {
        return SmokeTestResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let node_dir = root.join("bindings").join("node");

    let packed = Command::new("npm")
        .args(["pack", "--json"])
        .current_dir(&node_dir)
        .output();

    let tarball_path = match packed {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&stdout);
            parsed.ok().and_then(|arr| {
                arr.first()
                    .and_then(|v| v.get("filename"))
                    .and_then(|f| f.as_str())
                    .map(|f| node_dir.join(f))
            })
        }
        _ => None,
    };

    let Some(tarball) = tarball_path else {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to pack Node tarball".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    };

    let temp_dir = run_dir.join("node_smoke_project");
    let _ = fs::create_dir_all(&temp_dir);

    let package_json = r#"{"name": "smoke-test", "private": true, "type": "module"}"#;
    let _ = fs::write(temp_dir.join("package.json"), package_json);

    let npm_install = Command::new("npm")
        .args(["install", "--ignore-scripts"])
        .arg(&tarball)
        .current_dir(&temp_dir)
        .output();

    if !npm_install.map(|o| o.status.success()).unwrap_or(false) {
        let _ = fs::remove_file(&tarball);
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to install Node tarball".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let smoke_test_code = r#"
import { scan } from "@datafog/node";
const text = "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.";
const findings = scan(text);
const result = findings.map(f => ({
  entity_type: f.entityType,
  matched_text: f.matchedText,
  byte_start: f.byteRange.start,
  byte_end: f.byteRange.end
}));
console.log(JSON.stringify({pass: findings.length > 0, findings: result}));
"#;

    let _ = fs::write(temp_dir.join("smoke.mjs"), smoke_test_code);

    let output = Command::new("node")
        .arg("smoke.mjs")
        .current_dir(&temp_dir)
        .output();

    let _ = fs::remove_file(&tarball);
    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("node_smoke.log");
            let _ = fs::write(&log_path, &combined);

            let scan_result: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
            let pass = scan_result
                .as_ref()
                .and_then(|v| v.get("pass"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false);

            if out.status.success() && pass {
                SmokeTestResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            } else {
                SmokeTestResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("Smoke test failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            }
        }
        Err(e) => SmokeTestResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("Failed to run smoke test: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        },
    }
}

fn run_wasm_smoke(run_dir: &std::path::Path, dry_run: bool) -> SmokeTestResult {
    let script = "scripts/test-wasm-package.mjs";
    let cmd = "npx playwright chromium <wasm-smoke-test>";

    if dry_run {
        return SmokeTestResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let dist_dir = wasm_dir.join("dist");
    if !dist_dir.exists() {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("WASM dist/ not found. Run cargo dfv build --sdk wasm first.".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let packed = Command::new("npm")
        .args(["pack", "--json"])
        .current_dir(&wasm_dir)
        .output();

    let tarball_path = match packed {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&stdout);
            parsed.ok().and_then(|arr| {
                arr.first()
                    .and_then(|v| v.get("filename"))
                    .and_then(|f| f.as_str())
                    .map(|f| wasm_dir.join(f))
            })
        }
        _ => None,
    };

    let Some(tarball) = tarball_path else {
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to pack WASM tarball".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    };

    let temp_dir = run_dir.join("wasm_smoke_project");
    let _ = fs::create_dir_all(&temp_dir);

    let package_json = r#"{"name": "wasm-smoke-test", "private": true, "type": "module"}"#;
    let _ = fs::write(temp_dir.join("package.json"), package_json);

    let npm_install = Command::new("npm")
        .args(["install", "--ignore-scripts"])
        .arg(&tarball)
        .current_dir(&temp_dir)
        .output();

    if !npm_install.map(|o| o.status.success()).unwrap_or(false) {
        let _ = fs::remove_file(&tarball);
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to install WASM tarball".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let smoke_test_code = r#"
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import path from "node:path";

const tempDir = process.argv[2];
const port = 8765;

function contentType(file) {
  if (file.endsWith(".html")) return "text/html; charset=utf-8";
  if (file.endsWith(".js") || file.endsWith(".mjs")) return "text/javascript; charset=utf-8";
  if (file.endsWith(".wasm")) return "application/wasm";
  return "application/octet-stream";
}

const server = createServer((req, res) => {
  const pathname = new URL(req.url, "http://localhost").pathname;
  const file = path.resolve(tempDir, `.${pathname === "/" ? "/index.html" : pathname}`);
  try {
    res.writeHead(200, { "content-type": contentType(file) });
    res.end(readFileSync(file));
  } catch {
    res.writeHead(404).end();
  }
});

server.listen(port, "127.0.0.1", async () => {
  const { chromium } = await import("playwright");
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${port}`);
    const result = await page.evaluate(async () => {
      const { init, scan } = await import("/node_modules/@datafog/wasm/index.js");
      await init();
      const text = "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.";
      const findings = scan(text);
      return {
        pass: findings.length > 0,
        findings: findings.map(f => ({
          entity_type: f.entityType,
          matched_text: f.matchedText,
          byte_start: f.byteRange.start,
          byte_end: f.byteRange.end
        }))
      };
    });
    console.log(JSON.stringify(result));
  } finally {
    await browser.close();
    server.close();
  }
});
"#;

    let _ = fs::write(temp_dir.join("smoke-server.mjs"), smoke_test_code);
    let _ = fs::write(
        temp_dir.join("index.html"),
        "<!doctype html><title>WASM smoke test</title>",
    );

    let playwright_npm_install = Command::new("npm")
        .args(["install", "playwright"])
        .current_dir(&temp_dir)
        .output();

    if !playwright_npm_install
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&tarball);
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to install playwright npm package".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let playwright_install = Command::new("npx")
        .args(["playwright", "install", "--with-deps", "chromium"])
        .current_dir(&temp_dir)
        .status();

    if !playwright_install.map(|s| s.success()).unwrap_or(false) {
        let _ = fs::remove_file(&tarball);
        return SmokeTestResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("Failed to install Playwright Chromium".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        };
    }

    let output = Command::new("node")
        .args(["smoke-server.mjs", temp_dir.to_str().unwrap()])
        .current_dir(&temp_dir)
        .output();

    let _ = fs::remove_file(&tarball);
    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("wasm_smoke.log");
            let _ = fs::write(&log_path, &combined);

            let scan_result: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();
            let pass = scan_result
                .as_ref()
                .and_then(|v| v.get("pass"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false);

            if out.status.success() && pass {
                SmokeTestResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            } else {
                SmokeTestResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("Smoke test failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                    scan_result,
                }
            }
        }
        Err(e) => SmokeTestResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("Failed to run smoke test: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
            scan_result: None,
        },
    }
}

fn print_smoke_result(name: &str, result: &SmokeTestResult) {
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

pub fn run_smoke(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");

    let sdk_filter: Option<&str> = args
        .iter()
        .position(|a| a == "--sdk")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    if !json_output && !dry_run {
        println!("Running smoke tests (installed package verification)...");
        println!();
    }

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let mut tests = HashMap::new();

    let should_run_python = sdk_filter.is_none() || sdk_filter == Some("python");
    let should_run_node = sdk_filter.is_none() || sdk_filter == Some("node");
    let should_run_wasm = sdk_filter.is_none() || sdk_filter == Some("wasm");

    if should_run_python {
        if !json_output && !dry_run {
            println!("Running Python smoke test...");
        }
        let python_result = run_python_smoke(&run_dir, dry_run);
        if !json_output {
            if dry_run {
                println!(
                    "  python: {} (wraps {})",
                    python_result.command.as_deref().unwrap_or(""),
                    python_result.wrapped_script.as_deref().unwrap_or("")
                );
            } else {
                print_smoke_result("python", &python_result);
            }
        }
        tests.insert("python".to_string(), python_result);
    }

    if should_run_node {
        if !json_output && !dry_run {
            println!("Running Node smoke test...");
        }
        let node_result = run_node_smoke(&run_dir, dry_run);
        if !json_output {
            if dry_run {
                println!(
                    "  node: {} (wraps {})",
                    node_result.command.as_deref().unwrap_or(""),
                    node_result.wrapped_script.as_deref().unwrap_or("")
                );
            } else {
                print_smoke_result("node", &node_result);
            }
        }
        tests.insert("node".to_string(), node_result);
    }

    if should_run_wasm {
        if !json_output && !dry_run {
            println!("Running WASM smoke test (headless Chromium)...");
        }
        let wasm_result = run_wasm_smoke(&run_dir, dry_run);
        if !json_output {
            if dry_run {
                println!(
                    "  wasm: {} (wraps {})",
                    wasm_result.command.as_deref().unwrap_or(""),
                    wasm_result.wrapped_script.as_deref().unwrap_or("")
                );
            } else {
                print_smoke_result("wasm", &wasm_result);
            }
        }
        tests.insert("wasm".to_string(), wasm_result);
    }

    let all_pass = tests.values().all(|t| t.pass);
    let duration = start.elapsed().as_secs_f64();

    let result = SmokeResult {
        pass: all_pass,
        tests,
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        dry_run: if dry_run { Some(true) } else { None },
    };

    if !dry_run {
        let report_path = run_dir.join("smoke.json");
        if let Ok(json) = serde_json::to_string_pretty(&result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else if !dry_run {
        println!();
        if all_pass {
            println!("✓ All smoke tests passed in {duration:.1}s");
        } else {
            println!("✗ Some smoke tests failed");
            for (name, test) in &result.tests {
                if !test.pass {
                    println!("  - {name}: {}", test.error.as_deref().unwrap_or("unknown"));
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
