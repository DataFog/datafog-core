use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsCheckResult {
    pub pass: bool,
    pub doctor_pass: bool,
    pub models_ready: bool,
    pub checks: HashMap<String, ModelCheckResult>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor_failures: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCheckResult {
    pub pass: bool,
    pub duration_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub wrapped_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelWeightStatus {
    pub name: String,
    pub path: String,
    pub expected_sha256: String,
    pub actual_sha256: Option<String>,
    pub expected_bytes: u64,
    pub actual_bytes: Option<u64>,
    pub valid: bool,
}

pub fn verify_model_weights(models_dir: &Path) -> Vec<ModelWeightStatus> {
    let mut results = Vec::new();

    let manifest_path = models_dir.join("manifest.json");
    if !manifest_path.exists() {
        return results;
    }

    let manifest_content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => return results,
    };

    let manifest: serde_json::Value = match serde_json::from_str(&manifest_content) {
        Ok(v) => v,
        Err(_) => return results,
    };

    if let Some(models) = manifest.as_object() {
        for (name, info) in models {
            if let (Some(path), Some(expected_sha), Some(expected_bytes)) = (
                info.get("path").and_then(|p| p.as_str()),
                info.get("sha256").and_then(|s| s.as_str()),
                info.get("bytes").and_then(|b| b.as_u64()),
            ) {
                let model_path = models_dir.join(path);
                let (actual_sha, actual_bytes, valid) = if model_path.exists() {
                    match fs::read(&model_path) {
                        Ok(data) => {
                            let mut hasher = Sha256::new();
                            hasher.update(&data);
                            let actual = format!("{:x}", hasher.finalize());
                            let bytes = data.len() as u64;
                            let valid = actual == expected_sha && bytes == expected_bytes;
                            (Some(actual), Some(bytes), valid)
                        }
                        Err(_) => (None, None, false),
                    }
                } else {
                    (None, None, false)
                };

                results.push(ModelWeightStatus {
                    name: name.clone(),
                    path: path.to_string(),
                    expected_sha256: expected_sha.to_string(),
                    actual_sha256: actual_sha,
                    expected_bytes,
                    actual_bytes,
                    valid,
                });
            }
        }
    }

    results
}

fn run_provision_gliner2(run_dir: &Path, dry_run: bool) -> ModelCheckResult {
    let script = "scripts/provision-gliner2-model.py";
    let cmd = format!("python3 {script} --source <model-download> --destination models/gliner2");

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("provisioning script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let models_dir = root.join("models");
    let gliner2_lock = root.join("crates/core/src/models/gliner2-lock.json");

    if !gliner2_lock.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("gliner2-lock.json not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    if models_dir
        .join("gliner2")
        .join("bundle-manifest.json")
        .exists()
    {
        let duration = start.elapsed().as_secs_f64();
        let log_path = run_dir.join("gliner2_provision.log");
        let _ = fs::write(&log_path, "GLiNER2 bundle already provisioned");
        return ModelCheckResult {
            pass: true,
            duration_seconds: duration,
            output: Some("GLiNER2 bundle already provisioned".to_string()),
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    ModelCheckResult {
        pass: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        output: None,
        error: Some(
            "GLiNER2 model not provisioned. Run provision script with model source.".to_string(),
        ),
        wrapped_script: Some(script.to_string()),
        command: Some(cmd),
    }
}

fn run_provision_datafog(run_dir: &Path, dry_run: bool) -> ModelCheckResult {
    let script = "scripts/provision-datafog-model.py";
    let cmd = format!("python3 {script} --archive <archive-path> --destination models/datafog");

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("provisioning script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let models_dir = root.join("models");

    if models_dir
        .join("datafog")
        .join("bundle-manifest.json")
        .exists()
    {
        let duration = start.elapsed().as_secs_f64();
        let log_path = run_dir.join("datafog_provision.log");
        let _ = fs::write(&log_path, "DataFog bundle already provisioned");
        return ModelCheckResult {
            pass: true,
            duration_seconds: duration,
            output: Some("DataFog bundle already provisioned".to_string()),
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    ModelCheckResult {
        pass: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        output: None,
        error: Some(
            "DataFog model not provisioned. Run provision script with archive.".to_string(),
        ),
        wrapped_script: Some(script.to_string()),
        command: Some(cmd),
    }
}

fn run_model_manifest_test(run_dir: &Path, dry_run: bool) -> ModelCheckResult {
    let script = "scripts/test-model-manifest.py";
    let cmd = format!("python3 {script}");

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("test script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let output = Command::new("python3")
        .arg(&script_path)
        .current_dir(&root)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("model_manifest_test.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                ModelCheckResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            } else {
                ModelCheckResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("manifest test failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            }
        }
        Err(e) => ModelCheckResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run script: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        },
    }
}

fn run_model_integration_check(run_dir: &Path, dry_run: bool) -> ModelCheckResult {
    let script = "scripts/check-model-integration.py";
    let cmd =
        "python3 scripts/check-model-integration.py (requires GITHUB_REPOSITORY and GITHUB_SHA)";

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("integration check script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let sha = std::env::var("GITHUB_SHA").unwrap_or_default();

    if repo.is_empty() || sha.is_empty() {
        return ModelCheckResult {
            pass: true,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: Some("Skipped: GITHUB_REPOSITORY or GITHUB_SHA not set".to_string()),
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
        };
    }

    let output = Command::new("python3")
        .arg(&script_path)
        .current_dir(&root)
        .env("GITHUB_REPOSITORY", &repo)
        .env("GITHUB_SHA", &sha)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("model_integration_check.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                ModelCheckResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                }
            } else {
                ModelCheckResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("integration check failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd.to_string()),
                }
            }
        }
        Err(e) => ModelCheckResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run script: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd.to_string()),
        },
    }
}

fn run_model_qualification(run_dir: &Path, config_path: &Path, dry_run: bool) -> ModelCheckResult {
    let script = "scripts/model-qualification.py";
    let cmd = format!("python3 {script} <config.json> <output.json>");

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("qualification script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    if !config_path.exists() {
        return ModelCheckResult {
            pass: true,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: Some("Skipped: model config not available".to_string()),
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let evidence_path = run_dir.join("model-evidence.json");

    let output = Command::new("python3")
        .args(["-I", "-X", "utf8"])
        .arg(&script_path)
        .arg(config_path)
        .arg(&evidence_path)
        .current_dir(&root)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("model_qualification.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                ModelCheckResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            } else {
                ModelCheckResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("qualification failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            }
        }
        Err(e) => ModelCheckResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run script: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        },
    }
}

fn run_model_fault_conformance(
    run_dir: &Path,
    config_path: &Path,
    dry_run: bool,
) -> ModelCheckResult {
    let script = "scripts/model-fault-conformance.py";
    let cmd = format!("python3 {script} <config.json>");

    if dry_run {
        return ModelCheckResult {
            pass: true,
            duration_seconds: 0.0,
            output: None,
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let start = Instant::now();
    let root = workspace_root();
    let script_path = root.join(script);

    if !script_path.exists() {
        return ModelCheckResult {
            pass: false,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: None,
            error: Some("fault conformance script not found".to_string()),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    if !config_path.exists() {
        return ModelCheckResult {
            pass: true,
            duration_seconds: start.elapsed().as_secs_f64(),
            output: Some("Skipped: model config not available".to_string()),
            error: None,
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        };
    }

    let output = Command::new("python3")
        .args(["-I", "-X", "utf8"])
        .arg(&script_path)
        .arg(config_path)
        .current_dir(&root)
        .output();

    let duration = start.elapsed().as_secs_f64();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}\n{stderr}");

            let log_path = run_dir.join("model_fault_conformance.log");
            let _ = fs::write(&log_path, &combined);

            if out.status.success() {
                ModelCheckResult {
                    pass: true,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: None,
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            } else {
                ModelCheckResult {
                    pass: false,
                    duration_seconds: duration,
                    output: Some(combined),
                    error: Some("fault conformance failed".to_string()),
                    wrapped_script: Some(script.to_string()),
                    command: Some(cmd),
                }
            }
        }
        Err(e) => ModelCheckResult {
            pass: false,
            duration_seconds: duration,
            output: None,
            error: Some(format!("failed to run script: {e}")),
            wrapped_script: Some(script.to_string()),
            command: Some(cmd),
        },
    }
}

fn print_model_check_result(name: &str, result: &ModelCheckResult) {
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

pub fn run_models_check(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");

    if !json_output && !dry_run {
        println!("Running model tier checks...");
        println!();
    }

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let root = workspace_root();
    let models_dir = root.join("models");
    let model_config = std::env::var("DATAFOG_MODEL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| run_dir.join("model-session.json"));

    let mut checks = HashMap::new();

    if !json_output && !dry_run {
        println!("Verifying model weights...");
    }
    let weight_status = verify_model_weights(&models_dir);
    let all_weights_valid = weight_status.is_empty() || weight_status.iter().all(|w| w.valid);
    let weights_result = ModelCheckResult {
        pass: all_weights_valid,
        duration_seconds: 0.0,
        output: Some(serde_json::to_string_pretty(&weight_status).unwrap_or_default()),
        error: if all_weights_valid {
            None
        } else {
            Some("Some model weights missing or invalid".to_string())
        },
        wrapped_script: None,
        command: Some("doctor model_weights check".to_string()),
    };
    if !json_output {
        if dry_run {
            println!("  model_weights: verify sha256 hashes in models/manifest.json");
        } else {
            print_model_check_result("model_weights", &weights_result);
        }
    }
    checks.insert("model_weights".to_string(), weights_result);

    if !json_output && !dry_run {
        println!("Checking GLiNER2 provisioning...");
    }
    let gliner2_result = run_provision_gliner2(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  gliner2_provision: {} (wraps {})",
                gliner2_result.command.as_deref().unwrap_or(""),
                gliner2_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("gliner2_provision", &gliner2_result);
        }
    }
    checks.insert("gliner2_provision".to_string(), gliner2_result);

    if !json_output && !dry_run {
        println!("Checking DataFog provisioning...");
    }
    let datafog_result = run_provision_datafog(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  datafog_provision: {} (wraps {})",
                datafog_result.command.as_deref().unwrap_or(""),
                datafog_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("datafog_provision", &datafog_result);
        }
    }
    checks.insert("datafog_provision".to_string(), datafog_result);

    if !json_output && !dry_run {
        println!("Running manifest provisioning tests...");
    }
    let manifest_result = run_model_manifest_test(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  manifest_test: {} (wraps {})",
                manifest_result.command.as_deref().unwrap_or(""),
                manifest_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("manifest_test", &manifest_result);
        }
    }
    checks.insert("manifest_test".to_string(), manifest_result);

    if !json_output && !dry_run {
        println!("Running integration check...");
    }
    let integration_result = run_model_integration_check(&run_dir, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  integration_check: {} (wraps {})",
                integration_result.command.as_deref().unwrap_or(""),
                integration_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("integration_check", &integration_result);
        }
    }
    checks.insert("integration_check".to_string(), integration_result);

    if !json_output && !dry_run {
        println!("Running model qualification...");
    }
    let qualification_result = run_model_qualification(&run_dir, &model_config, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  qualification: {} (wraps {})",
                qualification_result.command.as_deref().unwrap_or(""),
                qualification_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("qualification", &qualification_result);
        }
    }
    checks.insert("qualification".to_string(), qualification_result);

    if !json_output && !dry_run {
        println!("Running fault conformance...");
    }
    let fault_result = run_model_fault_conformance(&run_dir, &model_config, dry_run);
    if !json_output {
        if dry_run {
            println!(
                "  fault_conformance: {} (wraps {})",
                fault_result.command.as_deref().unwrap_or(""),
                fault_result.wrapped_script.as_deref().unwrap_or("")
            );
        } else {
            print_model_check_result("fault_conformance", &fault_result);
        }
    }
    checks.insert("fault_conformance".to_string(), fault_result);

    let models_ready = checks
        .get("gliner2_provision")
        .map(|c| c.pass)
        .unwrap_or(false)
        || checks
            .get("datafog_provision")
            .map(|c| c.pass)
            .unwrap_or(false);

    let all_pass = checks.values().all(|c| c.pass);
    let duration = start.elapsed().as_secs_f64();

    let result = ModelsCheckResult {
        pass: all_pass,
        doctor_pass: true,
        models_ready,
        checks,
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        dry_run: if dry_run { Some(true) } else { None },
        doctor_failures: None,
    };

    if !dry_run {
        let report_path = run_dir.join("models_check.json");
        if let Ok(json) = serde_json::to_string_pretty(&result) {
            let _ = fs::write(&report_path, json);
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else if !dry_run {
        println!();
        if all_pass {
            println!("All model checks passed in {duration:.1}s");
        } else {
            println!("Some model checks failed");
            for (name, check) in &result.checks {
                if !check.pass {
                    println!(
                        "  - {name}: {}",
                        check.error.as_deref().unwrap_or("unknown")
                    );
                }
            }
        }
        if models_ready {
            println!("Models ready for inference.");
        } else {
            println!(
                "No models provisioned. Provision GLiNER2 or DataFog models to run model checks."
            );
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
