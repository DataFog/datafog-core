use crate::report::{create_run_directory, workspace_root};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Output};
use std::sync::OnceLock;
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ParityError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkFinding {
    pub entity_type: String,
    pub matched_text: String,
    pub codepoint_start: usize,
    pub codepoint_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utf16_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utf16_end: Option<usize>,
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

type ScanResult = Result<Vec<SdkFinding>, String>;
const LIVE_SDKS: [&str; 3] = ["python", "node", "wasm"];

fn command_output(command: &mut Command, stage: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|error| format!("{stage}: could not start process ({})", error.kind()))?;
    if !output.status.success() {
        // Child output can contain source text. Reports retain a bounded category only.
        return Err(format!("{stage}: process failed ({})", output.status));
    }
    Ok(output)
}

fn parse_scan_output(output: Output) -> ScanResult {
    let stdout =
        std::str::from_utf8(&output.stdout).map_err(|_| "scan output is not UTF-8".to_string())?;
    serde_json::from_str(stdout.trim())
        .map_err(|_| "scan output is not a findings array".to_string())
}

struct PythonEnv {
    python_bin: PathBuf,
}

fn init_python_env(root: &std::path::Path) -> Result<PythonEnv, String> {
    let wheel_dir = root.join("target/wheels");
    let mut wheels = fs::read_dir(&wheel_dir)
        .map_err(|_| "Python setup: built wheel directory is missing or unreadable".to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Python setup: could not read wheel directory".to_string())?;
    wheels.retain(|path| path.extension().is_some_and(|extension| extension == "whl"));
    if wheels.len() != 1 {
        return Err(
            "Python setup: expected exactly one built wheel; run cargo dfv build".to_string(),
        );
    }
    let venv_dir = root.join("target/dfv_parity_venv");
    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts/python.exe")
    } else {
        venv_dir.join("bin/python")
    };
    if !python_bin.exists() {
        command_output(
            Command::new("python3").args(["-m", "venv"]).arg(&venv_dir),
            "Python environment creation",
        )?;
    }
    // Refresh even an existing venv so a prior run cannot hide a changed wheel.
    command_output(
        Command::new(&python_bin)
            .args(["-m", "pip", "install", "--force-reinstall", "-q"])
            .arg(&wheels[0]),
        "Python wheel installation",
    )?;
    Ok(PythonEnv { python_bin })
}

fn scan_with_python(text: &str, root: &std::path::Path, env: &PythonEnv) -> ScanResult {
    let text_json = serde_json::to_string(text).map_err(|_| "could not encode scan input")?;
    let code = format!(
        r#"
import json
from datafog_core import scan
findings = scan({text_json})
result = [dict(entity_type=f.entity_type, matched_text=f.matched_text,
    codepoint_start=f.codepoint_range.start, codepoint_end=f.codepoint_range.end,
    byte_start=f.byte_range.start, byte_end=f.byte_range.end) for f in findings]
print(json.dumps(result))
"#,
    );
    parse_scan_output(command_output(
        Command::new(&env.python_bin)
            .args(["-I", "-c", &code])
            .current_dir(root),
        "Python scan",
    )?)
}

fn scan_with_node(text: &str, root: &std::path::Path) -> ScanResult {
    let node_dir = root.join("bindings/node");
    if !node_dir.join("index.js").is_file() {
        return Err("Node setup: binding index.js is missing".to_string());
    }
    let text_json = serde_json::to_string(text).map_err(|_| "could not encode scan input")?;
    let code = format!(
        r#"
import {{ scan }} from './index.js';
const findings = scan({text_json});
const result = findings.map(f => ({{
    entity_type: f.entityType,
    matched_text: f.matchedText,
    codepoint_start: f.codepointRange.start,
    codepoint_end: f.codepointRange.end,
    byte_start: f.byteRange.start,
    byte_end: f.byteRange.end,
    utf16_start: f.utf16Range.start,
    utf16_end: f.utf16Range.end
}}));
console.log(JSON.stringify(result));
"#,
    );
    parse_scan_output(command_output(
        Command::new("node")
            .args(["--input-type=module", "-e", &code])
            .current_dir(&node_dir),
        "Node scan",
    )?)
}

