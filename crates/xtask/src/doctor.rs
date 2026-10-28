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

#[derive(Debug, Clone)]
pub struct VersionPins {
    pub rust_channel: String,
    pub node_major: u32,
    pub wasm_bindgen: String,
    pub playwright: String,
}

fn read_rust_channel(root: &Path) -> Result<String, String> {
    let path = root.join("rust-toolchain.toml");
    if !path.exists() {
        return Err("rust-toolchain.toml not found".to_string());
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read rust-toolchain.toml: {e}"))?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("channel") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line.rfind('"') {
                    if end > start {
                        return Ok(line[start + 1..end].to_string());
                    }
                }
            }
        }
    }
    Err("rust-toolchain.toml missing channel".to_string())
}

fn read_node_major(root: &Path) -> Result<u32, String> {
    let path = root.join(".nvmrc");
    if !path.exists() {
        return Err(".nvmrc not found".to_string());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("cannot read .nvmrc: {e}"))?;
    content
        .trim()
        .parse()
        .map_err(|_| ".nvmrc does not contain a valid major version number".to_string())
}

fn read_wasm_bindgen_from_lockfile(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.lock");
    if !path.exists() {
        return Err("Cargo.lock not found".to_string());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("cannot read Cargo.lock: {e}"))?;

    let mut in_wasm_bindgen = false;
    for line in content.lines() {
        if line == r#"name = "wasm-bindgen""# {
            in_wasm_bindgen = true;
            continue;
        }
        if in_wasm_bindgen && line.starts_with("version = ") {
            let version = line
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
                .map(String::from);
            if let Some(v) = version {
                return Ok(v);
            }
        }
        if in_wasm_bindgen && line.starts_with('[') {
            break;
        }
    }
    Err("wasm-bindgen version not found in Cargo.lock".to_string())
}

fn read_playwright_version(root: &Path) -> Result<String, String> {
    let path = root.join("bindings/wasm/package.json");
    if !path.exists() {
        return Err("bindings/wasm/package.json not found".to_string());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("cannot read bindings/wasm/package.json: {e}"))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid package.json: {e}"))?;
    parsed
        .get("devDependencies")
        .and_then(|d| d.get("playwright"))
        .and_then(|p| p.as_str())
        .map(String::from)
        .ok_or_else(|| "devDependencies.playwright not found in package.json".to_string())
}

pub fn read_version_pins(root: &Path) -> Result<VersionPins, Vec<String>> {
    let mut errors = Vec::new();

    let rust_channel = read_rust_channel(root).unwrap_or_else(|e| {
        errors.push(e);
        String::new()
    });
    let node_major = read_node_major(root).unwrap_or_else(|e| {
        errors.push(e);
        0
    });
    let wasm_bindgen = read_wasm_bindgen_from_lockfile(root).unwrap_or_else(|e| {
        errors.push(e);
        String::new()
    });
    let playwright = read_playwright_version(root).unwrap_or_else(|e| {
        errors.push(e);
        String::new()
    });

    if errors.is_empty() {
        Ok(VersionPins {
            rust_channel,
            node_major,
            wasm_bindgen,
            playwright,
        })
    } else {
        Err(errors)
    }
}

pub fn check_rust_toolchain(pins: &VersionPins) -> DoctorCheck {
    let expected = &pins.rust_channel;
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
                    "rustup show",
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "rust_toolchain",
            None,
            &format!("rustc {expected}"),
            &format!(
                "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh && rustup install {expected}"
            ),
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
            "pipx install maturin  # or: python3 -m pip install maturin",
            true,
        ),
    }
}

pub fn check_node(pins: &VersionPins) -> DoctorCheck {
    let expected_major = pins.node_major;
    let output = Command::new("node").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();

            if node_satisfies(version, expected_major) {
                DoctorCheck::pass("node", version, true)
            } else {
                DoctorCheck::fail(
                    "node",
                    Some(version),
                    &format!("Node.js {expected_major}+"),
                    &format!(
                        "Install Node >= {expected_major} (nvm install / fnm install / mise install)"
                    ),
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "node",
            None,
            &format!("Node.js {expected_major}+"),
            &format!("Install Node >= {expected_major} (nvm install / fnm install / mise install)"),
            true,
        ),
    }
}

fn extract_wasm_bindgen_version(output: &str) -> Option<&str> {
    output
        .trim()
        .strip_prefix("wasm-bindgen ")
        .map(|s| s.split_whitespace().next().unwrap_or(s))
}

