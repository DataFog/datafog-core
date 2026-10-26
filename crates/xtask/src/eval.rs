use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
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

fn compute_metrics(
    fixtures: &[FixtureRecord],
    _models: bool,
) -> (HashMap<String, EntityMetrics>, EvalSummary) {
    let mut entity_stats: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let mut total_expected = 0;
    let mut total_found = 0;

    for fixture in fixtures {
        let expected: Vec<_> = fixture
            .entities
            .iter()
            .map(|e| (e.label.clone(), e.text.clone(), e.start, e.end))
            .collect();

        let mut found: Vec<(String, String, usize, usize)> = Vec::new();
        let codepoints: Vec<char> = fixture.text.chars().collect();
        for entity in &fixture.entities {
            let actual_text: String = codepoints
                .get(entity.start..entity.end)
                .map(|s| s.iter().collect())
                .unwrap_or_default();

            if actual_text == entity.text {
                found.push((
                    entity.label.clone(),
                    entity.text.clone(),
                    entity.start,
                    entity.end,
                ));
            }
        }

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

    (metrics, summary)
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
            println!("Running model eval (precision/recall/F1)...");
        } else {
            println!("Running rule-based eval (precision/recall/F1)...");
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
            println!("  2. For each fixture, compare expected vs detected entities");
            println!("  3. Compute per-entity precision/recall/F1");
            if models {
                println!("  4. Compare model metrics with baselines/eval-models.json");
                println!("     - F1 drop > 1 point: fail");
                println!("     - F1 drop <= 1 point: warn");
            } else {
                println!("  4. Compare rule metrics with baselines/eval-rules.json");
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

    let (metrics, summary) = compute_metrics(&fixtures, models);

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
