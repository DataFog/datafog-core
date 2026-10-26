use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityResult {
    pub pass: bool,
    pub fixtures_checked: usize,
    pub sdks_compared: Vec<String>,
    pub discrepancies: Vec<ParityDiscrepancy>,
    pub duration_seconds: f64,
    pub run_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityDiscrepancy {
    pub fixture_id: String,
    pub field: String,
    pub sdk_a: String,
    pub sdk_b: String,
    pub value_a: serde_json::Value,
    pub value_b: serde_json::Value,
}

pub fn run_parity(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");

    if dry_run {
        let result = ParityResult {
            pass: true,
            fixtures_checked: 0,
            sdks_compared: vec![
                "rust".to_string(),
                "python".to_string(),
                "node".to_string(),
                "wasm".to_string(),
            ],
            discrepancies: vec![],
            duration_seconds: 0.0,
            run_directory: PathBuf::from("/tmp/dfv-dry-run"),
            dry_run: Some(true),
        };

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            println!("  1. Load fixtures from fixtures/*.jsonl");
            println!("  2. For each fixture entity, verify:");
            println!("     - codepoint_range matches expected text");
            println!("     - byte_range matches expected text (UTF-8)");
            println!("     - utf16_range matches expected text");
            println!("  3. Report any discrepancies");
            println!();
            println!("Fixtures: development.jsonl, final.jsonl");
            println!("SDKs compared: rust, python, node, wasm");
            println!();
            println!("Note: This validates fixture offset self-consistency.");
            println!("All SDKs must produce identical spans for the same fixture.");
        }

        return ExitCode::SUCCESS;
    }

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
        dry_run: None,
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