/// Parse Node version string (e.g. "v25.1.0") and check if major >= pin.
pub fn node_satisfies(found: &str, pin: u32) -> bool {
    found
        .strip_prefix('v')
        .and_then(|v| v.split('.').next())
        .and_then(|m| m.parse::<u32>().ok())
        .is_some_and(|m| m >= pin)
}

/// Extract Playwright version from CLI output (e.g. "Version 1.64.1" -> "1.64.1").
pub fn extract_playwright_version(output: &str) -> &str {
    output
        .trim()
        .strip_prefix("Version ")
        .unwrap_or(output.trim())
}

/// Check if found Playwright version exactly matches pin.
pub fn playwright_matches(found: &str, pin: &str) -> bool {
    extract_playwright_version(found) == pin
}

pub fn check_wasm_bindgen_cli(pins: &VersionPins) -> DoctorCheck {
    let expected = &pins.wasm_bindgen;
    let output = Command::new("wasm-bindgen").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            let found_version = extract_wasm_bindgen_version(&raw);
            let version = raw.trim();

            if found_version == Some(expected.as_str()) {
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

pub fn check_playwright_chromium(pins: &VersionPins) -> DoctorCheck {
    let expected = &pins.playwright;
    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let output = Command::new("npx")
        .args(["playwright", "--version"])
        .current_dir(&wasm_dir)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            let found_version = extract_playwright_version(&raw);

            if playwright_matches(&raw, expected) {
                let chromium_output = Command::new("npx")
                    .args(["playwright", "chromium", "--version"])
                    .current_dir(&wasm_dir)
                    .output();
                match chromium_output {
                    Ok(co) if co.status.success() => {
                        let chromium_ver = String::from_utf8_lossy(&co.stdout);
                        DoctorCheck::pass(
                            "playwright_chromium",
                            &format!("Playwright {} (Chromium {})", expected, chromium_ver.trim()),
                            true,
                        )
                    }
                    _ => DoctorCheck::fail(
                        "playwright_chromium",
                        Some(&format!("Playwright {expected} (Chromium not installed)")),
                        &format!("Playwright {expected} with Chromium"),
                        &format!("npx playwright@{expected} install chromium"),
                        true,
                    ),
                }
            } else {
                DoctorCheck::fail(
                    "playwright_chromium",
                    Some(&format!("Playwright {found_version}")),
                    &format!("Playwright {expected}"),
                    &format!("npx playwright@{expected} install chromium"),
                    true,
                )
            }
        }
        _ => DoctorCheck::fail(
            "playwright_chromium",
            None,
            &format!("Playwright {expected} with Chromium"),
            &format!("npx playwright@{expected} install chromium"),
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
    let node_dir = root.join("bindings").join("node");
    // Prefer NAPI build artifacts over the hand-written index.js wrapper,
    // which is committed source and is not rewritten by `napi build`.
    let native_js = node_dir.join("native.js");
    if native_js.exists() {
        return check_binding_freshness("node", &native_js, core_mtime);
    }

    if let Ok(entries) = fs::read_dir(&node_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".node") {
                return check_binding_freshness("node", &entry.path(), core_mtime);
            }
        }
    }

    DoctorCheck::fail(
        "node_binding_fresh",
        Some("not built"),
        "built native.js or .node",
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

fn run_checks(pins: &VersionPins, root: &Path) -> Vec<DoctorCheck> {
    let mut checks = vec![
        check_rust_toolchain(pins),
        check_python(),
        check_maturin(),
        check_node(pins),
        check_wasm_bindgen_cli(pins),
        check_wasm_target(),
        check_playwright_chromium(pins),
    ];

    let core_mtime = get_core_mtime(root);

    checks.push(check_python_binding(root, core_mtime));
    checks.push(check_node_binding(root, core_mtime));
    checks.push(check_wasm_binding(root, core_mtime));
    checks.push(check_model_weights(root));

    checks
}

fn make_pin_error_checks(errors: &[String]) -> Vec<DoctorCheck> {
    errors
        .iter()
        .map(|e| DoctorCheck::fail("pin_file", None, "pin file present", e, true))
        .collect()
}

pub fn run_doctor_internal() -> DoctorResult {
    let root = workspace_root();

    let pins = match read_version_pins(&root) {
        Ok(p) => p,
        Err(errors) => {
            return DoctorResult {
                pass: false,
                checks: make_pin_error_checks(&errors),
                run_directory: None,
            };
        }
    };

    let checks = run_checks(&pins, &root);
    let pass = checks
        .iter()
        .all(|c| !c.required || c.status != CheckStatus::Fail);

    DoctorResult {
        pass,
        checks,
        run_directory: None,
    }
}

pub fn run_doctor(args: &[String], json_output: bool) -> ExitCode {
    let root = workspace_root();

    let pins = match read_version_pins(&root) {
        Ok(p) => p,
        Err(errors) => {
            if json_output {
                let result = DoctorResult {
                    pass: false,
                    checks: make_pin_error_checks(&errors),
                    run_directory: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("Missing pin files:");
                for e in &errors {
                    eprintln!("  {e}");
                }
                eprintln!();
                eprintln!("Pin files must exist. See CONTRIBUTING.md for setup.");
            }
            return ExitCode::FAILURE;
        }
    };

    let checks = run_checks(&pins, &root);
    let pass = checks
        .iter()
        .all(|c| !c.required || c.status != CheckStatus::Fail);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_dir() -> std::path::PathBuf {
        let unique = format!(
            "dfv-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn node_satisfies_major_version_comparison() {
        assert!(node_satisfies("v25.1.0", 24), "Node 25 passes pin 24");
        assert!(node_satisfies("v24.0.0", 24), "Node 24 passes pin 24");
        assert!(node_satisfies("v24.22.1", 24), "Node 24.22.1 passes pin 24");
        assert!(!node_satisfies("v22.23.4", 24), "Node 22 fails pin 24");
        assert!(!node_satisfies("v23.0.0", 24), "Node 23 fails pin 24");
        assert!(!node_satisfies("invalid", 24), "Invalid version fails");
        assert!(!node_satisfies("", 24), "Empty version fails");
    }

    #[test]
    fn missing_nvmrc_errors() {
        let tmp = make_temp_dir();
        let result = read_node_major(&tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".nvmrc not found"));
        cleanup(&tmp);
    }

    #[test]
    fn missing_rust_toolchain_errors() {
        let tmp = make_temp_dir();
        let result = read_rust_channel(&tmp);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("rust-toolchain.toml not found")
        );
        cleanup(&tmp);
    }

    #[test]
    fn reads_rust_channel_from_toml() {
        let tmp = make_temp_dir();
        fs::write(
            tmp.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.88.0\"\n",
        )
        .unwrap();
        let channel = read_rust_channel(&tmp).unwrap();
        assert_eq!(channel, "1.88.0");
        cleanup(&tmp);
    }

    #[test]
    fn reads_wasm_bindgen_from_lockfile() {
        let tmp = make_temp_dir();
        fs::write(
            tmp.join("Cargo.lock"),
            r#"[[package]]
name = "wasm-bindgen"
version = "0.2.127"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#,
        )
        .unwrap();
        let version = read_wasm_bindgen_from_lockfile(&tmp).unwrap();
        assert_eq!(version, "0.2.127");
        cleanup(&tmp);
    }

    #[test]
    fn reads_playwright_from_package_json() {
        let tmp = make_temp_dir();
        let wasm_dir = tmp.join("bindings/wasm");
        fs::create_dir_all(&wasm_dir).unwrap();
        fs::write(
            wasm_dir.join("package.json"),
            r#"{"devDependencies": {"playwright": "1.62.1"}}"#,
        )
        .unwrap();
        let version = read_playwright_version(&tmp).unwrap();
        assert_eq!(version, "1.62.1");
        cleanup(&tmp);
    }

    #[test]
    fn wasm_bindgen_version_extraction() {
        assert_eq!(
            extract_wasm_bindgen_version("wasm-bindgen 0.2.127"),
            Some("0.2.127")
        );
        assert_eq!(
            extract_wasm_bindgen_version("wasm-bindgen 0.2.100 (abc123)"),
            Some("0.2.100")
        );
    }

    #[test]
    fn playwright_version_matching() {
        assert!(
            playwright_matches("Version 1.62.1", "1.62.1"),
            "Exact match passes"
        );
        assert!(
            playwright_matches("1.62.1", "1.62.1"),
            "Without prefix passes"
        );
        assert!(
            !playwright_matches("Version 1.64.1", "1.62.1"),
            "Mismatch fails"
        );
        assert!(
            !playwright_matches("Version 1.62.0", "1.62.1"),
            "Patch mismatch fails"
        );
    }

    #[test]
    fn playwright_version_extraction() {
        assert_eq!(extract_playwright_version("Version 1.62.1"), "1.62.1");
        assert_eq!(extract_playwright_version("Version 1.64.1\n"), "1.64.1");
        assert_eq!(extract_playwright_version("1.62.1"), "1.62.1");
    }
}
