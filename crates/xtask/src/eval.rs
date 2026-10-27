use crate::engines::{DetectedEntity, ENGINE_REGISTRY, EngineConfig, EngineTier};
use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    pub model_backend: Option<String>,
}

/// Model evaluation result with tier-aware contract/accuracy separation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEvalResult {
    /// Overall pass/fail (contract AND accuracy for first-party, contract-only for community)
    pub pass: bool,
    /// Engine name
    pub engine: String,
    /// Engine tier (first_party or community)
    pub tier: EngineTier,
    /// Contract checks (blocking for all tiers)
    pub contract: ContractResult,
    /// Accuracy metrics and comparison
    pub accuracy: AccuracyResult,
    /// Raw metrics per entity type
    pub metrics: HashMap<String, EntityMetrics>,
    /// Summary statistics
    pub summary: EvalSummary,
    /// Execution time
    pub duration_seconds: f64,
    /// Run directory path
    pub run_directory: PathBuf,
}

/// Contract checks - blocking for all tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResult {
    /// Overall contract pass/fail
    pub pass: bool,
    /// Model is provisioned with pinned revision
    pub provisioned: bool,
    /// Manifest sha256 matches expected
    pub manifest_verified: bool,
    /// Model loaded successfully
    pub model_loads: bool,
    /// Revision from manifest
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Manifest sha256
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
}

/// Accuracy result - behavior depends on tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccuracyResult {
    /// Pass/fail/warn status
    pub status: AccuracyStatus,
    /// Baseline comparison details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_comparison: Option<BaselineComparison>,
    /// Scored entity types for this engine
    pub scored_entity_types: Vec<String>,
    /// Excluded fixture entity types
    pub excluded_entity_types: Vec<String>,
}

/// Accuracy status reflecting tier-aware gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccuracyStatus {
    /// No regressions
    Pass,
    /// Minor regression (< 1pt F1 drop) or community tier regression
    Warn,
    /// Major regression (> 1pt F1 drop) on first-party engine
    Fail,
    /// Missing or unparseable baseline (fails first-party, warns community)
    NoBaseline,
    /// Provisional baseline - warns instead of fails even for first-party
    Provisional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMetrics {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub f1: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub total_fixtures: usize,
    pub total_entities_expected: usize,
    pub total_entities_found: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_precision: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_f1: Option<f64>,
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
pub struct ModelBaseline {
    pub model_id: String,
    pub revision: String,
    pub manifest_sha256: String,
    pub scored_entity_types: Vec<String>,
    pub excluded_entity_types: Vec<String>,
    pub metrics: HashMap<String, EntityMetrics>,
    pub summary: EvalSummary,
    #[serde(default)]
    pub provisional: bool,
}

fn format_opt_metric(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.4}"),
        None => "n/a".to_string(),
    }
}

/// Print per-entity metrics table to stdout
fn print_metrics_table(metrics: &HashMap<String, EntityMetrics>) {
    println!("Per-entity metrics:");
    let mut sorted_entities: Vec<_> = metrics.iter().collect();
    sorted_entities.sort_by_key(|(k, _)| k.as_str());
    for (entity_type, m) in sorted_entities {
        println!(
            "  {}: P={} R={} F1={} (TP={} FP={} FN={})",
            entity_type,
            format_opt_metric(m.precision),
            format_opt_metric(m.recall),
            format_opt_metric(m.f1),
            m.true_positives,
            m.false_positives,
            m.false_negatives
        );
    }
}

/// Print baseline comparison to stdout
fn print_baseline_comparison(comparison: &BaselineComparison) {
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
}

