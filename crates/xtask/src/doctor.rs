use crate::report::{create_run_directory, workspace_root};
use crate::toolchain::{
    ToolchainConfig, VersionMatch, compare_exact_version, compare_node_version,
    compare_rust_version,
};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Vec<String>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_fixable: Option<bool>,
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
            auto_fixable: None,
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
            auto_fixable: Some(true),
        }
    }

    pub fn fail_manual(
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
            auto_fixable: Some(false),
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
            auto_fixable: None,
        }
    }
}

fn get_toolchain_config() -> Option<ToolchainConfig> {
    ToolchainConfig::from_workspace()
}

pub fn check_rust_toolchain(config: Option<&ToolchainConfig>) -> DoctorCheck {
    let expected = config.map(|c| c.rust_version.as_str()).unwrap_or("1.88.0");
    let output = Command::new("rustc").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version_str = String::from_utf8_lossy(&out.stdout);
            let version_str = version_str.trim();
            let found_version = version_str.split_whitespace().nth(1).unwrap_or(version_str);

            match compare_rust_version(found_version, expected) {
                VersionMatch::Exact => DoctorCheck::pass("rust_toolchain", version_str, true),
                _ => DoctorCheck::fail(
                    "rust_toolchain",
                    Some(version_str),
                    &format!("rustc {expected}"),
                    &format!("rustup install {expected} && rustup default {expected}"),
                    true,
                ),
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
                DoctorCheck::fail_manual(
                    "python",
                    Some(version),
                    "Python 3.10+",
                    "Install Python 3.10 or newer from https://python.org or your package manager",
                    true,
                )
            }
        }
        _ => DoctorCheck::fail_manual(
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

pub fn check_node(config: Option<&ToolchainConfig>) -> DoctorCheck {
    let expected = config.map(|c| c.node_version.as_str()).unwrap_or("24");
    let expected_major: u32 = expected.parse().unwrap_or(24);
    let output = Command::new("node").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();

            match compare_node_version(version, expected) {
                VersionMatch::Exact => DoctorCheck::pass("node", version, true),
                _ => {
                    let fix_cmd = detect_node_fix_command(expected_major);
                    let auto_fixable = fix_cmd.starts_with("nvm ")
                        || fix_cmd.starts_with("fnm ")
                        || fix_cmd.contains("nodesource");

                    DoctorCheck {
                        name: "node".to_string(),
                        status: CheckStatus::Fail,
                        found: Some(version.to_string()),
                        expected: Some(format!("Node.js {expected}")),
                        fix_command: Some(fix_cmd),
                        details: None,
                        required: true,
                        auto_fixable: Some(auto_fixable),
                    }
                }
            }
        }
        _ => {
            let fix_cmd = detect_node_fix_command(expected_major);
            let auto_fixable = fix_cmd.starts_with("nvm ")
                || fix_cmd.starts_with("fnm ")
                || fix_cmd.contains("nodesource");

            DoctorCheck {
                name: "node".to_string(),
                status: CheckStatus::Fail,
                found: None,
                expected: Some(format!("Node.js {expected}")),
                fix_command: Some(fix_cmd),
                details: None,
                required: true,
                auto_fixable: Some(auto_fixable),
            }
        }
    }
}

fn detect_node_fix_command(version: u32) -> String {
    if Command::new("nvm").arg("--version").output().is_ok() {
        return format!("nvm install {version} && nvm use {version}");
    }

    if Command::new("fnm").arg("--version").output().is_ok() {
        return format!("fnm install {version} && fnm use {version}");
    }

    if cfg!(target_os = "linux") {
        return format!(
            "curl -fsSL https://deb.nodesource.com/setup_{version}.x | sudo -E bash - && sudo apt-get install -y nodejs"
        );
    }

    if cfg!(target_os = "macos") {
        return format!("brew install node@{version}");
    }

    format!("Install Node.js {version} from https://nodejs.org")
}

