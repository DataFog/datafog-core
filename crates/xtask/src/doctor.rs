use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub pass: bool,
    pub checks: Vec<DoctorCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_directory: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Warn,
    Skip,
}

impl DoctorCheck {
    pub fn pass(name: &str, found: &str, required: bool) -> Self {
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

    pub fn fail(
        name: &str,
        found: Option<&str>,
        expected: &str,
        fix: &str,
        required: bool,
    ) -> Self {
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

    pub fn warn(name: &str, details: &str, required: bool) -> Self {
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

pub fn check_rust_toolchain() -> DoctorCheck {
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

pub fn check_python() -> DoctorCheck {
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

pub fn check_maturin() -> DoctorCheck {
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

pub fn check_node() -> DoctorCheck {
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

pub fn check_wasm_bindgen_cli() -> DoctorCheck {
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

pub fn check_wasm_target() -> DoctorCheck {
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

pub fn check_playwright_chromium() -> DoctorCheck {
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

pub fn check_python_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
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

pub fn check_node_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
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

pub fn check_wasm_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
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

pub fn check_model_weights(root: &Path) -> DoctorCheck {
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
        fix_command: Some(
            "python scripts/provision-gliner2-model.py && python scripts/provision-datafog-model.py"
                .to_string(),
        ),
        details: None,
        required: false,
    }
}

pub fn run_doctor_internal() -> DoctorResult {
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

pub fn run_doctor(args: &[String], json_output: bool) -> ExitCode {
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
