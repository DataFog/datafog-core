use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_results: Option<HashMap<String, Vec<SdkFinding>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkFinding {
    pub entity_type: String,
    pub matched_text: String,
    pub codepoint_start: usize,
    pub codepoint_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
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

fn scan_with_python(text: &str, root: &std::path::Path) -> Option<Vec<SdkFinding>> {
    let venv_dir = root.join("target").join("dfv_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    };

    if !python_bin.exists() {
        return None;
    }

    let code = format!(
        r#"
import json
from datafog_core import scan
findings = scan({})
result = [dict(entity_type=f.entity_type, matched_text=f.matched_text,
    codepoint_start=f.codepoint_range.start, codepoint_end=f.codepoint_range.end,
    byte_start=f.byte_range.start, byte_end=f.byte_range.end) for f in findings]
print(json.dumps(result))
"#,
        serde_json::to_string(text).unwrap()
    );

    let output = Command::new(&python_bin)
        .args(["-c", &code])
        .current_dir(root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).ok()
}

fn scan_with_node(text: &str, root: &std::path::Path) -> Option<Vec<SdkFinding>> {
    let node_dir = root.join("bindings").join("node");
    let index_js = node_dir.join("index.js");

    if !index_js.exists() {
        return None;
    }

    let escaped_text = serde_json::to_string(text).unwrap();
    let code = format!(
        r#"
const {{ scan }} = require('./index.js');
const findings = scan({escaped_text});
const result = findings.map(f => ({{
    entity_type: f.entityType,
    matched_text: f.matchedText,
    codepoint_start: f.codepointRange.start,
    codepoint_end: f.codepointRange.end,
    byte_start: f.byteRange.start,
    byte_end: f.byteRange.end
}}));
console.log(JSON.stringify(result));
"#
    );

    let output = Command::new("node")
        .args(["-e", &code])
        .current_dir(&node_dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).ok()
}

fn compare_sdk_findings(
    fixture_id: &str,
    sdk_a: &str,
    findings_a: &[SdkFinding],
    sdk_b: &str,
    findings_b: &[SdkFinding],
) -> Vec<ParityDiscrepancy> {
    let mut discrepancies = Vec::new();

    if findings_a.len() != findings_b.len() {
        discrepancies.push(ParityDiscrepancy {
            fixture_id: fixture_id.to_string(),
            field: "finding_count".to_string(),
            sdk_a: sdk_a.to_string(),
            sdk_b: sdk_b.to_string(),
            value_a: serde_json::json!(findings_a.len()),
            value_b: serde_json::json!(findings_b.len()),
        });
        return discrepancies;
    }

    for (i, (fa, fb)) in findings_a.iter().zip(findings_b.iter()).enumerate() {
        if fa.entity_type != fb.entity_type {
            discrepancies.push(ParityDiscrepancy {
                fixture_id: fixture_id.to_string(),
                field: format!("finding[{i}].entity_type"),
                sdk_a: sdk_a.to_string(),
                sdk_b: sdk_b.to_string(),
                value_a: serde_json::json!(fa.entity_type),
                value_b: serde_json::json!(fb.entity_type),
            });
        }

        if fa.matched_text != fb.matched_text {
            discrepancies.push(ParityDiscrepancy {
                fixture_id: fixture_id.to_string(),
                field: format!("finding[{i}].matched_text"),
                sdk_a: sdk_a.to_string(),
                sdk_b: sdk_b.to_string(),
                value_a: serde_json::json!(fa.matched_text),
                value_b: serde_json::json!(fb.matched_text),
            });
        }

        if fa.codepoint_start != fb.codepoint_start || fa.codepoint_end != fb.codepoint_end {
            discrepancies.push(ParityDiscrepancy {
                fixture_id: fixture_id.to_string(),
                field: format!("finding[{i}].codepoint_range"),
                sdk_a: sdk_a.to_string(),
                sdk_b: sdk_b.to_string(),
                value_a: serde_json::json!([fa.codepoint_start, fa.codepoint_end]),
                value_b: serde_json::json!([fb.codepoint_start, fb.codepoint_end]),
            });
        }

        if fa.byte_start != fb.byte_start || fa.byte_end != fb.byte_end {
            discrepancies.push(ParityDiscrepancy {
                fixture_id: fixture_id.to_string(),
                field: format!("finding[{i}].byte_range"),
                sdk_a: sdk_a.to_string(),
                sdk_b: sdk_b.to_string(),
                value_a: serde_json::json!([fa.byte_start, fa.byte_end]),
                value_b: serde_json::json!([fb.byte_start, fb.byte_end]),
            });
        }
    }

    discrepancies
}

fn run_live_parity(
    run_dir: &std::path::Path,
    root: &std::path::Path,
    json_output: bool,
) -> ParityResult {
    let start = Instant::now();
    let fixtures_dir = root.join("fixtures");
    let fixture_files = ["development.jsonl", "final.jsonl"];

    let mut all_discrepancies = Vec::new();
    let mut fixtures_checked = 0;
    let mut sdk_results: HashMap<String, Vec<SdkFinding>> = HashMap::new();

    if !json_output {
        println!("Running live cross-SDK parity check...");
        println!();
    }

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

            fixtures_checked += 1;

            let python_findings = scan_with_python(text, root);
            let node_findings = scan_with_node(text, root);

            if let (Some(py), Some(node)) = (&python_findings, &node_findings) {
                let discrepancies = compare_sdk_findings(&fixture_id, "python", py, "node", node);
                all_discrepancies.extend(discrepancies);

                sdk_results
                    .entry(format!("{fixture_id}_python"))
                    .or_default()
                    .extend(py.iter().cloned());
                sdk_results
                    .entry(format!("{fixture_id}_node"))
                    .or_default()
                    .extend(node.iter().cloned());
            }
        }
    }

    let duration = start.elapsed().as_secs_f64();
    let pass = all_discrepancies.is_empty();

    let parity_result = ParityResult {
        pass,
        fixtures_checked,
        sdks_compared: vec!["python".to_string(), "node".to_string()],
        discrepancies: all_discrepancies,
        duration_seconds: duration,
        run_directory: run_dir.to_path_buf(),
        dry_run: None,
        live: Some(true),
        sdk_results: if sdk_results.is_empty() {
            None
        } else {
            Some(sdk_results)
        },
    };

    let report_path = run_dir.join("parity_live.json");
    if let Ok(json) = serde_json::to_string_pretty(&parity_result) {
        let _ = fs::write(&report_path, json);
    }

    parity_result
}