pub fn check_wasm_bindgen_cli(config: Option<&ToolchainConfig>) -> DoctorCheck {
    let expected = config
        .map(|c| c.wasm_bindgen_version.as_str())
        .unwrap_or("0.2.127");
    let output = Command::new("wasm-bindgen").arg("--version").output();

    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let version = version.trim();
            match compare_exact_version(version, expected) {
                VersionMatch::Exact => DoctorCheck::pass("wasm_bindgen_cli", version, true),
                _ => DoctorCheck::fail(
                    "wasm_bindgen_cli",
                    Some(version),
                    &format!("wasm-bindgen {expected}"),
                    &format!("cargo install wasm-bindgen-cli --version {expected} --locked"),
                    true,
                ),
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

pub fn check_playwright_chromium(config: Option<&ToolchainConfig>) -> DoctorCheck {
    let expected_version = config
        .map(|c| c.playwright_version.as_str())
        .unwrap_or("1.62.1");
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
            &format!("Playwright Chromium (version {expected_version})"),
            &format!("npx playwright@{expected_version} install chromium"),
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
            auto_fixable: Some(true),
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
            auto_fixable: Some(true),
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

    DoctorCheck {
        name: "python_binding_fresh".to_string(),
        status: CheckStatus::Fail,
        found: Some("not built".to_string()),
        expected: Some("built wheel or .so".to_string()),
        fix_command: Some("cargo dfv build --sdk python".to_string()),
        details: None,
        required: true,
        auto_fixable: Some(true),
    }
}

pub fn check_node_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
    let node_dir = root.join("bindings").join("node");
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

    DoctorCheck {
        name: "node_binding_fresh".to_string(),
        status: CheckStatus::Fail,
        found: Some("not built".to_string()),
        expected: Some("built native.js or .node".to_string()),
        fix_command: Some("cargo dfv build --sdk node".to_string()),
        details: None,
        required: true,
        auto_fixable: Some(true),
    }
}

pub fn check_wasm_binding(root: &Path, core_mtime: Option<std::time::SystemTime>) -> DoctorCheck {
    let dist_dir = root.join("bindings").join("wasm").join("dist");
    let wasm_file = dist_dir.join("datafog_wasm_bg.wasm");

    if wasm_file.exists() {
        return check_binding_freshness("wasm", &wasm_file, core_mtime);
    }

    DoctorCheck {
        name: "wasm_binding_fresh".to_string(),
        status: CheckStatus::Fail,
        found: Some("not built".to_string()),
        expected: Some("built dist/datafog_wasm_bg.wasm".to_string()),
        fix_command: Some("cargo dfv build --sdk wasm".to_string()),
        details: None,
        required: true,
        auto_fixable: Some(true),
    }
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
        auto_fixable: None,
    }
}

fn run_toolchain_checks(config: Option<&ToolchainConfig>) -> Vec<DoctorCheck> {
    vec![
        check_rust_toolchain(config),
        check_python(),
        check_maturin(),
        check_node(config),
        check_wasm_bindgen_cli(config),
        check_wasm_target(),
        check_playwright_chromium(config),
    ]
}

pub fn run_doctor_internal() -> DoctorResult {
    let config = get_toolchain_config();
    let mut checks = run_toolchain_checks(config.as_ref());

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
        fixed: None,
    }
}

pub fn run_toolchain_check_internal() -> DoctorResult {
    let config = get_toolchain_config();
    let checks = run_toolchain_checks(config.as_ref());

    let required_failures: Vec<_> = checks
        .iter()
        .filter(|c| c.required && c.status == CheckStatus::Fail)
        .collect();

    let pass = required_failures.is_empty();

    DoctorResult {
        pass,
        checks,
        run_directory: None,
        fixed: None,
    }
}

