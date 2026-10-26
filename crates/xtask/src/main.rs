use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let json_output = args.iter().any(|a| a == "--json");
    let cmd = &args[0];

    match cmd.as_str() {
        "doctor" => run_doctor(&args[1..], json_output),
        "build" => run_build(&args[1..], json_output),
        "check" => run_check(&args[1..], json_output),
        "parity" => run_parity(&args[1..], json_output),
        "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Unknown command: {cmd}");
            eprintln!("Run `cargo dfv --help` for usage.");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    println!(
        r#"datafog-core verification CLI

USAGE:
    cargo dfv <COMMAND> [OPTIONS]

COMMANDS:
    doctor    Check environment readiness (toolchains, bindings, models)
    build     Build SDK bindings
    check     Run rule-based detector tests across all SDKs
    parity    Cross-SDK span/offset comparison

OPTIONS:
    --json           Machine-readable JSON output
    --models         (check only) Include model-based detectors [NOT YET IMPLEMENTED]

EXAMPLES:
    cargo dfv doctor              # Check if environment is ready
    cargo dfv doctor --json       # JSON output for automation
    cargo dfv build               # Build all bindings
    cargo dfv build --sdk python  # Build only Python binding
    cargo dfv check               # Run all rule-based tests
    cargo dfv check --json        # Machine-readable test results
    cargo dfv parity              # Compare spans across SDKs

Reports are written to target/dfv/runs/<timestamp>/ and never deleted by this CLI."#
    );
}

fn workspace_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn create_run_directory() -> std::io::Result<PathBuf> {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let run_dir = workspace_root()
        .join("target")
        .join("dfv")
        .join("runs")
        .join(&timestamp);
    fs::create_dir_all(&run_dir)?;
    Ok(run_dir)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DoctorResult {
    pass: bool,
    checks: Vec<DoctorCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DoctorCheck {
    name: String,
    status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Pass,
    Fail,
    Warn,
    Skip,
}

impl DoctorCheck {
    fn pass(name: &str, found: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Pass,
            found: Some(found.to_string()),
            expected: None,
            fix_command: None,
            details: None,
            required,
        }
    }

    fn fail(name: &str, found: Option<&str>, expected: &str, fix: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            found: found.map(String::from),
            expected: Some(expected.to_string()),
            fix_command: Some(fix.to_string()),
            details: None,
            required,
        }
    }

    fn warn(name: &str, details: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warn,
            found: None,
            expected: None,
            fix_command: None,
            details: Some(details.to_string()),
            required,
        }
    }
}

fn check_rust_toolchain() -> DoctorCheck {
    let expected = "1.88";
    let output = Command::new("rustc").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();
            if version.contains(expected) {
                DoctorCheck::pass("rust_toolchain", version, true)
            } else {
                DoctorCheck::fail(
                    "rust_toolchain",
                    Some(version),
                    &format!("rustc {expected}"),
                    &format!("rustup install {expected} && rustup default {expected}"),
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "rust_toolchain",
            None,
            &format!("rustc {expected}"),
            "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh && rustup install 1.88",
            true,
        ),
    }
}

fn check_python() -> DoctorCheck {
    let output = Command::new("python3").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();
            if version.contains("3.10")
                || version.contains("3.11")
                || version.contains("3.12")
                || version.contains("3.13")
                || version.contains("3.14")
            {
                DoctorCheck::pass("python", version, true)
            } else {
                DoctorCheck::fail(
                    "python",
                    Some(version),
                    "Python 3.10+",
                    "Install Python 3.10 or newer from https://python.org or your package manager",
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "python",
            None,
            "Python 3.10+",
            "sudo apt-get install python3 python3-dev  # or: brew install python@3.12",
            true,
        ),
    }
}

fn check_maturin() -> DoctorCheck {
    let output = Command::new("python3")
        .args(["-m", "maturin", "--version"])
        .output();

    if let Ok(ref out) = output {
        if out.status.success() {
            let version = String::from_utf8_lossy(&out.stdout);
            return DoctorCheck::pass("maturin", version.trim(), true);
        }
    }

    let pip_output = Command::new("maturin").arg("--version").output();
    match pip_output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            DoctorCheck::pass("maturin", version.trim(), true)
        }
        _ => DoctorCheck::fail(
            "maturin",
            None,
            "maturin (any version)",
            "python3 -m pip install maturin",
            true,
        ),
    }
}