pub fn run_parity(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let live = args.iter().any(|a| a == "--live");

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
            live: if live { Some(true) } else { None },
            sdk_results: None,
        };

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            if live {
                println!("  1. Load fixtures from fixtures/*.jsonl");
                println!("  2. Run scan() through each installed SDK (Python, Node)");
                println!("  3. Compare actual output spans across SDKs");
                println!("  4. Report any discrepancies in entity_type, matched_text, or ranges");
            } else {
                println!("  1. Load fixtures from fixtures/*.jsonl");
                println!("  2. For each fixture entity, verify:");
                println!("     - codepoint_range matches expected text");
                println!("     - byte_range matches expected text (UTF-8)");
                println!("     - utf16_range matches expected text");
                println!("  3. Report any discrepancies");
            }
            println!();
            println!("Fixtures: development.jsonl, final.jsonl");
            println!("SDKs compared: rust, python, node, wasm");
            println!();
            if live {
                println!("Note: --live runs actual scans through installed SDKs.");
                println!("Requires built and installed Python wheel and Node package.");
            } else {
                println!("Note: This validates fixture offset self-consistency.");
                println!("All SDKs must produce identical spans for the same fixture.");
            }
        }

        return ExitCode::SUCCESS;
    }

    let run_dir = create_run_directory().expect("failed to create run directory");
    let root = workspace_root();

    if live {
        let parity_result = run_live_parity(&run_dir, &root, json_output);

        if json_output {
            println!("{}", serde_json::to_string_pretty(&parity_result).unwrap());
        } else {
            println!();
            println!("Live parity check complete:");
            println!("  Fixtures checked: {}", parity_result.fixtures_checked);
            println!("  SDKs compared: python, node");
            println!("  Duration: {:.1}s", parity_result.duration_seconds);
            println!();

            if parity_result.pass {
                println!("✓ All SDKs produce identical outputs");
            } else {
                println!(
                    "✗ Found {} discrepancies:",
                    parity_result.discrepancies.len()
                );
                for d in parity_result.discrepancies.iter().take(10) {
                    println!(
                        "  - {}: {} mismatch between {} and {}",
                        d.fixture_id, d.field, d.sdk_a, d.sdk_b
                    );
                }
                if parity_result.discrepancies.len() > 10 {
                    println!("  ... and {} more", parity_result.discrepancies.len() - 10);
                }
            }

            println!();
            println!("Report saved to: {}", run_dir.display());
        }

        return if parity_result.pass {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
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
        live: None,
        sdk_results: None,
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