fn fix_check(check: &DoctorCheck) -> Result<String, String> {
    let fix_cmd = check.fix_command.as_ref().ok_or("no fix command")?;
    let name = &check.name;

    match name.as_str() {
        "rust_toolchain" => fix_rust_toolchain(fix_cmd),
        "node" => fix_node(fix_cmd),
        "wasm_bindgen_cli" => fix_wasm_bindgen(fix_cmd),
        "wasm_target" => fix_wasm_target(),
        "maturin" => fix_maturin(),
        "playwright_chromium" => fix_playwright(fix_cmd),
        _ => Err(format!("no auto-fix for {name}")),
    }
}

fn fix_rust_toolchain(fix_cmd: &str) -> Result<String, String> {
    if fix_cmd.contains("rustup install") {
        let version = fix_cmd
            .split("rustup install ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .ok_or("cannot parse Rust version")?;

        let install = Command::new("rustup")
            .args(["install", version])
            .status()
            .map_err(|e| e.to_string())?;

        if !install.success() {
            return Err("rustup install failed".to_string());
        }

        let default = Command::new("rustup")
            .args(["default", version])
            .status()
            .map_err(|e| e.to_string())?;

        if !default.success() {
            return Err("rustup default failed".to_string());
        }

        Ok(format!("installed Rust {version}"))
    } else {
        Err("cannot auto-install Rust; run the suggested command manually".to_string())
    }
}

fn fix_node(fix_cmd: &str) -> Result<String, String> {
    if fix_cmd.starts_with("nvm ") {
        let version = fix_cmd
            .split("nvm install ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .ok_or("cannot parse Node version")?;

        let script = format!(
            r#"source ~/.nvm/nvm.sh && nvm install {version} && nvm alias default {version}"#
        );
        let status = Command::new("bash")
            .args(["-c", &script])
            .status()
            .map_err(|e| e.to_string())?;

        if status.success() {
            Ok(format!("installed Node {version} via nvm"))
        } else {
            Err("nvm install failed".to_string())
        }
    } else if fix_cmd.starts_with("fnm ") {
        let version = fix_cmd
            .split("fnm install ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .ok_or("cannot parse Node version")?;

        let install = Command::new("fnm")
            .args(["install", version])
            .status()
            .map_err(|e| e.to_string())?;

        if !install.success() {
            return Err("fnm install failed".to_string());
        }

        let default_cmd = Command::new("fnm")
            .args(["default", version])
            .status()
            .map_err(|e| e.to_string())?;

        if default_cmd.success() {
            Ok(format!("installed Node {version} via fnm"))
        } else {
            Err("fnm default failed".to_string())
        }
    } else if fix_cmd.contains("nodesource") {
        let version = fix_cmd
            .split("setup_")
            .nth(1)
            .and_then(|s| s.split(".x").next())
            .ok_or("cannot parse Node version from nodesource URL")?;

        let curl_cmd =
            format!("curl -fsSL https://deb.nodesource.com/setup_{version}.x | sudo -E bash -");
        let curl = Command::new("bash")
            .args(["-c", &curl_cmd])
            .status()
            .map_err(|e| e.to_string())?;

        if !curl.success() {
            return Err("nodesource setup failed".to_string());
        }

        let apt = Command::new("sudo")
            .args(["apt-get", "install", "-y", "nodejs"])
            .status()
            .map_err(|e| e.to_string())?;

        if apt.success() {
            Ok(format!("installed Node {version} via nodesource"))
        } else {
            Err("apt-get install nodejs failed".to_string())
        }
    } else {
        Err(format!("cannot auto-install Node; run manually: {fix_cmd}"))
    }
}

fn fix_wasm_bindgen(fix_cmd: &str) -> Result<String, String> {
    let version = fix_cmd
        .split("--version ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .ok_or("cannot parse wasm-bindgen version")?;

    let status = Command::new("cargo")
        .args([
            "install",
            "wasm-bindgen-cli",
            "--version",
            version,
            "--locked",
        ])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(format!("installed wasm-bindgen-cli {version}"))
    } else {
        Err("cargo install wasm-bindgen-cli failed".to_string())
    }
}

fn fix_wasm_target() -> Result<String, String> {
    let status = Command::new("rustup")
        .args(["target", "add", "wasm32-unknown-unknown"])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok("added wasm32-unknown-unknown target".to_string())
    } else {
        Err("rustup target add failed".to_string())
    }
}

fn fix_maturin() -> Result<String, String> {
    let status = Command::new("python3")
        .args(["-m", "pip", "install", "maturin"])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok("installed maturin".to_string())
    } else {
        Err("pip install maturin failed".to_string())
    }
}

fn fix_playwright(fix_cmd: &str) -> Result<String, String> {
    let version = fix_cmd
        .split("playwright@")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or("1.62.1");

    let root = workspace_root();
    let wasm_dir = root.join("bindings").join("wasm");

    let status = Command::new("npx")
        .args([&format!("playwright@{version}"), "install", "chromium"])
        .current_dir(&wasm_dir)
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(format!("installed Playwright Chromium {version}"))
    } else {
        Err("npx playwright install failed".to_string())
    }
}