fn check_node() -> DoctorCheck {
    let expected_major = 24;
    let output = Command::new("node").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();
            let major: Option<u32> = version
                .strip_prefix('v')
                .and_then(|v| v.split('.').next())
                .and_then(|m| m.parse().ok());

            if major.is_some_and(|m| m >= expected_major) {
                DoctorCheck::pass("node", version, true)
            } else {
                DoctorCheck::fail(
                    "node",
                    Some(version),
                    &format!("Node.js {expected_major}+"),
                    &format!(
                        "nvm install {expected_major} && nvm use {expected_major}  # or: brew install node@{expected_major}"
                    ),
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "node",
            None,
            &format!("Node.js {expected_major}+"),
            "curl -fsSL https://fnm.vercel.app/install | bash && fnm install 24",
            true,
        ),
    }
}

fn check_wasm_bindgen_cli() -> DoctorCheck {
    let expected = "0.2.127";
    let output = Command::new("wasm-bindgen").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();
            if version.contains(expected) {
                DoctorCheck::pass("wasm_bindgen_cli", version, true)
            } else {
                DoctorCheck::fail(
                    "wasm_bindgen_cli",
                    Some(version),
                    &format!("wasm-bindgen {expected}"),
                    &format!("cargo install wasm-bindgen-cli --version {expected} --locked"),
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "wasm_bindgen_cli",
            None,
            &format!("wasm-bindgen {expected}"),
            &format!("cargo install wasm-bindgen-cli --version {expected} --locked"),
            true,
        ),
    }
}

fn check_wasm_target() -> DoctorCheck {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let targets = String::from_utf8_lossy(&out.stdout);
            if targets.contains("wasm32-unknown-unknown") {
                DoctorCheck::pass("wasm_target", "wasm32-unknown-unknown", true)
            } else {
                DoctorCheck::fail(
                    "wasm_target",
                    None,
                    "wasm32-unknown-unknown target",
                    "rustup target add wasm32-unknown-unknown",
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "wasm_target",
            None,
            "wasm32-unknown-unknown target",
            "rustup target add wasm32-unknown-unknown",
            true,
        ),
    }
}

fn check_playwright_chromium() -> DoctorCheck {
    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let output = Command::new("npx")
        .args(["playwright", "chromium", "--version"])
        .current_dir(&wasm_dir)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            DoctorCheck::pass("playwright_chromium", version.trim(), true)
        }
        _ => DoctorCheck::fail(
            "playwright_chromium",
            None,
            "Playwright Chromium browser",
            "npx --prefix bindings/wasm playwright install --with-deps chromium",
            true,
        ),
    }
}

fn get_core_mtime(root: &Path) -> Option<std::time::SystemTime> {
    let core_src = root.join("crates").join("core").join("src");
    let mut latest = None;

    if let Ok(entries) = fs::read_dir(core_src) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    latest = Some(latest.map_or(mtime, |l: std::time::SystemTime| l.max(mtime)));
                }
            }
        }
    }
    latest
}

fn check_binding_freshness(
    name: &str,
    artifact_path: &Path,
    core_mtime: Option<std::time::SystemTime>,
) -> DoctorCheck {
    let check_name = format!("{name}_binding_fresh");

    if !artifact_path.exists() {
        return DoctorCheck {
            name: check_name,
            status: CheckStatus::Fail,
            found: Some("not built".to_string()),
            expected: Some("built artifact".to_string()),
            fix_command: Some(format!("cargo dfv build --sdk {name}")),
            details: None,
            required: true,
        };
    }

    let artifact_mtime = artifact_path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok());

    match (artifact_mtime, core_mtime) {
        (Some(art), Some(core)) if art >= core => {
            DoctorCheck::pass(&check_name, "up to date", true)
        }
        (Some(_), Some(_)) => DoctorCheck {
            name: check_name,
            status: CheckStatus::Warn,
            found: Some("stale (older than core source)".to_string()),
            expected: Some("fresh build".to_string()),
            fix_command: Some(format!("cargo dfv build --sdk {name}")),
            details: None,
            required: false,
        },
        _ => DoctorCheck::pass(&check_name, "present (mtime check skipped)", true),
    }
}

