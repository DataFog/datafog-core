use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub pass: bool,
    pub measurements: Vec<BenchMeasurement>,
    pub summary: BenchSummary,
    pub baseline_comparison: Option<BenchBaselineComparison>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchMeasurement {
    pub run_index: usize,
    pub iterations: usize,
    pub documents: usize,
    pub bytes_per_iteration: usize,
    pub findings: usize,
    pub elapsed_seconds: f64,
    pub mib_per_second: f64,
    pub cold_scan_us: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSummary {
    pub runs: usize,
    pub median_mib_per_second: f64,
    pub mean_mib_per_second: f64,
    pub min_mib_per_second: f64,
    pub max_mib_per_second: f64,
    pub stddev_mib_per_second: f64,
    pub coefficient_of_variation: f64,
    pub variance_assessment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchBaselineComparison {
    pub baseline_path: String,
    pub baseline_median: f64,
    pub current_median: f64,
    pub delta_percent: f64,
    pub status: String,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchBaseline {
    median_mib_per_second: f64,
    #[serde(default)]
    coefficient_of_variation: f64,
    #[serde(default)]
    runs: usize,
}

fn parse_benchmark_output(output: &str) -> Option<BenchMeasurement> {
    let mut cold_scan_us = 0.0;
    let mut documents = 0;
    let mut iterations = 0;
    let mut bytes_per_iteration = 0;
    let mut findings = 0;
    let mut elapsed_seconds = 0.0;
    let mut mib_per_second = 0.0;

    for line in output.lines() {
        if line.starts_with("cold_scan_us=") {
            cold_scan_us = line
                .strip_prefix("cold_scan_us=")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        } else if line.starts_with("documents=") {
            for part in line.split_whitespace() {
                if let Some(val) = part.strip_prefix("documents=") {
                    documents = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("iterations=") {
                    iterations = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("bytes_per_iteration=") {
                    bytes_per_iteration = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("findings=") {
                    findings = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("elapsed_s=") {
                    elapsed_seconds = val.parse().unwrap_or(0.0);
                } else if let Some(val) = part.strip_prefix("mib_s=") {
                    mib_per_second = val.parse().unwrap_or(0.0);
                }
            }
        }
    }

    if documents > 0 && mib_per_second > 0.0 {
        Some(BenchMeasurement {
            run_index: 0,
            iterations,
            documents,
            bytes_per_iteration,
            findings,
            elapsed_seconds,
            mib_per_second,
            cold_scan_us,
        })
    } else {
        None
    }
}

fn run_benchmark_iteration(iterations: usize) -> Option<BenchMeasurement> {
    let root = workspace_root();

    let output = Command::new("cargo")
        .args([
            "run",
            "--package",
            "datafog-core",
            "--release",
            "--example",
            "scan_benchmark",
            "--",
            &iterations.to_string(),
        ])
        .current_dir(&root)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_benchmark_output(&stdout)
        }
        _ => None,
    }
}

fn compute_summary(measurements: &[BenchMeasurement]) -> BenchSummary {
    let runs = measurements.len();
    if runs == 0 {
        return BenchSummary {
            runs: 0,
            median_mib_per_second: 0.0,
            mean_mib_per_second: 0.0,
            min_mib_per_second: 0.0,
            max_mib_per_second: 0.0,
            stddev_mib_per_second: 0.0,
            coefficient_of_variation: 0.0,
            variance_assessment: "no data".to_string(),
        };
    }

    let mut values: Vec<f64> = measurements.iter().map(|m| m.mib_per_second).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let median = if runs % 2 == 0 {
        (values[runs / 2 - 1] + values[runs / 2]) / 2.0
    } else {
        values[runs / 2]
    };

    let mean: f64 = values.iter().sum::<f64>() / runs as f64;
    let min = values[0];
    let max = values[runs - 1];

    let variance: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / runs as f64;
    let stddev = variance.sqrt();

    let cv = if mean > 0.0 {
        (stddev / mean) * 100.0
    } else {
        0.0
    };

    let variance_assessment = if cv < 2.0 {
        "excellent (CV < 2%)".to_string()
    } else if cv < 5.0 {
        "good (CV < 5%)".to_string()
    } else if cv < 10.0 {
        "acceptable (CV < 10%)".to_string()
    } else {
        format!("high variance (CV = {cv:.1}%) - consider adjusting thresholds")
    };

    BenchSummary {
        runs,
        median_mib_per_second: median,
        mean_mib_per_second: mean,
        min_mib_per_second: min,
        max_mib_per_second: max,
        stddev_mib_per_second: stddev,
        coefficient_of_variation: cv,
        variance_assessment,
    }
}

fn compare_with_baseline(
    summary: &BenchSummary,
    baseline_path: &std::path::Path,
) -> Option<BenchBaselineComparison> {
    let content = fs::read_to_string(baseline_path).ok()?;
    let baseline: BenchBaseline = serde_json::from_str(&content).ok()?;

    let delta_percent = ((summary.median_mib_per_second - baseline.median_mib_per_second)
        / baseline.median_mib_per_second)
        * 100.0;

    let (status, pass) = if delta_percent < -10.0 {
        ("fail: >10% slower".to_string(), false)
    } else if delta_percent < -5.0 {
        ("warn: 5-10% slower".to_string(), true)
    } else if delta_percent > 5.0 {
        ("improved: >5% faster".to_string(), true)
    } else {
        ("ok: within 5%".to_string(), true)
    };

    Some(BenchBaselineComparison {
        baseline_path: baseline_path.display().to_string(),
        baseline_median: baseline.median_mib_per_second,
        current_median: summary.median_mib_per_second,
        delta_percent,
        status,
        pass,
    })
}

pub fn run_bench(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let update_baseline = args.iter().any(|a| a == "--update-baseline");

    let runs: usize = args
        .iter()
        .position(|a| a == "--runs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let iterations: usize = args
        .iter()
        .position(|a| a == "--iterations")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    if !json_output && !dry_run {
        println!("Running benchmark ({runs} runs, {iterations} iterations each)...");
        println!();
    }

    let start = Instant::now();
    let run_dir = if dry_run {
        PathBuf::from("/tmp/dfv-dry-run")
    } else {
        create_run_directory().expect("failed to create run directory")
    };

    let root = workspace_root();
    let baseline_path = root.join("baselines/bench.json");

    if dry_run {
        let result = BenchResult {
            pass: true,
            measurements: vec![],
            summary: BenchSummary {
                runs: 0,
                median_mib_per_second: 0.0,
                mean_mib_per_second: 0.0,
                min_mib_per_second: 0.0,
                max_mib_per_second: 0.0,
                stddev_mib_per_second: 0.0,
                coefficient_of_variation: 0.0,
                variance_assessment: "dry run".to_string(),
            },
            baseline_comparison: None,
            duration_seconds: 0.0,
            run_directory: run_dir,
            dry_run: Some(true),
        };

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            println!(
                "  1. Run scan_benchmark example {runs} times with {iterations} iterations each"
            );
            println!("  2. Compute median, mean, min, max, stddev, and coefficient of variation");
            println!("  3. Compare with baselines/bench.json:");
            println!("     - >10% slower: fail");
            println!("     - 5-10% slower: warn");
            println!("     - Within 5%: ok");
            println!("     - >5% faster: improved");
            println!();
            println!("Variance thresholds:");
            println!("  - CV < 2%: excellent");
            println!("  - CV < 5%: good");
            println!("  - CV < 10%: acceptable");
            println!("  - CV >= 10%: high - consider adjusting thresholds");
        }

        return ExitCode::SUCCESS;
    }

    if !json_output {
        println!("Building release binary...");
    }

    let build = Command::new("cargo")
        .args([
            "build",
            "--package",
            "datafog-core",
            "--release",
            "--example",
            "scan_benchmark",
        ])
        .current_dir(&root)
        .status();

    if !build.map(|s| s.success()).unwrap_or(false) {
        let err = serde_json::json!({
            "pass": false,
            "error": "build_failed",
            "message": "Failed to build scan_benchmark example"
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: Failed to build scan_benchmark example");
        }
        return ExitCode::FAILURE;
    }

    let mut measurements = Vec::with_capacity(runs);

    for i in 0..runs {
        if !json_output {
            print!("  Run {}/{}: ", i + 1, runs);
        }

        if let Some(mut measurement) = run_benchmark_iteration(iterations) {
            measurement.run_index = i;

            if !json_output {
                println!("{:.2} MiB/s", measurement.mib_per_second);
            }

            measurements.push(measurement);
        } else if !json_output {
            println!("failed");
        }
    }

    if measurements.is_empty() {
        let err = serde_json::json!({
            "pass": false,
            "error": "no_measurements",
            "message": "No successful benchmark runs"
        });
        if json_output {
            println!("{}", serde_json::to_string_pretty(&err).unwrap());
        } else {
            eprintln!("Error: No successful benchmark runs");
        }
        return ExitCode::FAILURE;
    }

    let summary = compute_summary(&measurements);

    let baseline_comparison = if baseline_path.exists() {
        compare_with_baseline(&summary, &baseline_path)
    } else {
        None
    };

    let pass = baseline_comparison.as_ref().map(|c| c.pass).unwrap_or(true);

    let duration = start.elapsed().as_secs_f64();

    let result = BenchResult {
        pass,
        measurements: measurements.clone(),
        summary: summary.clone(),
        baseline_comparison: baseline_comparison.clone(),
        duration_seconds: duration,
        run_directory: run_dir.clone(),
        dry_run: None,
    };

    let report_path = run_dir.join("bench.json");
    if let Ok(json) = serde_json::to_string_pretty(&result) {
        let _ = fs::write(&report_path, json);
    }

    if update_baseline {
        let baseline_data = BenchBaseline {
            median_mib_per_second: summary.median_mib_per_second,
            coefficient_of_variation: summary.coefficient_of_variation,
            runs: summary.runs,
        };
        if let Some(parent) = baseline_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&baseline_data) {
            let _ = fs::write(&baseline_path, json);
            if !json_output {
                println!();
                println!("Updated baseline: {}", baseline_path.display());
            }
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        println!();
        println!("Benchmark Summary ({} runs):", summary.runs);
        println!("  Median:  {:.2} MiB/s", summary.median_mib_per_second);
        println!("  Mean:    {:.2} MiB/s", summary.mean_mib_per_second);
        println!("  Min:     {:.2} MiB/s", summary.min_mib_per_second);
        println!("  Max:     {:.2} MiB/s", summary.max_mib_per_second);
        println!("  Stddev:  {:.2} MiB/s", summary.stddev_mib_per_second);
        println!("  CV:      {:.1}%", summary.coefficient_of_variation);
        println!();
        println!("Variance: {}", summary.variance_assessment);

        if let Some(ref comparison) = baseline_comparison {
            println!();
            println!("Baseline comparison ({}):", comparison.baseline_path);
            println!("  Baseline: {:.2} MiB/s", comparison.baseline_median);
            println!("  Current:  {:.2} MiB/s", comparison.current_median);
            println!("  Delta:    {:+.1}%", comparison.delta_percent);
            println!("  Status:   {}", comparison.status);
        } else if !baseline_path.exists() {
            println!();
            println!(
                "No baseline found at {}. Run with --update-baseline to create one.",
                baseline_path.display()
            );
        }

        println!();
        if pass {
            println!("✓ Benchmark passed in {duration:.1}s");
        } else {
            println!("✗ Benchmark failed: performance regression detected");
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