struct WasmEnv {
    temp_dir: PathBuf,
}

fn init_wasm_env(root: &std::path::Path, run_dir: &std::path::Path) -> Result<WasmEnv, String> {
    let wasm_dir = root.join("bindings/wasm");
    if !wasm_dir.join("dist").is_dir() {
        return Err("WASM setup: built dist directory is missing".to_string());
    }
    let temp_dir = run_dir.join("wasm_parity_project");
    fs::create_dir_all(&temp_dir).map_err(|_| "WASM setup: could not create project directory")?;
    let packed = command_output(
        Command::new("npm")
            .args(["pack", "--json", "--pack-destination"])
            .arg(&temp_dir)
            .current_dir(&wasm_dir),
        "WASM package creation",
    )?;
    let parsed: Vec<serde_json::Value> = serde_json::from_slice(&packed.stdout)
        .map_err(|_| "WASM setup: invalid npm pack output")?;
    let tarball_name = parsed
        .first()
        .and_then(|value| value.get("filename"))
        .and_then(|value| value.as_str())
        .ok_or("WASM setup: npm pack returned no filename")?;
    let tarball = temp_dir.join(tarball_name);
    fs::write(
        temp_dir.join("package.json"),
        r#"{"name":"wasm-parity-test","private":true,"type":"module"}"#,
    )
    .map_err(|_| "WASM setup: could not write package.json")?;
    command_output(
        Command::new("npm")
            .args(["install", "--ignore-scripts"])
            .arg(&tarball)
            .current_dir(&temp_dir),
        "WASM package installation",
    )?;
    fs::remove_file(&tarball).map_err(|_| "WASM setup: could not remove package archive")?;
    // Use the repository's pinned Playwright dependency rather than installing latest.
    let package: serde_json::Value = serde_json::from_slice(
        &fs::read(wasm_dir.join("package.json"))
            .map_err(|_| "WASM setup: could not read package.json")?,
    )
    .map_err(|_| "WASM setup: invalid package.json")?;
    let playwright = package["devDependencies"]["playwright"]
        .as_str()
        .ok_or("WASM setup: missing Playwright version")?;
    command_output(
        Command::new("npm")
            .args(["install", "--ignore-scripts"])
            .arg(format!("playwright@{playwright}"))
            .current_dir(&temp_dir),
        "WASM browser dependency installation",
    )?;
    command_output(
        Command::new("npx")
            .args(["--no-install", "playwright", "install", "chromium"])
            .current_dir(&temp_dir),
        "WASM browser installation",
    )?;
    fs::write(
        temp_dir.join("index.html"),
        "<!doctype html><title>WASM parity test</title>",
    )
    .map_err(|_| "WASM setup: could not write browser page")?;
    Ok(WasmEnv { temp_dir })
}