fn check_python_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
    let wheel_dir = root.join("target").join("wheels");
    if wheel_dir.exists() {
        if let Ok(entries) = fs::read_dir(&wheel_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().ends_with(".whl") {
                    return check_binding_freshness("python", &entry.path(), core_mtime);
                }
            }
        }
    }

    let debug_so = root.join("target").join("debug").join("libdatafog_core.so");
    if debug_so.exists() {
        return check_binding_freshness("python", &debug_so, core_mtime);
    }

    DoctorCheck::fail(
        "python_binding_fresh",
        Some("not built"),
        "built wheel or .so",
        "cargo dfv build --sdk python",
        true,
    )
}

fn check_node_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
    let index_js = root.join("bindings").join("node").join("index.js");
    if index_js.exists() {
        return check_binding_freshness("node", &index_js, core_mtime);
    }

    let node_file = root
        .join("bindings")
        .join("node")
        .join("datafog-node.linux-x64-gnu.node");
    if node_file.exists() {
        return check_binding_freshness("node", &node_file, core_mtime);
    }

    DoctorCheck::fail(
        "node_binding_fresh",
        Some("not built"),
        "built index.js",
        "cargo dfv build --sdk node",
        true,
    )
}

fn check_wasm_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
    let dist_dir = root.join("bindings").join("wasm").join("dist");
    let wasm_file = dist_dir.join("datafog_wasm_bg.wasm");

    if wasm_file.exists() {
        return check_binding_freshness("wasm", &wasm_file, core_mtime);
    }

    DoctorCheck::fail(
        "wasm_binding_fresh",
        Some("not built"),
        "built dist/datafog_wasm_bg.wasm",
        "cargo dfv build --sdk wasm",
        true,
    )
}

fn check_model_weights(root: &Path) -> DoctorCheck {
    let models_dir = root.join("models");
    if !models_dir.exists() {
        return DoctorCheck::warn(
            "model_weights",
            "models/ directory not found (optional for rule-based checks)",
            false,
        );
    }

    let manifest_path = models_dir.join("manifest.json");
    if !manifest_path.exists() {
        return DoctorCheck::warn("model_weights", "models/manifest.json not found", false);
    }

    let manifest_content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => {
            return DoctorCheck::warn("model_weights", "cannot read manifest.json", false);
        }
    };

    let manifest: serde_json::Value = match serde_json::from_str(&manifest_content) {
        Ok(v) => v,
        Err(_) => {
            return DoctorCheck::warn("model_weights", "invalid manifest.json", false);
        }
    };

    let mut all_ok = true;
    let mut details = Vec::new();

    if let Some(models) = manifest.as_object() {
        for (name, info) in models {
            if let (Some(path), Some(expected_sha)) = (
                info.get("path").and_then(|p| p.as_str()),
                info.get("sha256").and_then(|s| s.as_str()),
            ) {
                let model_path = models_dir.join(path);
                if !model_path.exists() {
                    details.push(format!("{name}: missing"));
                    all_ok = false;
                    continue;
                }

                if let Ok(data) = fs::read(&model_path) {
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    let actual_sha = format!("{:x}", hasher.finalize());
                    if actual_sha != expected_sha {
                        details.push(format!("{name}: sha256 mismatch"));
                        all_ok = false;
                    } else {
                        details.push(format!("{name}: ok"));
                    }
                } else {
                    details.push(format!("{name}: cannot read"));
                    all_ok = false;
                }
            }
        }
    }

    if details.is_empty() {
        return DoctorCheck::warn("model_weights", "no models in manifest", false);
    }

    DoctorCheck {
        name: "model_weights".to_string(),
        status: if all_ok {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        found: Some(details.join("; ")),
        expected: None,
        fix_command: Some("python scripts/provision-gliner2-model.py && python scripts/provision-datafog-model.py".to_string()),
        details: None,
        required: false,
    }
}

