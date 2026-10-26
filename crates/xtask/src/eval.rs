use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub pass: bool,
    pub metrics: HashMap<String, EntityMetrics>,
    pub summary: EvalSummary,
    pub baseline_comparison: Option<BaselineComparison>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMetrics {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub total_fixtures: usize,
    pub total_entities_expected: usize,
    pub total_entities_found: usize,
    pub macro_precision: f64,
    pub macro_recall: f64,
    pub macro_f1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub baseline_path: String,
    pub regressions: Vec<MetricRegression>,
    pub improvements: Vec<MetricImprovement>,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRegression {
    pub entity_type: String,
    pub metric: String,
    pub baseline: f64,
    pub current: f64,
    pub delta: f64,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricImprovement {
    pub entity_type: String,
    pub metric: String,
    pub baseline: f64,
    pub current: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureRecord {
    id: String,
    text: String,
    entities: Vec<ExpectedEntity>,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExpectedEntity {
    label: String,
    text: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaselineMetrics {
    metrics: HashMap<String, EntityMetrics>,
    summary: EvalSummary,
    #[serde(default)]
    models: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DetectedEntity {
    entity_type: String,
    matched_text: String,
    codepoint_start: usize,
    codepoint_end: usize,
}

fn load_fixtures(fixture_dir: &Path) -> Vec<FixtureRecord> {
    let mut records = Vec::new();
    let fixture_files = ["development.jsonl", "final.jsonl"];

    for file in &fixture_files {
        let path = fixture_dir.join(file);
        if !path.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str::<FixtureRecord>(line) {
                    records.push(record);
                }
            }
        }
    }

    records
}

struct PythonEnv {
    python_bin: PathBuf,
}

fn setup_python_env(root: &Path) -> Option<PythonEnv> {
    let venv_dir = root.join("target").join("dfv_eval_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

    if python_bin.exists() {
        return Some(PythonEnv { python_bin });
    }

    let wheel_dir = root.join("target").join("wheels");
    let wheel = fs::read_dir(&wheel_dir).ok().and_then(|mut entries| {
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
    })?;

    Command::new("python3")
        .args(["-m", "venv"])
        .arg(&venv_dir)
        .status()
        .ok()?;

    let status = Command::new(&python_bin)
        .args(["-m", "pip", "install", "--force-reinstall", "-q"])
        .arg(&wheel)
        .status()
        .ok()?;

    if !status.success() {
        return None;
    }

    Some(PythonEnv { python_bin })
}

fn run_rule_scan(text: &str, env: &PythonEnv, root: &Path) -> Option<Vec<DetectedEntity>> {
    let code = format!(
        r#"
import json
from datafog_core import scan
findings = scan({})
result = [dict(entity_type=f.entity_type, matched_text=f.matched_text,
    codepoint_start=f.codepoint_range.start, codepoint_end=f.codepoint_range.end) for f in findings]
print(json.dumps(result))
"#,
        serde_json::to_string(text).unwrap()
    );

    let output = Command::new(&env.python_bin)
        .args(["-c", &code])
        .current_dir(root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).ok()
}

fn run_model_scan(text: &str, env: &PythonEnv, root: &Path) -> Option<Vec<DetectedEntity>> {
    let models_dir = root.join("models");
    let datafog_bundle = models_dir.join("datafog");
    let gliner2_bundle = models_dir.join("gliner2");

    let (bundle_path, is_gliner2) = if datafog_bundle.exists() {
        (datafog_bundle, false)
    } else if gliner2_bundle.exists() {
        (gliner2_bundle, true)
    } else {
        return None;
    };

    let manifest_path = bundle_path.join("bundle-manifest.json");
    if !manifest_path.exists() {
        return None;
    }

    let manifest_content = fs::read_to_string(&manifest_path).ok()?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(manifest_content.as_bytes());
    let sha256 = format!("{:x}", hasher.finalize());

    let bundle_str = bundle_path.to_string_lossy().replace('\\', "\\\\");
    let python_str = env.python_bin.to_string_lossy().replace('\\', "\\\\");
    let text_escaped = serde_json::to_string(text).unwrap();

    let config_extras = if is_gliner2 {
        format!(
            r#", "gliner2": {{"python": "{python_str}", "labels": [{{"label": "street_address", "threshold": 0.5}}], "intra_threads": 1, "chunk_size": 384, "chunk_overlap": 64}}"#
        )
    } else {
        String::new()
    };

    let code = format!(
        r#"
import json
from datafog_core import ModelSession
config = {{"bundle": "{bundle_str}", "manifest_sha256": "{sha256}", "limits": {{"max_input_bytes": 65536, "max_response_bytes": 16777216, "max_findings": 65536, "timeout_ms": 120000, "cleanup_timeout_ms": 5000}}{config_extras}}}
with ModelSession.open(config) as session:
    result = session.scan_model({text_escaped})
    findings = [dict(entity_type=f.entity_type, matched_text=f.matched_text,
        codepoint_start=f.codepoint_range.start, codepoint_end=f.codepoint_range.end) for f in result.findings]
    print(json.dumps(findings))
"#
    );

    let output = Command::new(&env.python_bin)
        .args(["-c", &code])
        .current_dir(root)
        .output()
        .ok()?;

    if !output.status.success() {
        eprintln!(
            "Model scan error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).ok()
}

fn compute_metrics(
    fixtures: &[FixtureRecord],
    models: bool,
    root: &Path,
    json_output: bool,
) -> Option<(HashMap<String, EntityMetrics>, EvalSummary)> {
    let env = setup_python_env(root)?;

    let mut entity_stats: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let mut total_expected = 0;
    let mut total_found = 0;

    for (i, fixture) in fixtures.iter().enumerate() {
        if !json_output && i % 100 == 0 {
            eprint!("\rProcessing fixture {}/{}...", i + 1, fixtures.len());
        }

        let expected: Vec<_> = fixture
            .entities
            .iter()
            .map(|e| (e.label.clone(), e.text.clone(), e.start, e.end))
            .collect();

        let detected = if models {
            run_model_scan(&fixture.text, &env, root)?
        } else {
            run_rule_scan(&fixture.text, &env, root)?
        };

        let found: Vec<_> = detected
            .iter()
            .map(|d| {
                (
                    d.entity_type.clone(),
                    d.matched_text.clone(),
                    d.codepoint_start,
                    d.codepoint_end,
                )
            })
            .collect();

        total_expected += expected.len();
        total_found += found.len();

        for exp in &expected {
            let stats = entity_stats.entry(exp.0.clone()).or_insert((0, 0, 0));

            let matched = found
                .iter()
                .any(|f| f.0 == exp.0 && f.2 == exp.2 && f.3 == exp.3);

            if matched {
                stats.0 += 1;
            } else {
                stats.2 += 1;
            }
        }

        for f in &found {
            let matched = expected
                .iter()
                .any(|e| e.0 == f.0 && e.2 == f.2 && e.3 == f.3);
            if !matched {
                let stats = entity_stats.entry(f.0.clone()).or_insert((0, 0, 0));
                stats.1 += 1;
            }
        }
    }

    if !json_output {
        eprintln!();
    }

    let mut metrics = HashMap::new();
    let mut total_precision = 0.0;
    let mut total_recall = 0.0;
    let mut entity_count = 0;

    for (entity_type, (tp, fp, fn_)) in &entity_stats {
        let precision = if *tp + *fp > 0 {
            *tp as f64 / (*tp + *fp) as f64
        } else {
            1.0
        };
        let recall = if *tp + *fn_ > 0 {
            *tp as f64 / (*tp + *fn_) as f64
        } else {
            1.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };

        metrics.insert(
            entity_type.clone(),
            EntityMetrics {
                true_positives: *tp,
                false_positives: *fp,
                false_negatives: *fn_,
                precision,
                recall,
                f1,
            },
        );

        total_precision += precision;
        total_recall += recall;
        entity_count += 1;
    }

    let macro_precision = if entity_count > 0 {
        total_precision / entity_count as f64
    } else {
        1.0
    };
    let macro_recall = if entity_count > 0 {
        total_recall / entity_count as f64
    } else {
        1.0
    };
    let macro_f1 = if macro_precision + macro_recall > 0.0 {
        2.0 * macro_precision * macro_recall / (macro_precision + macro_recall)
    } else {
        0.0
    };

    let summary = EvalSummary {
        total_fixtures: fixtures.len(),
        total_entities_expected: total_expected,
        total_entities_found: total_found,
        macro_precision,
        macro_recall,
        macro_f1,
    };

    Some((metrics, summary))
}

fn compare_with_baseline(
    current: &HashMap<String, EntityMetrics>,
    baseline_path: &Path,
    models: bool,
) -> Option<BaselineComparison> {
    let content = fs::read_to_string(baseline_path).ok()?;
    let baseline: BaselineMetrics = serde_json::from_str(&content).ok()?;

    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    for (entity_type, current_metrics) in current {
        if let Some(baseline_metrics) = baseline.metrics.get(entity_type) {
            let precision_delta = current_metrics.precision - baseline_metrics.precision;
            if precision_delta < 0.0 {
                let severity = if models {
                    if precision_delta.abs() > 0.01 {
                        "fail"
                    } else {
                        "warn"
                    }
                } else {
                    "fail"
                };
                regressions.push(MetricRegression {
                    entity_type: entity_type.clone(),
                    metric: "precision".to_string(),
                    baseline: baseline_metrics.precision,
                    current: current_metrics.precision,
                    delta: precision_delta,
                    severity: severity.to_string(),
                });
            } else if precision_delta > 0.001 {
                improvements.push(MetricImprovement {
                    entity_type: entity_type.clone(),
                    metric: "precision".to_string(),
                    baseline: baseline_metrics.precision,
                    current: current_metrics.precision,
                    delta: precision_delta,
                });
            }

            let recall_delta = current_metrics.recall - baseline_metrics.recall;
            if recall_delta < 0.0 {
                let severity = if models {
                    if recall_delta.abs() > 0.01 {
                        "fail"
                    } else {
                        "warn"
                    }
                } else {
                    "fail"
                };
                regressions.push(MetricRegression {
                    entity_type: entity_type.clone(),
                    metric: "recall".to_string(),
                    baseline: baseline_metrics.recall,
                    current: current_metrics.recall,
                    delta: recall_delta,
                    severity: severity.to_string(),
                });
            } else if recall_delta > 0.001 {
                improvements.push(MetricImprovement {
                    entity_type: entity_type.clone(),
                    metric: "recall".to_string(),
                    baseline: baseline_metrics.recall,
                    current: current_metrics.recall,
                    delta: recall_delta,
                });
            }

            if models {
                let f1_delta = current_metrics.f1 - baseline_metrics.f1;
                if f1_delta < -0.01 {
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_metrics.f1,
                        current: current_metrics.f1,
                        delta: f1_delta,
                        severity: "fail".to_string(),
                    });
                } else if f1_delta < 0.0 {
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_metrics.f1,
                        current: current_metrics.f1,
                        delta: f1_delta,
                        severity: "warn".to_string(),
                    });
                } else if f1_delta > 0.001 {
                    improvements.push(MetricImprovement {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_metrics.f1,
                        current: current_metrics.f1,
                        delta: f1_delta,
                    });
                }
            }
        }
    }

    let pass = if models {
        !regressions.iter().any(|r| r.severity == "fail")
    } else {
        regressions.is_empty()
    };

    Some(BaselineComparison {
        baseline_path: baseline_path.display().to_string(),
        regressions,
        improvements,
        pass,
    })
}

pub fn run_eval(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let models = args.iter().any(|a| a == "--models");
    let update_baseline = args.iter().any(|a| a == "--update-baseline");

    if !json_output && !dry_run {
        if models {
            println!("Running model eval (precision/recall/F1 via ModelSession.scan_model)...");
        } else {
            println!("Running rule-based eval (precision/recall/F1 via scan())...");
        }
        println!();
    }

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let root = workspace_root();
    let fixture_dir = root.join("fixtures");

    if dry_run {
        let result = EvalResult {
            pass: true,
            metrics: HashMap::new(),
            summary: EvalSummary {
                total_fixtures: 0,
                total_entities_expected: 0,
                total_entities_found: 0,
                macro_precision: 1.0,
                macro_recall: 1.0,
                macro_f1: 1.0,
            },
            baseline_comparison: None,
            duration_seconds: 0.0,
            run_directory: run_dir,
            dry_run: Some(true),
            models: if models { Some(true) } else { None },
        };

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            println!("  1. Load fixtures from fixtures/*.jsonl");
            if models {
                println!("  2. For each fixture, run ModelSession.scan_model() via Python");
                println!("  3. Compare model findings against expected entities");
                println!("  4. Compute per-entity precision/recall/F1");
                println!("  5. Compare with baselines/eval-models.json");
                println!("     - F1 drop > 1 point: fail");
                println!("     - F1 drop <= 1 point: warn");
            } else {
                println!("  2. For each fixture, run scan() via Python");
                println!("  3. Compare rule findings against expected entities");
                println!("  4. Compute per-entity precision/recall/F1");
                println!("  5. Compare with baselines/eval-rules.json");
                println!("     - Any precision or recall drop: fail");
            }
        }

        return ExitCode::SUCCESS;
    }

    let fixtures = load_fixtures(&fixture_dir);

    if fixtures.is_empty() {
        let err = serde_json::json!({
            "pass": false,
            "error": "no_fixtures",
            "message": "No fixture files found in fixtures/"
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: No fixture files found in fixtures/");
        }
        return ExitCode::FAILURE;
    }

    if !json_output {
        println!("Loaded {} fixtures", fixtures.len());
    }

    let Some((metrics, summary)) = compute_metrics(&fixtures, models, &root, json_output) else {
        let err = serde_json::json!({
            "pass": false,
            "error": if models { "model_unavailable" } else { "scan_failed" },
            "message": if models {
                "Models not provisioned. Run scripts/provision-gliner2-model.py or scripts/provision-datafog-model.py first."
            } else {
                "Failed to run scan. Ensure Python wheel is built with cargo dfv build."
            }
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: {}", err.get("message").unwrap().as_str().unwrap());
        }
        return ExitCode::FAILURE;
    };

    let baseline_name = if models {
        "baselines/eval-models.json"
    } else {
        "baselines/eval-rules.json"
    };
    let baseline_path = root.join(baseline_name);

    let baseline_comparison = if baseline_path.exists() {
        compare_with_baseline(&metrics, &baseline_path, models)
    } else {
        None
    };

    let pass = baseline_comparison.as_ref().map(|c| c.pass).unwrap_or(true);

    let duration = start.elapsed().as_secs_f64();

    let result = EvalResult {
        pass,
        metrics: metrics.clone(),
        summary: summary.clone(),
        baseline_comparison: baseline_comparison.clone(),
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        dry_run: None,
        models: if models { Some(true) } else { None },
    };

    let report_path = run_dir.join("eval.json");
    if let Ok(json) = serde_json::to_string_pretty(&result) {
        let _ = fs::write(&report_path, json);
    }

    if update_baseline {
        let baseline_data = BaselineMetrics {
            metrics: metrics.clone(),
            summary: summary.clone(),
            models: if models { Some(true) } else { None },
        };
        if let Some(parent) = baseline_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&baseline_data) {
            let _ = fs::write(&baseline_path, json);
            if !json_output {
                println!("Updated baseline: {}", baseline_path.display());
            }
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!();
        println!("Evaluation Summary:");
        println!("  Fixtures: {}", summary.total_fixtures);
        println!("  Expected entities: {}", summary.total_entities_expected);
        println!("  Found entities: {}", summary.total_entities_found);
        println!();
        println!("  Macro Precision: {:.4}", summary.macro_precision);
        println!("  Macro Recall:    {:.4}", summary.macro_recall);
        println!("  Macro F1:        {:.4}", summary.macro_f1);
        println!();

        println!("Per-entity metrics:");
        let mut sorted_entities: Vec<_> = metrics.iter().collect();
        sorted_entities.sort_by_key(|(k, _)| k.as_str());
        for (entity_type, m) in sorted_entities {
            println!(
                "  {}: P={:.4} R={:.4} F1={:.4} (TP={} FP={} FN={})",
                entity_type,
                m.precision,
                m.recall,
                m.f1,
                m.true_positives,
                m.false_positives,
                m.false_negatives
            );
        }

        if let Some(ref comparison) = baseline_comparison {
            println!();
            println!("Baseline comparison ({}):", comparison.baseline_path);

            if comparison.regressions.is_empty() && comparison.improvements.is_empty() {
                println!("  No changes from baseline");
            }

            for reg in &comparison.regressions {
                let icon = if reg.severity == "fail" { "✗" } else { "⚠" };
                println!(
                    "  {} {} {}: {:.4} -> {:.4} ({:+.4})",
                    icon, reg.entity_type, reg.metric, reg.baseline, reg.current, reg.delta
                );
            }

            for imp in &comparison.improvements {
                println!(
                    "  ↑ {} {}: {:.4} -> {:.4} ({:+.4})",
                    imp.entity_type, imp.metric, imp.baseline, imp.current, imp.delta
                );
            }
        } else if !baseline_path.exists() {
            println!();
            println!(
                "No baseline found at {}. Run with --update-baseline to create one.",
                baseline_path.display()
            );
        }

        println!();
        if pass {
            println!("✓ Eval passed in {duration:.1}s");
        } else {
            println!("✗ Eval failed: regressions detected");
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