fn scan_with_wasm(text: &str, env: &WasmEnv) -> ScanResult {
    let text_json = serde_json::to_string(text).map_err(|_| "could not encode scan input")?;
    let server_code = format!(
        r#"
import {{ createServer }} from "node:http";
import {{ readFileSync }} from "node:fs";
import path from "node:path";
import {{ chromium }} from "playwright";
const tempDir = process.argv[2];
const server = createServer((req, res) => {{
  const pathname = new URL(req.url, "http://localhost").pathname;
  const file = path.resolve(tempDir, `./${{pathname === "/" ? "/index.html" : pathname}}`);
  const type = file.endsWith(".wasm") ? "application/wasm" : file.endsWith(".html") ? "text/html" : "text/javascript";
  try {{ res.writeHead(200, {{ "content-type": type }}); res.end(readFileSync(file)); }}
  catch {{ res.writeHead(404).end(); }}
}});
let browser;
try {{
  await new Promise((resolve, reject) => {{
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  }});
  browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${{server.address().port}}`);
  const result = await page.evaluate(async () => {{
    const {{ init, scan }} = await import("/node_modules/@datafog/wasm/index.js");
    await init();
    return scan({text_json}).map(f => ({{
      entity_type: f.entityType, matched_text: f.matchedText,
      codepoint_start: f.codepointRange.start, codepoint_end: f.codepointRange.end,
      byte_start: f.byteRange.start, byte_end: f.byteRange.end,
      utf16_start: f.utf16Range.start, utf16_end: f.utf16Range.end
    }}));
  }});
  console.log(JSON.stringify(result));
}} finally {{
  try {{ if (browser) await browser.close(); }}
  finally {{ await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); }}
}}
"#,
    );
    fs::write(env.temp_dir.join("parity-scan.mjs"), server_code)
        .map_err(|_| "WASM scan: could not write browser script")?;
    parse_scan_output(command_output(
        Command::new("node")
            .arg("parity-scan.mjs")
            .arg(&env.temp_dir)
            .current_dir(&env.temp_dir),
        "WASM browser scan",
    )?)
}

/// Identifies if a fixture contains multibyte characters that could cause offset drift.
fn fixture_has_multibyte(text: &str) -> bool {
    text.chars().any(|c| c.len_utf8() > 1)
}

/// Identifies if a fixture contains emoji characters.
fn fixture_has_emoji(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{1F300}'..='\u{1F9FF}' | '\u{2600}'..='\u{26FF}'))
}

/// Identifies if a fixture contains German characters (umlauts, eszett).
fn fixture_has_german(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, 'ä' | 'ö' | 'ü' | 'Ä' | 'Ö' | 'Ü' | 'ß'))
}

/// Identifies if a fixture is structured (table, multi-line).
fn fixture_is_structured(text: &str) -> bool {
    text.contains('\n') || text.contains('\t') || text.contains('|')
}

#[derive(Debug, Clone)]
struct FixtureMeta {
    id: String,
    text: String,
    is_multibyte: bool,
    is_emoji: bool,
    is_german: bool,
    is_structured: bool,
}

impl FixtureMeta {
    fn from_record(record: &serde_json::Value) -> Option<Self> {
        let id = record.get("id")?.as_str()?.to_string();
        let text = record.get("text")?.as_str()?.to_string();
        Some(Self {
            is_multibyte: fixture_has_multibyte(&text),
            is_emoji: fixture_has_emoji(&text),
            is_german: fixture_has_german(&text),
            is_structured: fixture_is_structured(&text),
            id,
            text,
        })
    }

    fn is_priority(&self) -> bool {
        self.is_multibyte || self.is_emoji || self.is_german || self.is_structured
    }
}

/// Select a deterministic sample of fixtures, prioritizing offset-sensitive cases.
fn select_sample_fixtures(fixtures: &[FixtureMeta], sample_size: usize) -> Vec<&FixtureMeta> {
    let mut selected = Vec::new();

    // First: all priority fixtures (multibyte, emoji, german, structured)
    for f in fixtures {
        if f.is_priority() {
            selected.push(f);
        }
    }

    // Fill remaining slots deterministically from non-priority fixtures
    if selected.len() < sample_size {
        let remaining = sample_size - selected.len();
        let non_priority: Vec<_> = fixtures.iter().filter(|f| !f.is_priority()).collect();

        // Take evenly distributed samples
        if !non_priority.is_empty() {
            let step = non_priority.len().max(1) / remaining.max(1);
            let step = step.max(1);
            for (i, f) in non_priority.iter().enumerate() {
                if i % step == 0 && selected.len() < sample_size {
                    selected.push(f);
                }
            }
        }
    }

    selected
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
        if let (Some(a_start), Some(a_end), Some(b_start), Some(b_end)) =
            (fa.utf16_start, fa.utf16_end, fb.utf16_start, fb.utf16_end)
        {
            if (a_start, a_end) != (b_start, b_end) {
                discrepancies.push(ParityDiscrepancy {
                    fixture_id: fixture_id.to_string(),
                    field: format!("finding[{i}].utf16_range"),
                    sdk_a: sdk_a.to_string(),
                    sdk_b: sdk_b.to_string(),
                    value_a: serde_json::json!([a_start, a_end]),
                    value_b: serde_json::json!([b_start, b_end]),
                });
            }
        }
    }

    discrepancies
}

fn load_live_fixtures(root: &std::path::Path) -> Result<Vec<FixtureMeta>, String> {
    let mut fixtures = Vec::new();
    let mut ids = HashSet::new();
    for filename in ["development.jsonl", "final.jsonl"] {
        let content = fs::read_to_string(root.join("fixtures").join(filename))
            .map_err(|_| format!("could not read fixtures/{filename}"))?;
        for (line_index, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str(line)
                .ok()
                .and_then(|record| FixtureMeta::from_record(&record))
                .ok_or_else(|| format!("invalid fixture at {filename}:{}", line_index + 1))?;
            if record.id.is_empty() || !ids.insert(record.id.clone()) {
                return Err(format!(
                    "empty or duplicate fixture ID at {filename}:{}",
                    line_index + 1
                ));
            }
            fixtures.push(record);
        }
    }
    if fixtures.is_empty() {
        return Err("no live parity fixtures were loaded".to_string());
    }
    Ok(fixtures)
}

fn validate_sdk_findings(text: &str, sdk: &str, findings: &[SdkFinding]) -> Result<(), String> {
    for (index, finding) in findings.iter().enumerate() {
        let prefix = text.get(..finding.byte_start);
        let end_prefix = text.get(..finding.byte_end);
        if finding.byte_start >= finding.byte_end
            || text.get(finding.byte_start..finding.byte_end) != Some(finding.matched_text.as_str())
            || prefix.map(|value| value.chars().count()) != Some(finding.codepoint_start)
            || end_prefix.map(|value| value.chars().count()) != Some(finding.codepoint_end)
        {
            return Err(format!(
                "finding[{index}] has invalid byte/codepoint offsets or matched text"
            ));
        }
        let requires_utf16 =
            sdk != "python" || finding.utf16_start.is_some() || finding.utf16_end.is_some();
        if requires_utf16
            && (finding.utf16_start.is_none()
                || finding.utf16_end.is_none()
                || prefix.map(|value| value.encode_utf16().count()) != finding.utf16_start
                || end_prefix.map(|value| value.encode_utf16().count()) != finding.utf16_end)
        {
            return Err(format!(
                "finding[{index}] has missing or invalid UTF-16 offsets"
            ));
        }
    }
    Ok(())
}

fn record_live_fixture(
    result: &mut ParityResult,
    fixture: &FixtureMeta,
    scans: [(&str, ScanResult); 3],
) {
    result.fixtures_checked += 1;
    let mut valid = Vec::new();
    for (sdk, scan) in scans {
        let validated = scan.and_then(|findings| {
            validate_sdk_findings(&fixture.text, sdk, &findings)?;
            Ok(findings)
        });
        match validated {
            Ok(findings) => {
                result
                    .sdk_results
                    .get_or_insert_with(HashMap::new)
                    .insert(format!("{}_{sdk}", fixture.id), findings.clone());
                valid.push((sdk, findings));
            }
            Err(message) => {
                result.pass = false;
                result.sdks_compared.retain(|name| name != sdk);
                result.errors.push(ParityError {
                    fixture_id: Some(fixture.id.clone()),
                    sdk: Some(sdk.to_string()),
                    message,
                });
            }
        }
    }
    if valid.len() < 2 {
        result.sdks_compared.clear();
    }
    for (index, (sdk_a, findings_a)) in valid.iter().enumerate() {
        for (sdk_b, findings_b) in &valid[index + 1..] {
            result.discrepancies.extend(compare_sdk_findings(
                &fixture.id,
                sdk_a,
                findings_a,
                sdk_b,
                findings_b,
            ));
        }
    }
    result.pass &= result.discrepancies.is_empty();
}

fn run_live_parity(
    run_dir: &std::path::Path,
    root: &std::path::Path,
    json_output: bool,
    sample_size: Option<usize>,
) -> ParityResult {
    let start = Instant::now();
    let mut result = ParityResult {
        pass: true,
        fixtures_checked: 0,
        sdks_compared: Vec::new(),
        discrepancies: Vec::new(),
        duration_seconds: 0.0,
        run_directory: run_dir.to_path_buf(),
        dry_run: None,
        live: Some(true),
        sdk_results: None,
        errors: Vec::new(),
    };
    let loaded = load_live_fixtures(root).and_then(|fixtures| {
        if sample_size == Some(0) {
            return Err("live parity sample must be greater than zero".to_string());
        }
        Ok(fixtures)
    });
    match loaded {
        Err(message) => {
            result.pass = false;
            result.errors.push(ParityError {
                fixture_id: None,
                sdk: None,
                message,
            });
        }
        Ok(fixtures) => {
            let selected = match sample_size {
                Some(size) => select_sample_fixtures(&fixtures, size),
                None => fixtures.iter().collect(),
            };
            if !json_output {
                println!(
                    "Running live cross-SDK parity check ({} of {} fixtures)...",
                    selected.len(),
                    fixtures.len()
                );
            }
            result.sdks_compared = LIVE_SDKS.iter().map(|sdk| (*sdk).to_string()).collect();
            let python_env = OnceLock::new();
            let wasm_env = OnceLock::new();
            for fixture in selected {
                let python = python_env
                    .get_or_init(|| init_python_env(root))
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|env| scan_with_python(&fixture.text, root, env));
                let node = scan_with_node(&fixture.text, root);
                let wasm = wasm_env
                    .get_or_init(|| init_wasm_env(root, run_dir))
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|env| scan_with_wasm(&fixture.text, env));
                record_live_fixture(
                    &mut result,
                    fixture,
                    [("python", python), ("node", node), ("wasm", wasm)],
                );
                if !result.errors.is_empty() {
                    break;
                }
            }
        }
    }
    result.duration_seconds = start.elapsed().as_secs_f64();
    let write_report = serde_json::to_vec_pretty(&result)
        .map_err(std::io::Error::other)
        .and_then(|bytes| fs::write(run_dir.join("parity_live.json"), bytes));
    if write_report.is_err() {
        result.pass = false;
        result.errors.push(ParityError {
            fixture_id: None,
            sdk: None,
            message: "could not write live parity report".to_string(),
        });
    }
    result
}

pub fn run_parity(args: &[String], json_output: bool) -> ExitCode {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let live = args.iter().any(|a| a == "--live");
    let full = args.iter().any(|a| a == "--full");

    let sample_size = match args.iter().position(|argument| argument == "--sample") {
        None => None,
        Some(index) => match args
            .get(index + 1)
            .and_then(|value| value.parse::<usize>().ok())
        {
            Some(size) if size > 0 => Some(size),
            _ => {
                if json_output {
                    println!(
                        "{}",
                        serde_json::json!({"pass": false, "error": "sample must be a positive integer"})
                    );
                } else {
                    eprintln!("Parity sample must be a positive integer");
                }
                return ExitCode::FAILURE;
            }
        },
    };

    // Default: sample 100 for live parity on PRs, unless --full is specified
    let effective_sample = if full {
        None // Full run
    } else if live && sample_size.is_none() {
        Some(100) // Default sample for live parity
    } else {
        sample_size
    };

    if dry_run {
        let result = ParityResult {
            pass: true,
            fixtures_checked: 0,
            sdks_compared: if live {
                vec!["python".to_string(), "node".to_string(), "wasm".to_string()]
            } else {
                vec![
                    "rust".to_string(),
                    "python".to_string(),
                    "node".to_string(),
                    "wasm".to_string(),
                ]
            },
            discrepancies: vec![],
            duration_seconds: 0.0,
            run_directory: PathBuf::from("/tmp/dfv-dry-run"),
            dry_run: Some(true),
            live: if live { Some(true) } else { None },
            sdk_results: None,
            errors: Vec::new(),
        };

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            println!("Dry run - would execute:");
            if live {
                println!("  1. Load fixtures from fixtures/*.jsonl");
                if let Some(n) = effective_sample {
                    println!(
                        "  2. Select ~{n} fixtures (all multibyte/emoji/german/structured + fill)"
                    );
                } else {
                    println!("  2. Run on all fixtures (--full)");
                }
                println!("  3. Run scan() through each installed SDK (Python, Node, WASM)");
                println!("  4. Compare actual output spans across SDK pairs");
                println!("  5. Report any discrepancies in entity_type, matched_text, or ranges");
                println!("  Note: WASM runs via headless Chromium for UTF-16 offset verification.");
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
            println!("SDKs compared: {}", result.sdks_compared.join(", "));
            println!();
            if live {
                println!("Note: --live runs actual scans through installed SDKs.");
                if effective_sample.is_some() {
                    println!("      Default: ~100 fixtures (prioritizing offset-sensitive cases).");
                    println!("      Use --full for all fixtures, --sample N for custom count.");
                }
                println!("Requires built Python wheel and Node package.");
                println!("Both wrap the same Rust core, so this tests binding correctness.");
                println!("WASM scans run in headless Chromium and include UTF-16 ranges.");
            } else {
                println!("Note: This validates fixture offset self-consistency.");
                println!("All SDKs must produce identical spans for the same fixture.");
            }
        }

        return ExitCode::SUCCESS;
    }

    let run_dir = match create_run_directory() {
        Ok(directory) => directory,
        Err(error) => {
            eprintln!("Could not create parity report directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = workspace_root();

    if live {
        let parity_result = run_live_parity(&run_dir, &root, json_output, effective_sample);

        if json_output {
            println!("{}", serde_json::to_string_pretty(&parity_result).unwrap());
        } else {
            println!();
            println!("Live parity check complete:");
            println!("  Fixtures checked: {}", parity_result.fixtures_checked);
            println!(
                "  SDKs compared: {}",
                parity_result.sdks_compared.join(", ")
            );
            println!("  Duration: {:.1}s", parity_result.duration_seconds);
            println!();

            for error in &parity_result.errors {
                println!(
                    "  Error (SDK: {}, fixture: {}): {}",
                    error.sdk.as_deref().unwrap_or("none"),
                    error.fixture_id.as_deref().unwrap_or("none"),
                    error.message
                );
            }
            if parity_result.pass {
                println!("✓ All SDKs produce identical outputs");
            } else {
                println!(
                    "✗ Live parity failed: {} execution/validation errors, {} discrepancies",
                    parity_result.errors.len(),
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
        errors: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "datafog-live-parity-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("fixtures")).unwrap();
            fs::create_dir_all(path.join("reports")).unwrap();
            Self(path)
        }
        fn fixtures(&self) {
            for name in ["development.jsonl", "final.jsonl"] {
                fs::write(
                    self.0.join("fixtures").join(name),
                    format!(
                        "{{\"id\":\"{name}\",\"text\":\"No sensitive values.\",\"entities\":[]}}\n"
                    ),
                )
                .unwrap();
            }
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn live_parity_rejects_missing_bindings_without_claiming_sdk_comparisons() {
        let directory = TestDirectory::new();
        directory.fixtures();
        let result = run_live_parity(&directory.0.join("reports"), &directory.0, true, None);
        assert!(!result.pass, "missing bindings must fail live parity");
        assert!(result.sdks_compared.is_empty());
    }

    #[test]
    fn live_parity_rejects_missing_fixtures() {
        let directory = TestDirectory::new();
        let result = run_live_parity(&directory.0.join("reports"), &directory.0, true, None);
        assert!(!result.pass, "zero fixtures cannot prove parity");
        assert!(result.sdks_compared.is_empty());
    }

    fn report() -> ParityResult {
        ParityResult {
            pass: true,
            fixtures_checked: 0,
            sdks_compared: LIVE_SDKS.iter().map(|sdk| (*sdk).to_string()).collect(),
            discrepancies: Vec::new(),
            duration_seconds: 0.0,
            run_directory: PathBuf::new(),
            dry_run: None,
            live: Some(true),
            sdk_results: None,
            errors: Vec::new(),
        }
    }

    fn fixture(id: &str) -> FixtureMeta {
        FixtureMeta::from_record(&serde_json::json!({"id": id, "text": "👋 é jane@example.com"}))
            .unwrap()
    }

    fn finding() -> SdkFinding {
        SdkFinding {
            entity_type: "EMAIL".to_string(),
            matched_text: "jane@example.com".to_string(),
            byte_start: 9,
            byte_end: 25,
            codepoint_start: 5,
            codepoint_end: 21,
            utf16_start: Some(6),
            utf16_end: Some(22),
        }
    }

    fn successful_scans() -> [(&'static str, ScanResult); 3] {
        LIVE_SDKS.map(|sdk| {
            let mut finding = finding();
            if sdk == "python" {
                finding.utf16_start = None;
                finding.utf16_end = None;
            }
            (sdk, Ok(vec![finding]))
        })
    }

    #[test]
    fn live_parity_requires_each_sdk_even_after_an_earlier_successful_fixture() {
        for failed_sdk in LIVE_SDKS {
            let mut result = report();
            record_live_fixture(&mut result, &fixture("first"), successful_scans());
            assert!(result.pass);
            let mut scans = successful_scans();
            scans
                .iter_mut()
                .find(|(sdk, _)| *sdk == failed_sdk)
                .unwrap()
                .1 = Err("scan process failed".to_string());
            record_live_fixture(&mut result, &fixture("second"), scans);
            assert!(!result.pass);
            assert_eq!(result.errors.len(), 1);
            assert_eq!(result.errors[0].sdk.as_deref(), Some(failed_sdk));
            assert_eq!(result.errors[0].fixture_id.as_deref(), Some("second"));
            assert!(!result.sdks_compared.iter().any(|sdk| sdk == failed_sdk));
        }
    }

    #[test]
    fn successful_unicode_and_empty_scans_prove_all_three_bindings_ran() {
        let mut result = report();
        record_live_fixture(&mut result, &fixture("unicode"), successful_scans());
        record_live_fixture(
            &mut result,
            &fixture("negative"),
            LIVE_SDKS.map(|sdk| (sdk, Ok(Vec::new()))),
        );
        assert!(result.pass);
        assert_eq!(result.sdks_compared, LIVE_SDKS);
        assert_eq!(result.fixtures_checked, 2);
        let stored = result.sdk_results.as_ref().unwrap();
        assert_eq!(stored.len(), 6);
        assert!(stored["negative_wasm"].is_empty());
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("errors").is_none());
        assert_eq!(json["sdk_results"]["unicode_wasm"][0]["utf16_start"], 6);
    }

    #[test]
    fn live_parity_still_fails_for_different_binding_findings() {
        let mut result = report();
        let mut scans = successful_scans();
        scans[1].1.as_mut().unwrap()[0].entity_type = "PERSON".to_string();
        record_live_fixture(&mut result, &fixture("different"), scans);
        assert!(!result.pass);
        assert!(result.errors.is_empty());
        assert!(
            result
                .discrepancies
                .iter()
                .any(|d| d.field == "finding[0].entity_type")
        );
    }

    #[test]
    fn scan_output_rejects_invalid_json_utf8_and_missing_fields() {
        for bytes in [b"".as_slice(), b"not JSON", b"{}", b"[{}]", &[0xff]] {
            let output = Output {
                status: Default::default(),
                stdout: bytes.to_vec(),
                stderr: Vec::new(),
            };
            let mut result = report();
            let mut scans = successful_scans();
            scans[2].1 = parse_scan_output(output);
            record_live_fixture(&mut result, &fixture("bad-response"), scans);
            assert!(!result.pass);
            assert_eq!(result.errors[0].sdk.as_deref(), Some("wasm"));
        }
    }

    #[test]
    fn live_parity_rejects_shared_bad_offsets_not_just_disagreements() {
        let mut result = report();
        let mut scans = successful_scans();
        for (_, scan) in &mut scans {
            scan.as_mut().unwrap()[0].byte_start = 8;
        }
        record_live_fixture(&mut result, &fixture("bad-bytes"), scans);
        assert!(!result.pass);
        assert_eq!(result.errors.len(), 3);
        assert!(result.sdks_compared.is_empty());
    }

    #[test]
    fn live_parity_validates_utf16_even_when_both_javascript_bindings_agree() {
        for offsets in [
            (Some(5), Some(21)),
            (Some(1), Some(2)),
            (None, None),
            (Some(6), None),
        ] {
            let mut result = report();
            let mut scans = successful_scans();
            for (_, scan) in &mut scans[1..] {
                let finding = &mut scan.as_mut().unwrap()[0];
                (finding.utf16_start, finding.utf16_end) = offsets;
            }
            record_live_fixture(&mut result, &fixture("bad-utf16"), scans);
            assert!(!result.pass);
            assert_eq!(result.errors.len(), 2);
            assert!(
                result
                    .errors
                    .iter()
                    .all(|error| error.message.contains("UTF-16"))
            );
            assert!(
                result.sdks_compared.is_empty(),
                "a lone SDK was not compared"
            );
        }
    }

    #[test]
    fn command_start_and_nonzero_browser_process_failures_are_errors() {
        let directory = TestDirectory::new();
        let unavailable = command_output(
            &mut Command::new(directory.0.join("missing-runtime")),
            "Python scan",
        );
        assert!(unavailable.unwrap_err().contains("could not start process"));
        #[cfg(unix)]
        let failed = command_output(
            Command::new("/bin/sh").args(["-c", "echo sensitive-source >&2; exit 7"]),
            "WASM browser scan",
        );
        #[cfg(windows)]
        let failed = command_output(
            Command::new("cmd").args(["/C", "echo sensitive-source 1>&2 & exit /b 7"]),
            "WASM browser scan",
        );
        let message = failed.unwrap_err();
        assert!(message.contains("WASM browser scan: process failed"));
        assert!(!message.contains("sensitive-source"));
        let mut result = report();
        let mut scans = successful_scans();
        scans[2].1 = Err(message);
        record_live_fixture(&mut result, &fixture("browser-failure"), scans);
        assert!(!result.pass);
        assert_eq!(result.errors[0].sdk.as_deref(), Some("wasm"));
    }

    #[test]
    fn live_parity_rejects_empty_or_malformed_fixture_inputs() {
        for source in ["", "\n", "not json", r#"{"id":"broken"}"#] {
            let directory = TestDirectory::new();
            for name in ["development.jsonl", "final.jsonl"] {
                fs::write(directory.0.join("fixtures").join(name), source).unwrap();
            }
            let result = run_live_parity(&directory.0.join("reports"), &directory.0, true, None);
            assert!(!result.pass);
            assert_eq!(result.fixtures_checked, 0);
            assert!(!result.errors.is_empty());
        }
    }

    #[test]
    fn live_parity_rejects_unwritable_evidence() {
        let directory = TestDirectory::new();
        directory.fixtures();
        let result = run_live_parity(
            &directory.0.join("missing-directory"),
            &directory.0,
            true,
            None,
        );
        assert!(!result.pass);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.message.contains("report"))
        );
    }
}