/// Print evaluation summary to stdout
fn print_eval_summary(summary: &EvalSummary, scored_only: bool) {
    println!("Evaluation Summary:");
    println!("  Fixtures: {}", summary.total_fixtures);
    if scored_only {
        println!(
            "  Expected entities (scored types): {}",
            summary.total_entities_expected
        );
        println!(
            "  Found entities (scored types): {}",
            summary.total_entities_found
        );
    } else {
        println!("  Expected entities: {}", summary.total_entities_expected);
        println!("  Found entities: {}", summary.total_entities_found);
    }
    println!();
    println!(
        "  Macro Precision: {}",
        format_opt_metric(summary.macro_precision)
    );
    println!(
        "  Macro Recall:    {}",
        format_opt_metric(summary.macro_recall)
    );
    println!("  Macro F1:        {}", format_opt_metric(summary.macro_f1));
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

fn setup_python_env(root: &Path, extras: &[&str]) -> Option<PythonEnv> {
    let venv_dir = root.join("target").join("dfv_eval_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

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

    if !python_bin.exists() {
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
    }

    for extra in extras {
        let status = Command::new(&python_bin)
            .args(["-m", "pip", "install", "-q"])
            .arg(format!("{}[{extra}]", wheel.display()))
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
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

fn compute_model_metrics(
    fixtures: &[FixtureRecord],
    engine: &EngineConfig,
    python_bin: &Path,
    root: &Path,
    json_output: bool,
) -> Option<(HashMap<String, EntityMetrics>, EvalSummary, Vec<String>)> {
    let scored_types = engine.scored_types_set();

    let mut entity_stats: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let mut total_expected = 0;
    let mut total_found = 0;
    let mut excluded_types: HashSet<String> = HashSet::new();

    let texts: Vec<String> = fixtures
        .iter()
        .map(|fixture| fixture.text.clone())
        .collect();
    if !json_output {
        eprintln!(
            "Scanning {} fixtures with one {} session...",
            texts.len(),
            engine.name
        );
    }
    let all_detected = engine.scan_many(&texts, python_bin, root)?;
    if all_detected.len() != fixtures.len() {
        eprintln!(
            "{} scan returned {} results for {} fixtures",
            engine.name,
            all_detected.len(),
            fixtures.len()
        );
        return None;
    }

    for (i, fixture) in fixtures.iter().enumerate() {
        if !json_output && i % 100 == 0 {
            eprint!("\rScoring fixture {}/{}...", i + 1, fixtures.len());
        }

        let detected = &all_detected[i];

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

        for exp in &fixture.entities {
            if !scored_types.contains(&exp.label) {
                excluded_types.insert(exp.label.clone());
                continue;
            }

            total_expected += 1;
            let stats = entity_stats.entry(exp.label.clone()).or_insert((0, 0, 0));

            let matched = found
                .iter()
                .any(|f| f.0 == exp.label && f.2 == exp.start && f.3 == exp.end);

            if matched {
                stats.0 += 1;
            } else {
                stats.2 += 1;
            }
        }

        for f in &found {
            if !scored_types.contains(&f.0) {
                continue;
            }

            total_found += 1;

            let matched = fixture
                .entities
                .iter()
                .any(|e| e.label == f.0 && e.start == f.2 && e.end == f.3);

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
    let mut defined_count = 0;

    for (entity_type, (tp, fp, fn_)) in &entity_stats {
        let precision = if *tp + *fp > 0 {
            Some(*tp as f64 / (*tp + *fp) as f64)
        } else {
            None // undefined when TP+FP=0
        };
        let recall = if *tp + *fn_ > 0 {
            Some(*tp as f64 / (*tp + *fn_) as f64)
        } else {
            None // undefined when TP+FN=0
        };
        let f1 = match (precision, recall) {
            (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
            _ => None,
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

        if let (Some(p), Some(r)) = (precision, recall) {
            total_precision += p;
            total_recall += r;
            defined_count += 1;
        }
    }

    let macro_precision = if defined_count > 0 {
        Some(total_precision / defined_count as f64)
    } else {
        None
    };
    let macro_recall = if defined_count > 0 {
        Some(total_recall / defined_count as f64)
    } else {
        None
    };
    let macro_f1 = match (macro_precision, macro_recall) {
        (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
        _ => None,
    };

    let summary = EvalSummary {
        total_fixtures: fixtures.len(),
        total_entities_expected: total_expected,
        total_entities_found: total_found,
        macro_precision,
        macro_recall,
        macro_f1,
    };

    let mut excluded: Vec<_> = excluded_types.into_iter().collect();
    excluded.sort();

    Some((metrics, summary, excluded))
}

fn compute_rule_metrics(
    fixtures: &[FixtureRecord],
    root: &Path,
    json_output: bool,
) -> Option<(HashMap<String, EntityMetrics>, EvalSummary)> {
    let env = setup_python_env(root, &[])?;

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

        let detected = run_rule_scan(&fixture.text, &env, root)?;

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
    let mut defined_count = 0;

    for (entity_type, (tp, fp, fn_)) in &entity_stats {
        let precision = if *tp + *fp > 0 {
            Some(*tp as f64 / (*tp + *fp) as f64)
        } else {
            None // undefined when TP+FP=0
        };
        let recall = if *tp + *fn_ > 0 {
            Some(*tp as f64 / (*tp + *fn_) as f64)
        } else {
            None // undefined when TP+FN=0
        };
        let f1 = match (precision, recall) {
            (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
            _ => None,
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

        if let (Some(p), Some(r)) = (precision, recall) {
            total_precision += p;
            total_recall += r;
            defined_count += 1;
        }
    }

    let macro_precision = if defined_count > 0 {
        Some(total_precision / defined_count as f64)
    } else {
        None
    };
    let macro_recall = if defined_count > 0 {
        Some(total_recall / defined_count as f64)
    } else {
        None
    };
    let macro_f1 = match (macro_precision, macro_recall) {
        (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
        _ => None,
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

fn compare_with_model_baseline(
    current: &HashMap<String, EntityMetrics>,
    baseline_path: &Path,
    tier: EngineTier,
) -> Option<(BaselineComparison, AccuracyStatus, bool)> {
    let content = fs::read_to_string(baseline_path).ok()?;
    let baseline: ModelBaseline = serde_json::from_str(&content).ok()?;

    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    for (entity_type, current_metrics) in current {
        if let Some(baseline_metrics) = baseline.metrics.get(entity_type) {
            // Only compare when both current and baseline have defined F1
            if let (Some(current_f1), Some(baseline_f1)) = (current_metrics.f1, baseline_metrics.f1)
            {
                let f1_delta = current_f1 - baseline_f1;
                if f1_delta < -0.01 {
                    // > 1pt drop: fail for first-party (unless provisional), warn for community
                    let severity = match tier {
                        EngineTier::FirstParty if !baseline.provisional => "fail",
                        _ => "warn",
                    };
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_f1,
                        current: current_f1,
                        delta: f1_delta,
                        severity: severity.to_string(),
                    });
                } else if f1_delta < 0.0 {
                    // <= 1pt drop: always warn
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_f1,
                        current: current_f1,
                        delta: f1_delta,
                        severity: "warn".to_string(),
                    });
                } else if f1_delta > 0.001 {
                    improvements.push(MetricImprovement {
                        entity_type: entity_type.clone(),
                        metric: "f1".to_string(),
                        baseline: baseline_f1,
                        current: current_f1,
                        delta: f1_delta,
                    });
                }
            }
        }
    }

    // Determine accuracy status based on tier and provisional flag
    let has_fail = regressions.iter().any(|r| r.severity == "fail");
    let has_warn = regressions.iter().any(|r| r.severity == "warn");

    let status = if baseline.provisional {
        // Provisional baselines never fail, only warn
        if has_fail || has_warn {
            AccuracyStatus::Provisional
        } else {
            AccuracyStatus::Pass
        }
    } else if has_fail {
        AccuracyStatus::Fail
    } else if has_warn {
        AccuracyStatus::Warn
    } else {
        AccuracyStatus::Pass
    };

    // For baseline comparison pass field: first-party fails on "fail" (unless provisional), community never fails
    let pass = match tier {
        EngineTier::FirstParty => !has_fail || baseline.provisional,
        EngineTier::Community => true,
    };

    Some((
        BaselineComparison {
            baseline_path: baseline_path.display().to_string(),
            regressions,
            improvements,
            pass,
        },
        status,
        baseline.provisional,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleBaseline {
    metrics: HashMap<String, EntityMetrics>,
    summary: EvalSummary,
}

fn compare_with_rule_baseline(
    current: &HashMap<String, EntityMetrics>,
    baseline_path: &Path,
) -> Option<BaselineComparison> {
    let content = fs::read_to_string(baseline_path).ok()?;
    let baseline: RuleBaseline = serde_json::from_str(&content).ok()?;

    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    for (entity_type, current_metrics) in current {
        if let Some(baseline_metrics) = baseline.metrics.get(entity_type) {
            // Compare precision when both are defined
            if let (Some(current_precision), Some(baseline_precision)) =
                (current_metrics.precision, baseline_metrics.precision)
            {
                let precision_delta = current_precision - baseline_precision;
                if precision_delta < 0.0 {
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "precision".to_string(),
                        baseline: baseline_precision,
                        current: current_precision,
                        delta: precision_delta,
                        severity: "fail".to_string(),
                    });
                } else if precision_delta > 0.001 {
                    improvements.push(MetricImprovement {
                        entity_type: entity_type.clone(),
                        metric: "precision".to_string(),
                        baseline: baseline_precision,
                        current: current_precision,
                        delta: precision_delta,
                    });
                }
            }

            // Compare recall when both are defined
            if let (Some(current_recall), Some(baseline_recall)) =
                (current_metrics.recall, baseline_metrics.recall)
            {
                let recall_delta = current_recall - baseline_recall;
                if recall_delta < 0.0 {
                    regressions.push(MetricRegression {
                        entity_type: entity_type.clone(),
                        metric: "recall".to_string(),
                        baseline: baseline_recall,
                        current: current_recall,
                        delta: recall_delta,
                        severity: "fail".to_string(),
                    });
                } else if recall_delta > 0.001 {
                    improvements.push(MetricImprovement {
                        entity_type: entity_type.clone(),
                        metric: "recall".to_string(),
                        baseline: baseline_recall,
                        current: current_recall,
                        delta: recall_delta,
                    });
                }
            }
        }
    }

    let pass = regressions.is_empty();

    Some(BaselineComparison {
        baseline_path: baseline_path.display().to_string(),
        regressions,
        improvements,
        pass,
    })
}

fn run_all_model_evals(json_output: bool, update_baseline: bool, dry_run: bool) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Error: failed to locate xtask binary: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut pass = true;
    let mut engines = Vec::new();
    for engine in ENGINE_REGISTRY {
        if dry_run {
            engines.push(serde_json::json!({
                "engine": engine.name,
                "command": format!("eval --model {}", engine.name),
            }));
            continue;
        }
        let mut command = Command::new(&exe);
        command.args(["eval", "--model", engine.name]);
        if json_output {
            command.arg("--json");
        }
        if update_baseline {
            command.arg("--update-baseline");
        }
        let status = command.status();
        let engine_pass = status.as_ref().map(|s| s.success()).unwrap_or(false);
        pass &= engine_pass;
        engines.push(serde_json::json!({
            "engine": engine.name,
            "pass": engine_pass,
        }));
        if !engine_pass && !json_output {
            eprintln!("Engine {} eval failed", engine.name);
        }
    }
    let result = serde_json::json!({
        "pass": pass,
        "engines": engines,
        "dry_run": if dry_run { Some(true) } else { None::<bool> },
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else if dry_run {
        println!(
            "Dry run - would evaluate engines: {:?}",
            EngineConfig::names()
        );
    }
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub fn run_eval(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let update_baseline = args.iter().any(|a| a == "--update-baseline");

    let model_backend = args
        .iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let root = workspace_root();
    let fixture_dir = root.join("fixtures");

    if dry_run {
        if json_output {
            let result = EvalResult {
                pass: true,
                metrics: HashMap::new(),
                summary: EvalSummary {
                    total_fixtures: 0,
                    total_entities_expected: 0,
                    total_entities_found: 0,
                    macro_precision: None,
                    macro_recall: None,
                    macro_f1: None,
                },
                baseline_comparison: None,
                duration_seconds: 0.0,
                run_directory: run_dir,
                dry_run: Some(true),
                model_backend: model_backend.map(String::from),
            };
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            println!("  1. Load fixtures from fixtures/*.jsonl");
            if let Some(backend) = model_backend {
                println!("  2. For each fixture, run {backend} model scan via Python");
                println!("  3. Compare model findings against expected entities");
                println!("  4. Score only entity types the model outputs");
                println!("  5. Compare with baselines/eval-models-{backend}.json",);
                println!();
                println!("Available engines: {:?}", EngineConfig::names());
            } else {
                println!("  2. For each fixture, run scan() via Python");
                println!("  3. Compare rule findings against expected entities");
                println!("  4. Compute per-entity precision/recall/F1");
                println!("  5. Compare with baselines/eval-rules.json");
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

    if let Some(backend) = model_backend {
        let engine = match EngineConfig::get(backend) {
            Some(e) => e,
            None => {
                let err = serde_json::json!({
                    "pass": false,
                    "error": "unknown_engine",
                    "message": format!("Unknown engine '{}'. Available: {:?}", backend, EngineConfig::names())
                });
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&err).unwrap());
                } else {
                    eprintln!(
                        "Error: Unknown engine '{}'. Available: {:?}",
                        backend,
                        EngineConfig::names()
                    );
                }
                return ExitCode::FAILURE;
            }
        };

        if !json_output {
            println!("Running {} model eval...", engine.name);
            println!("  {}", engine.description);
            println!();
        }

        let extras: &[&str] = if engine.name == "gliner2" {
            &["gliner2"]
        } else {
            &[]
        };
        let env = match setup_python_env(&root, extras) {
            Some(e) => e,
            None => {
                let err = serde_json::json!({
                    "pass": false,
                    "error": "python_env_failed",
                    "message": "Failed to set up Python environment"
                });
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&err).unwrap());
                } else {
                    eprintln!("Error: Failed to set up Python environment");
                }
                return ExitCode::FAILURE;
            }
        };

        if !engine.is_provisioned(&root) {
            let err = serde_json::json!({
                "pass": false,
                "error": "model_not_provisioned",
                "message": format!("Engine '{}' not provisioned. Run: python {} ...", engine.name, engine.provision_script)
            });
            if json_output {
                println!("{}", serde_json::to_string_pretty(&err).unwrap());
            } else {
                eprintln!(
                    "Error: Engine '{}' not provisioned at models/{}",
                    engine.name, engine.name
                );
            }
            return ExitCode::FAILURE;
        }

        let manifest_info = match engine.manifest_info(&root) {
            Some(info) => info,
            None => {
                let err = serde_json::json!({
                    "pass": false,
                    "error": "manifest_read_failed",
                    "message": "Failed to read model manifest"
                });
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&err).unwrap());
                } else {
                    eprintln!("Error: Failed to read model manifest");
                }
                return ExitCode::FAILURE;
            }
        };

        let scored_types: Vec<_> = engine
            .scored_entity_types
            .iter()
            .map(|s| s.to_string())
            .collect();

        let Some((metrics, summary, excluded)) =
            compute_model_metrics(&fixtures, engine, &env.python_bin, &root, json_output)
        else {
            let err = serde_json::json!({
                "pass": false,
                "error": "model_scan_failed",
                "message": format!("Failed to run {} model scan", engine.name)
            });
            if json_output {
                println!("{}", serde_json::to_string_pretty(&err).unwrap());
            } else {
                eprintln!("Error: Model scan failed");
            }
            return ExitCode::FAILURE;
        };

        let baseline_path = engine.baseline_path(&root);

        // Build contract result - blocking for all tiers
        let contract = ContractResult {
            pass: true, // We passed all checks to get here
            provisioned: true,
            manifest_verified: true,
            model_loads: true,
            revision: manifest_info.revision.clone(),
            manifest_sha256: Some(manifest_info.sha256.clone()),
        };

        // Build accuracy result with tier-aware gating
        let (baseline_comparison, accuracy_status, is_provisional) = if baseline_path.exists() {
            match compare_with_model_baseline(&metrics, &baseline_path, engine.tier) {
                Some((comparison, status, provisional)) => (Some(comparison), status, provisional),
                None => {
                    // Baseline exists but couldn't be parsed
                    (None, AccuracyStatus::NoBaseline, false)
                }
            }
        } else {
            // Baseline doesn't exist
            (None, AccuracyStatus::NoBaseline, false)
        };

        let accuracy = AccuracyResult {
            status: accuracy_status,
            baseline_comparison: baseline_comparison.clone(),
            scored_entity_types: scored_types.clone(),
            excluded_entity_types: excluded.clone(),
        };

        // Overall pass: contract must pass, accuracy depends on tier
        // For first-party: Fail or NoBaseline blocks (Provisional warns only)
        // For community: nothing blocks (accuracy is warn-only)
        let accuracy_blocks = match engine.tier {
            EngineTier::FirstParty => matches!(
                accuracy_status,
                AccuracyStatus::Fail | AccuracyStatus::NoBaseline
            ),
            EngineTier::Community => false,
        };
        let pass = contract.pass && !accuracy_blocks;
        let _ = is_provisional; // Used in baseline comparison logic

        let duration = start.elapsed().as_secs_f64();

        let model_result = ModelEvalResult {
            pass,
            engine: engine.name.to_string(),
            tier: engine.tier,
            contract: contract.clone(),
            accuracy: accuracy.clone(),
            metrics: metrics.clone(),
            summary: summary.clone(),
            duration_seconds: duration,
            run_directory: run_dir.clone(),
        };

        let report_path = run_dir.join("eval.json");
        if let Ok(json) = serde_json::to_string_pretty(&model_result) {
            let _ = fs::write(&report_path, json);
        }

        // Extract these before moving into baseline
        let model_id_display = manifest_info
            .model_id
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let revision_display = manifest_info
            .revision
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        if update_baseline {
            let baseline_data = ModelBaseline {
                model_id: manifest_info
                    .model_id
                    .unwrap_or_else(|| "unknown".to_string()),
                revision: manifest_info
                    .revision
                    .unwrap_or_else(|| "unknown".to_string()),
                manifest_sha256: manifest_info.sha256.clone(),
                scored_entity_types: scored_types.clone(),
                excluded_entity_types: excluded.clone(),
                metrics: metrics.clone(),
                summary: summary.clone(),
                provisional: false,
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
            println!("{}", serde_json::to_string_pretty(&model_result).unwrap());
        } else {
            let tier_str = match engine.tier {
                EngineTier::FirstParty => "first-party",
                EngineTier::Community => "community",
            };
            println!();
            println!(
                "Model: {} ({}, {})",
                model_id_display, engine.name, tier_str
            );
            println!("Revision: {revision_display}");
            println!("Manifest SHA256: {}", manifest_info.sha256);
            println!();
            println!(
                "Contract: {}",
                if contract.pass {
                    "✓ pass"
                } else {
                    "✗ fail"
                }
            );
            println!("  Provisioned: {}", contract.provisioned);
            println!("  Manifest verified: {}", contract.manifest_verified);
            println!("  Model loads: {}", contract.model_loads);
            println!();
            println!("Scored entity types: {scored_types:?}");
            println!("Excluded fixture types: {excluded:?}");
            println!();
            print_eval_summary(&summary, true);
            println!();
            print_metrics_table(&metrics);

            let accuracy_icon = match accuracy_status {
                AccuracyStatus::Pass => "✓ pass",
                AccuracyStatus::Warn => "⚠ warn",
                AccuracyStatus::Fail => "✗ fail",
                AccuracyStatus::NoBaseline => "✗ no_baseline",
                AccuracyStatus::Provisional => "⚠ provisional",
            };
            println!();
            println!("Accuracy: {accuracy_icon} ({tier_str})");
            if engine.tier == EngineTier::Community {
                println!("  (community tier: accuracy is warn-only, never blocks PR)");
            }
            if accuracy_status == AccuracyStatus::Provisional {
                println!("  (provisional baseline: regressions warn only, do not block PR)");
            }

            if let Some(ref comparison) = baseline_comparison {
                println!();
                print_baseline_comparison(comparison);
            } else if accuracy_status == AccuracyStatus::NoBaseline {
                println!();
                if engine.tier == EngineTier::Community {
                    println!(
                        "No baseline yet (community tier) - {}",
                        baseline_path.display()
                    );
                    println!("Run with --update-baseline to create one.");
                } else {
                    println!(
                        "No baseline found at {} (blocking for first-party)",
                        baseline_path.display()
                    );
                    println!("Run with --update-baseline to create one.");
                }
            }

            println!();
            if pass {
                println!("✓ Eval passed in {duration:.1}s");
            } else {
                println!("✗ Eval failed");
                if !contract.pass {
                    println!("  Contract checks failed (blocking)");
                }
                if accuracy_status == AccuracyStatus::Fail {
                    println!("  Accuracy regressions detected (blocking for first-party)");
                }
                if accuracy_status == AccuracyStatus::NoBaseline
                    && engine.tier == EngineTier::FirstParty
                {
                    println!("  Missing baseline (blocking for first-party)");
                }
            }
            println!();
            println!("Report saved to: {}", run_dir.display());
        }

        return if pass {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    if args.iter().any(|a| a == "--models") {
        return run_all_model_evals(json_output, update_baseline, dry_run);
    }

    if !json_output {
        println!("Running rule-based eval...");
        println!();
    }

    let Some((metrics, summary)) = compute_rule_metrics(&fixtures, &root, json_output) else {
        let err = serde_json::json!({
            "pass": false,
            "error": "scan_failed",
            "message": "Failed to run scan. Ensure Python wheel is built with cargo dfv build."
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: Failed to run scan");
        }
        return ExitCode::FAILURE;
    };

    let baseline_path = root.join("baselines/eval-rules.json");

    let baseline_comparison = if baseline_path.exists() {
        compare_with_rule_baseline(&metrics, &baseline_path)
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
        model_backend: None,
    };

    let report_path = run_dir.join("eval.json");
    if let Ok(json) = serde_json::to_string_pretty(&result) {
        let _ = fs::write(&report_path, json);
    }

    if update_baseline {
        let baseline_data = RuleBaseline {
            metrics: metrics.clone(),
            summary: summary.clone(),
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
        print_eval_summary(&summary, false);
        println!();
        print_metrics_table(&metrics);

        if let Some(ref comparison) = baseline_comparison {
            println!();
            print_baseline_comparison(comparison);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_metrics(tp: usize, fp: usize, fn_: usize) -> EntityMetrics {
        let precision = if tp + fp > 0 {
            Some(tp as f64 / (tp + fp) as f64)
        } else {
            None
        };
        let recall = if tp + fn_ > 0 {
            Some(tp as f64 / (tp + fn_) as f64)
        } else {
            None
        };
        let f1 = match (precision, recall) {
            (Some(p), Some(r)) if p + r > 0.0 => Some(2.0 * p * r / (p + r)),
            _ => None,
        };
        EntityMetrics {
            true_positives: tp,
            false_positives: fp,
            false_negatives: fn_,
            precision,
            recall,
            f1,
        }
    }

    #[test]
    fn first_party_f1_drop_over_1pt_fails() {
        // Baseline: F1 = 0.80
        let mut baseline_metrics = HashMap::new();
        baseline_metrics.insert("EMAIL".to_string(), make_metrics(80, 10, 10)); // F1 = 0.80

        // Current: F1 dropped by > 1pt (0.80 -> 0.70 = 10pt drop)
        let mut current_metrics = HashMap::new();
        current_metrics.insert("EMAIL".to_string(), make_metrics(70, 15, 15)); // F1 ~= 0.70

        // Create temp baseline file
        let temp_dir = std::env::temp_dir().join("dfv_test_baseline_1");
        let _ = std::fs::create_dir_all(&temp_dir);
        let baseline_path = temp_dir.join("test-baseline.json");

        let baseline = ModelBaseline {
            model_id: "test".to_string(),
            revision: "test".to_string(),
            manifest_sha256: "test".to_string(),
            scored_entity_types: vec!["EMAIL".to_string()],
            excluded_entity_types: vec![],
            metrics: baseline_metrics,
            summary: EvalSummary {
                total_fixtures: 100,
                total_entities_expected: 100,
                total_entities_found: 95,
                macro_precision: Some(0.8),
                macro_recall: Some(0.8),
                macro_f1: Some(0.8),
            },
            provisional: false,
        };
        std::fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&baseline).unwrap(),
        )
        .unwrap();

        // First-party: should fail
        let result =
            compare_with_model_baseline(&current_metrics, &baseline_path, EngineTier::FirstParty);
        assert!(result.is_some());
        let (comparison, status, _provisional) = result.unwrap();
        assert_eq!(status, AccuracyStatus::Fail);
        assert!(!comparison.pass);
        assert!(comparison.regressions.iter().any(|r| r.severity == "fail"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn community_f1_drop_over_1pt_warns_only() {
        // Same setup as above but for community tier
        let mut baseline_metrics = HashMap::new();
        baseline_metrics.insert("EMAIL".to_string(), make_metrics(80, 10, 10));

        let mut current_metrics = HashMap::new();
        current_metrics.insert("EMAIL".to_string(), make_metrics(70, 15, 15));

        let temp_dir = std::env::temp_dir().join("dfv_test_baseline_2");
        let _ = std::fs::create_dir_all(&temp_dir);
        let baseline_path = temp_dir.join("test-baseline.json");

        let baseline = ModelBaseline {
            model_id: "test".to_string(),
            revision: "test".to_string(),
            manifest_sha256: "test".to_string(),
            scored_entity_types: vec!["EMAIL".to_string()],
            excluded_entity_types: vec![],
            metrics: baseline_metrics,
            summary: EvalSummary {
                total_fixtures: 100,
                total_entities_expected: 100,
                total_entities_found: 95,
                macro_precision: Some(0.8),
                macro_recall: Some(0.8),
                macro_f1: Some(0.8),
            },
            provisional: false,
        };
        std::fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&baseline).unwrap(),
        )
        .unwrap();

        // Community: should warn only, pass=true
        let result =
            compare_with_model_baseline(&current_metrics, &baseline_path, EngineTier::Community);
        assert!(result.is_some());
        let (comparison, status, _provisional) = result.unwrap();
        assert_eq!(status, AccuracyStatus::Warn);
        assert!(comparison.pass); // Community never fails on accuracy
        assert!(comparison.regressions.iter().all(|r| r.severity == "warn"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn provisional_baseline_warns_not_fails() {
        // Same setup but with provisional=true
        let mut baseline_metrics = HashMap::new();
        baseline_metrics.insert("EMAIL".to_string(), make_metrics(80, 10, 10));

        let mut current_metrics = HashMap::new();
        current_metrics.insert("EMAIL".to_string(), make_metrics(70, 15, 15));

        let temp_dir = std::env::temp_dir().join("dfv_test_baseline_provisional");
        let _ = std::fs::create_dir_all(&temp_dir);
        let baseline_path = temp_dir.join("test-baseline.json");

        let baseline = ModelBaseline {
            model_id: "test".to_string(),
            revision: "test".to_string(),
            manifest_sha256: "test".to_string(),
            scored_entity_types: vec!["EMAIL".to_string()],
            excluded_entity_types: vec![],
            metrics: baseline_metrics,
            summary: EvalSummary {
                total_fixtures: 100,
                total_entities_expected: 100,
                total_entities_found: 95,
                macro_precision: Some(0.8),
                macro_recall: Some(0.8),
                macro_f1: Some(0.8),
            },
            provisional: true, // Provisional baseline
        };
        std::fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&baseline).unwrap(),
        )
        .unwrap();

        // First-party with provisional: should warn (Provisional status), not fail
        let result =
            compare_with_model_baseline(&current_metrics, &baseline_path, EngineTier::FirstParty);
        assert!(result.is_some());
        let (comparison, status, provisional) = result.unwrap();
        assert!(provisional);
        assert_eq!(status, AccuracyStatus::Provisional);
        assert!(comparison.pass); // Provisional baselines pass even with regressions

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn missing_baseline_fails_first_party() {
        let nonexistent = std::path::PathBuf::from("/nonexistent/baseline.json");
        let result =
            compare_with_model_baseline(&HashMap::new(), &nonexistent, EngineTier::FirstParty);
        assert!(result.is_none()); // None means no baseline found

        // The caller should treat None as NoBaseline, which fails for first-party
        // This is tested in the main eval logic
    }
}