fn run_fix(checks: &[DoctorCheck]) -> Vec<String> {
    let mut fixed = Vec::new();

    for check in checks {
        if check.status != CheckStatus::Fail {
            continue;
        }
        if check.auto_fixable != Some(true) {
            continue;
        }

        match fix_check(check) {
            Ok(msg) => {
                println!("  ✓ Fixed {}: {msg}", check.name);
                fixed.push(check.name.clone());
            }
            Err(e) => {
                println!("  ✗ Failed to fix {}: {e}", check.name);
            }
        }
    }

    fixed
}

pub fn run_doctor(args: &[String], json_output: bool) -> ExitCode {
    let do_fix = args.iter().any(|a| a == "--fix");
    let config = get_toolchain_config();
    let mut checks = run_toolchain_checks(config.as_ref());

    let root = workspace_root();
    let core_mtime = get_core_mtime(&root);

    checks.push(check_python_binding(&root, core_mtime));
    checks.push(check_node_binding(&root, core_mtime));
    checks.push(check_wasm_binding(&root, core_mtime));
    checks.push(check_model_weights(&root));

    let mut fixed = Vec::new();

    if do_fix {
        if !json_output {
            println!("Attempting to fix toolchain issues...");
            println!();
        }
        fixed = run_fix(&checks);

        if !fixed.is_empty() {
            checks = run_toolchain_checks(config.as_ref());
            checks.push(check_python_binding(&root, core_mtime));
            checks.push(check_node_binding(&root, core_mtime));
            checks.push(check_wasm_binding(&root, core_mtime));
            checks.push(check_model_weights(&root));
        }
    }

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
        fixed: if fixed.is_empty() { None } else { Some(fixed) },
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

            if let Some(ref expected) = check.expected {
                if check.status == CheckStatus::Fail {
                    print!(" (expected: {expected})");
                }
            }

            println!();

            if check.status == CheckStatus::Fail || check.status == CheckStatus::Warn {
                if let Some(ref fix) = check.fix_command {
                    let auto_tag = if check.auto_fixable == Some(true) {
                        " [auto-fixable with --fix]"
                    } else if check.auto_fixable == Some(false) {
                        " [manual fix required]"
                    } else {
                        ""
                    };
                    println!("      fix: {fix}{auto_tag}");
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
            let manual_fixes: Vec<_> = required_failures
                .iter()
                .filter(|c| c.auto_fixable != Some(true))
                .collect();

            if manual_fixes.is_empty() {
                println!(
                    "Some required checks failed. Run `cargo dfv doctor --fix` to fix them automatically."
                );
            } else {
                println!("Some required checks failed.");
                println!();
                println!("Manual fixes required:");
                for check in manual_fixes {
                    if let Some(ref fix) = check.fix_command {
                        println!("  {}: {fix}", check.name);
                    }
                }
                println!();
                println!(
                    "After manual fixes, run `cargo dfv doctor --fix` for remaining auto-fixable issues."
                );
            }
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