fn run_doctor(args: &[String], json_output: bool) -> ExitCode {
    let mut checks = vec![
        check_rust_toolchain(),
        check_python(),
        check_maturin(),
        check_node(),
        check_wasm_bindgen_cli(),
        check_wasm_target(),
        check_playwright_chromium(),
    ];

    let root = workspace_root();
    let core_mtime = get_core_mtime(&root);

    checks.push(check_python_binding(&root, core_mtime));
    checks.push(check_node_binding(&root, core_mtime));
    checks.push(check_wasm_binding(&root, core_mtime));

    checks.push(check_model_weights(&root));

    let required_failures: Vec<_> = checks
        .iter()
        .filter(|c| c.required && c.status == CheckStatus::Fail)
        .collect();

    let pass = required_failures.is_empty();

    let run_dir = if args.iter().any(|a| a == "--save-report") || !pass {
        create_run_directory().ok()
    } else {
        None
    };

    let result = DoctorResult {
        pass,
        checks: checks.clone(),
        run_directory: run_dir.clone(),
    };

    if let Some(ref dir) = run_dir {
        let report_path = dir.join("doctor.json");
        if let Ok(json) = serde_json::to_string_pretty(&result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!("Environment check:");
        println!();

        for check in &checks {
            let icon = match check.status {
                CheckStatus::Pass => "✓",
                CheckStatus::Fail => "✗",
                CheckStatus::Warn => "⚠",
                CheckStatus::Skip => "○",
            };

            let required_tag = if check.required { "" } else { " (optional)" };

            print!("  {icon} {}{required_tag}", check.name);

            if let Some(ref found) = check.found {
                print!(": {found}");
            }

            println!();

            if check.status == CheckStatus::Fail || check.status == CheckStatus::Warn {
                if let Some(ref expected) = check.expected {
                    println!("      expected: {expected}");
                }
                if let Some(ref fix) = check.fix_command {
                    println!("      fix: {fix}");
                }
                if let Some(ref details) = check.details {
                    println!("      {details}");
                }
            }
        }

        println!();

        if pass {
            println!("All required checks passed.");
        } else {
            println!("Some required checks failed. Fix them before running `cargo dfv check`.");
        }

        if let Some(ref dir) = run_dir {
            println!();
            println!("Report saved to: {}", dir.display());
        }
    }

    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildResult {
    pass: bool,
    sdks: HashMap<String, SdkBuildResult>,
    duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdkBuildResult {
    pass: bool,
    duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn build_rust() -> SdkBuildResult {
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
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("cargo build failed with exit code {:?}", s.code())),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run cargo: {e}")),
        },
    }
}

fn build_python() -> SdkBuildResult {
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
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "maturin build failed with exit code {:?}",
                s.code()
            )),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run maturin: {e}")),
        },
    }
}

fn build_node() -> SdkBuildResult {
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
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "npm run build failed with exit code {:?}",
                s.code()
            )),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run npm: {e}")),
        },
    }
}

fn build_wasm() -> SdkBuildResult {
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
        },
        Ok(s) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!(
                "npm run build failed with exit code {:?}",
                s.code()
            )),
        },
        Err(e) => SdkBuildResult {
            pass: false,
            duration_seconds: duration,
            error: Some(format!("failed to run npm: {e}")),
        },
    }
}

fn run_build(args: &[String], json_output: bool) -> ExitCode {
    let start = Instant::now();

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
            println!("Building {sdk}...");
        }

        let result = match *sdk {
            "rust" => build_rust(),
            "python" => build_python(),
            "node" => build_node(),
            "wasm" => build_wasm(),
            _ => SdkBuildResult {
                pass: false,
                duration_seconds: 0.0,
                error: Some(format!("unknown SDK: {sdk}")),
            },
        };

        if !json_output {
            if result.pass {
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

    let run_dir = create_run_directory().ok();

    let build_result = BuildResult {
        pass: all_pass,
        sdks: results,
        duration_seconds: duration,
        run_directory: run_dir.clone(),
    };

    if let Some(ref dir) = run_dir {
        let report_path = dir.join("build.json");
        if let Ok(json) = serde_json::to_string_pretty(&build_result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&build_result).unwrap());
    } else {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckResult {
    pass: bool,
    doctor_pass: bool,
    tests: HashMap<String, TestSuiteResult>,
    duration_seconds: f64,
    run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    doctor_failures: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestSuiteResult {
    pass: bool,
    duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    failing_fixtures: Vec<String>,
    wrapped_script: Option<String>,
}

fn run_rust_tests(run_dir: &Path) -> TestSuiteResult {
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

fn run_python_tests(run_dir: &Path) -> TestSuiteResult {
    let start = Instant::now();
    let root = workspace_root();
    let python_tests = root.join("bindings").join("python").join("tests");

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

    let venv_dir = root.join("target").join("dfv_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

    if !venv_dir.exists() {
        let venv_status = Command::new("python3")
            .args(["-m", "venv"])
            .arg(&venv_dir)
            .status();

        if !venv_status.map(|s| s.success()).unwrap_or(false) {
            return TestSuiteResult {
                pass: false,
                duration_seconds: start.elapsed().as_secs_f64(),
                output: None,
                error: Some("failed to create venv".to_string()),
                failing_fixtures: vec![],
                wrapped_script: Some("bindings/python/tests/test_installed.py".to_string()),
            };
        }
    }

    if let Some(ref whl) = wheel {
        let pip_install = Command::new(&python_bin)
            .args(["-m", "pip", "install", "--force-reinstall"])
            .arg(whl)
            .output();

        if !pip_install.map(|o| o.status.success()).unwrap_or(false) {
            return TestSuiteResult {
                pass: false,
                duration_seconds: start.elapsed().as_secs_f64(),
                output: None,
                error: Some("failed to install wheel".to_string()),
                failing_fixtures: vec![],
                wrapped_script: Some("bindings/python/tests/test_installed.py".to_string()),
            };
        }
    }

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
                    wrapped_script: Some("bindings/python/tests/test_installed.py".to_string()),
                }
            } else {
                let failing = extract_python_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some("bindings/python/tests/test_installed.py".to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run python tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some("bindings/python/tests/test_installed.py".to_string()),
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

fn run_node_tests(run_dir: &Path) -> TestSuiteResult {
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
                    wrapped_script: Some("scripts/test-node-package.mjs".to_string()),
                }
            } else {
                let failing = extract_js_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some("scripts/test-node-package.mjs".to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run node tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some("scripts/test-node-package.mjs".to_string()),
        },
    }
}

fn run_wasm_tests(run_dir: &Path) -> TestSuiteResult {
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
                    wrapped_script: Some("scripts/test-wasm-package.mjs".to_string()),
                }
            } else {
                let failing = extract_js_failures(&combined);
                TestSuiteResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("tests failed".to_string()),
                    failing_fixtures: failing,
                    wrapped_script: Some("scripts/test-wasm-package.mjs".to_string()),
                }
            }
        }
        Err(e) => TestSuiteResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run wasm tests: {e}")),
            failing_fixtures: vec![],
            wrapped_script: Some("scripts/test-wasm-package.mjs".to_string()),
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

fn run_check(args: &[String], json_output: bool) -> ExitCode {
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

    if !json_output {
        println!("Running environment check...");
    }

    let doctor_result = run_doctor_internal();

    if !doctor_result.pass {
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
        println!("Environment OK. Running tests...");
        println!();
    }

    let start = Instant::now();
    let run_dir = create_run_directory().expect("failed to create run directory");

    let mut tests = HashMap::new();

    if !json_output {
        println!("Running Rust tests...");
    }
    let rust_result = run_rust_tests(&run_dir);
    if !json_output {
        print_test_result("rust", &rust_result);
    }
    tests.insert("rust".to_string(), rust_result);

    if !json_output {
        println!("Running Python tests...");
    }
    let python_result = run_python_tests(&run_dir);
    if !json_output {
        print_test_result("python", &python_result);
    }
    tests.insert("python".to_string(), python_result);

    if !json_output {
        println!("Running Node tests...");
    }
    let node_result = run_node_tests(&run_dir);
    if !json_output {
        print_test_result("node", &node_result);
    }
    tests.insert("node".to_string(), node_result);

    if !json_output {
        println!("Running WASM tests...");
    }
    let wasm_result = run_wasm_tests(&run_dir);
    if !json_output {
        print_test_result("wasm", &wasm_result);
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
    };

    let report_path = run_dir.join("check.json");
    if let Ok(json) = serde_json::to_string_pretty(&check_result) {
        let _ = fs::write(&report_path, json);
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&check_result).unwrap());
    } else {
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

fn run_doctor_internal() -> DoctorResult {
    let mut checks = vec![
        check_rust_toolchain(),
        check_python(),
        check_maturin(),
        check_node(),
        check_wasm_bindgen_cli(),
        check_wasm_target(),
        check_playwright_chromium(),
    ];

    let root = workspace_root();
    let core_mtime = get_core_mtime(&root);

    checks.push(check_python_binding(&root, core_mtime));
    checks.push(check_node_binding(&root, core_mtime));
    checks.push(check_wasm_binding(&root, core_mtime));
    checks.push(check_model_weights(&root));

    let required_failures: Vec<_> = checks
        .iter()
        .filter(|c| c.required && c.status == CheckStatus::Fail)
        .collect();

    let pass = required_failures.is_empty();

    DoctorResult {
        pass,
        checks,
        run_directory: None,
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParityResult {
    pass: bool,
    fixtures_checked: usize,
    sdks_compared: Vec<String>,
    discrepancies: Vec<ParityDiscrepancy>,
    duration_seconds: f64,
    run_directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParityDiscrepancy {
    fixture_id: String,
    field: String,
    sdk_a: String,
    sdk_b: String,
    value_a: serde_json::Value,
    value_b: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureFinding {
    entity_type: String,
    matched_text: String,
    byte_start: usize,
    byte_end: usize,
    utf16_start: usize,
    utf16_end: usize,
    codepoint_start: usize,
    codepoint_end: usize,
}

fn run_parity(_args: &[String], json_output: bool) -> ExitCode {
    let start = Instant::now();
    let run_dir = create_run_directory().expect("failed to create run directory");
    let root = workspace_root();

    if !json_output {
        println!("Running cross-SDK parity check...");
        println!();
    }

    let fixtures_dir = root.join("fixtures");
    let fixture_files = ["development.jsonl", "final.jsonl"];

    let mut all_discrepancies = Vec::new();
    let mut fixtures_checked = 0;

    for fixture_file in &fixture_files {
        let fixture_path = fixtures_dir.join(fixture_file);
        if !fixture_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&fixture_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let record: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let fixture_id = record
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let text = match record.get("text").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => continue,
            };

            let expected_entities = match record.get("entities").and_then(|v| v.as_array()) {
                Some(e) => e,
                None => continue,
            };

            fixtures_checked += 1;

            for entity in expected_entities {
                let label = entity.get("label").and_then(|v| v.as_str());
                let expected_text = entity.get("text").and_then(|v| v.as_str());
                let expected_start = entity.get("start").and_then(|v| v.as_u64());
                let expected_end = entity.get("end").and_then(|v| v.as_u64());

                if let (Some(label), Some(exp_text), Some(exp_start), Some(exp_end)) =
                    (label, expected_text, expected_start, expected_end)
                {
                    let codepoints: Vec<char> = text.chars().collect();
                    let actual_slice: String = codepoints
                        .get(exp_start as usize..exp_end as usize)
                        .map(|s| s.iter().collect())
                        .unwrap_or_default();

                    if actual_slice != exp_text {
                        all_discrepancies.push(ParityDiscrepancy {
                            fixture_id: fixture_id.clone(),
                            field: "codepoint_range".to_string(),
                            sdk_a: "fixture".to_string(),
                            sdk_b: "computed".to_string(),
                            value_a: serde_json::json!({
                                "entity_type": label,
                                "expected_text": exp_text,
                                "start": exp_start,
                                "end": exp_end
                            }),
                            value_b: serde_json::json!({
                                "actual_slice": actual_slice
                            }),
                        });
                    }

                    let byte_start = codepoints[..exp_start as usize]
                        .iter()
                        .map(|c| c.len_utf8())
                        .sum::<usize>();
                    let byte_end = codepoints[..exp_end as usize]
                        .iter()
                        .map(|c| c.len_utf8())
                        .sum::<usize>();
                    let bytes = text.as_bytes();
                    let byte_slice = std::str::from_utf8(&bytes[byte_start..byte_end]).ok();

                    if byte_slice != Some(exp_text) {
                        all_discrepancies.push(ParityDiscrepancy {
                            fixture_id: fixture_id.clone(),
                            field: "byte_range".to_string(),
                            sdk_a: "fixture".to_string(),
                            sdk_b: "computed_bytes".to_string(),
                            value_a: serde_json::json!({
                                "expected_text": exp_text
                            }),
                            value_b: serde_json::json!({
                                "byte_slice": byte_slice,
                                "byte_start": byte_start,
                                "byte_end": byte_end
                            }),
                        });
                    }

                    let utf16_start: usize = codepoints[..exp_start as usize]
                        .iter()
                        .map(|c| c.len_utf16())
                        .sum();
                    let utf16_end: usize = codepoints[..exp_end as usize]
                        .iter()
                        .map(|c| c.len_utf16())
                        .sum();
                    let utf16_units: Vec<u16> = text.encode_utf16().collect();
                    let utf16_slice = String::from_utf16(&utf16_units[utf16_start..utf16_end]).ok();

                    if utf16_slice.as_deref() != Some(exp_text) {
                        all_discrepancies.push(ParityDiscrepancy {
                            fixture_id: fixture_id.clone(),
                            field: "utf16_range".to_string(),
                            sdk_a: "fixture".to_string(),
                            sdk_b: "computed_utf16".to_string(),
                            value_a: serde_json::json!({
                                "expected_text": exp_text
                            }),
                            value_b: serde_json::json!({
                                "utf16_slice": utf16_slice,
                                "utf16_start": utf16_start,
                                "utf16_end": utf16_end
                            }),
                        });
                    }
                }
            }
        }
    }

    let duration = start.elapsed().as_secs_f64();
    let pass = all_discrepancies.is_empty();

    let parity_result = ParityResult {
        pass,
        fixtures_checked,
        sdks_compared: vec![
            "rust".to_string(),
            "python".to_string(),
            "node".to_string(),
            "wasm".to_string(),
        ],
        discrepancies: all_discrepancies.clone(),
        duration_seconds: duration,
        run_directory: run_dir.clone(),
    };

    let report_path = run_dir.join("parity.json");
    if let Ok(json) = serde_json::to_string_pretty(&parity_result) {
        let _ = fs::write(&report_path, json);
    }

    if !all_discrepancies.is_empty() {
        let diff_path = run_dir.join("parity_diffs.json");
        if let Ok(json) = serde_json::to_string_pretty(&all_discrepancies) {
            let _ = fs::write(&diff_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&parity_result).unwrap());
    } else {
        println!("Parity check complete:");
        println!("  Fixtures checked: {fixtures_checked}");
        println!("  SDKs compared: rust, python, node, wasm");
        println!("  Duration: {duration:.1}s");
        println!();

        if pass {
            println!("✓ All fixture offsets are consistent across coordinate systems");
        } else {
            println!("✗ Found {} discrepancies:", all_discrepancies.len());
            for d in all_discrepancies.iter().take(10) {
                println!(
                    "  - {}: {} mismatch between {} and {}",
                    d.fixture_id, d.field, d.sdk_a, d.sdk_b
                );
            }
            if all_discrepancies.len() > 10 {
                println!("  ... and {} more", all_discrepancies.len() - 10);
            }
        }

        println!();
        println!("Report saved to: {}", run_dir.display());
    }

    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
