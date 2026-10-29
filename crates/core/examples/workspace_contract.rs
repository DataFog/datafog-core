//! Deterministic test-only reference for installed workspace SDK parity.
use datafog_core::{
    TextRange,
    workspace::{
        self as w, FindingSource, PreparedChange, ReviewCorrection, ReviewInput, Workspace,
        WorkspaceError, WorkspaceFinding, WorkspaceInput,
    },
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFinding {
    entity_type: String,
    matched_text: String,
    range: RawRange,
    confidence: Option<f32>,
    detector_name: Option<String>,
    detector_version: Option<String>,
    #[serde(default)]
    source: FindingSource,
    #[serde(default = "needs_review")]
    needs_review: bool,
}
#[derive(Deserialize)]
struct RawRange {
    unit: String,
    start: usize,
    end: usize,
}
fn needs_review() -> bool {
    true
}
impl RawFinding {
    fn finding(self) -> Run<WorkspaceFinding> {
        if self.range.unit != "utf8_bytes" {
            return Err(Failure::Fixture);
        }
        Ok(WorkspaceFinding {
            entity_type: self.entity_type,
            matched_text: self.matched_text,
            byte_range: TextRange {
                start: self.range.start,
                end: self.range.end,
            },
            confidence: self.confidence,
            detector_name: self.detector_name,
            detector_version: self.detector_version,
            source: self.source,
            needs_review: self.needs_review,
        })
    }
}
#[derive(Deserialize)]
struct RawInput {
    text: String,
    findings: Vec<RawFinding>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCorrection {
    candidate_key: String,
    replacement: RawFinding,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    text: String,
    scanner_findings: Vec<RawFinding>,
    user_selections: Vec<RawFinding>,
    #[serde(default)]
    corrections: Vec<RawCorrection>,
}
struct OwnedReview {
    text: String,
    scanner: Vec<WorkspaceFinding>,
    user: Vec<WorkspaceFinding>,
    corrections: Vec<ReviewCorrection>,
}
impl OwnedReview {
    fn input(&self) -> ReviewInput<'_> {
        ReviewInput {
            text: &self.text,
            scanner_findings: &self.scanner,
            user_selections: &self.user,
            corrections: &self.corrections,
        }
    }
}
enum Failure {
    Workspace(WorkspaceError),
    Fixture,
}
impl From<WorkspaceError> for Failure {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
type Run<T> = Result<T, Failure>;
fn parse<T: DeserializeOwned>(value: Value) -> Run<T> {
    serde_json::from_value(value).map_err(|_| Failure::Fixture)
}
fn field<T: DeserializeOwned>(value: &Value, name: &str) -> Run<T> {
    parse(value.get(name).cloned().ok_or(Failure::Fixture)?)
}
fn name<'a>(value: &'a Value, key: &str) -> Run<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(Failure::Fixture)
}
fn id(value: &Value, key: &str) -> Run<w::PublicId> {
    Ok([field::<u8>(value, key)?; 16])
}
fn text_value(command: &Value) -> Run<String> {
    if let Some(repeated) = command.get("textRepeat") {
        let value = name(repeated, "value")?;
        let count: usize = field(repeated, "count")?;
        if count > 2 * 1024 * 1024 {
            return Err(Failure::Fixture);
        }
        return Ok(value.repeat(count));
    }
    field(command, "text")
}
fn unhex(value: &str) -> Run<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(Failure::Fixture);
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|chunk| {
            std::str::from_utf8(chunk)
                .ok()
                .and_then(|text| u8::from_str_radix(text, 16).ok())
                .ok_or(Failure::Fixture)
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn head(value: w::Head) -> Value {
    json!({"revision":value.revision.to_string(),"commitId":value.commit_id})
}
fn issue(value: &w::MarkerIssue) -> Value {
    json!({"kind":value.kind.as_str(),"byteRange":value.byte_range,"codepointRange":value.codepoint_range,"utf16Range":value.utf16_range})
}
fn failure(value: WorkspaceError) -> Value {
    json!({"code":value.code.as_str(),"findingIndex":value.finding_index,"inputIndex":value.input_index,"markerIssues":value.marker_issues.iter().map(issue).collect::<Vec<_>>(),"reviewConflicts":value.review_conflicts})
}
fn preview(value: &w::Preview) -> Value {
    json!({"text":value.text,"transformations":value.transformations.iter().map(|t|json!({"source":t.source.as_str(),"needsReview":t.needs_review,"sourceByteRange":t.source_byte_range,"sourceCodepointRange":t.source_codepoint_range,"sourceUtf16Range":t.source_utf16_range,"outputByteRange":t.output_byte_range,"outputCodepointRange":t.output_codepoint_range,"outputUtf16Range":t.output_utf16_range})).collect::<Vec<_>>()})
}
fn finding_value(value: &WorkspaceFinding) -> Value {
    json!({"entityType":value.entity_type,"matchedText":value.matched_text,"range":{"unit":"utf8_bytes","start":value.byte_range.start,"end":value.byte_range.end},"confidence":value.confidence,"detectorName":value.detector_name,"detectorVersion":value.detector_version,"source":value.source.as_str(),"needsReview":value.needs_review})
}
fn review(value: w::ReviewResult) -> Value {
    json!({"head":head(value.head),"conflicts":value.conflicts,"candidates":value.candidates.iter().map(|c|json!({"key":c.key,"finding":finding_value(&c.finding),"sources":c.sources.iter().map(|source|source.as_str()).collect::<Vec<_>>(),"byteRange":c.finding.byte_range,"codepointRange":c.codepoint_range,"utf16Range":c.utf16_range,"eligible":c.eligible,"selected":c.selected})).collect::<Vec<_>>()})
}
fn recall(value: w::RecallResult) -> Value {
    json!({"head":head(value.head),"complete":true,"findings":value.findings.iter().map(|f|json!({"finding":finding_value(&f.finding),"byteRange":f.finding.byte_range,"codepointRange":f.codepoint_range,"utf16Range":f.utf16_range})).collect::<Vec<_>>()})
}
fn report(value: w::RestoreReport) -> Value {
    json!({"text":value.text,"restoredMarkers":value.restored_markers,"issues":value.issues.iter().map(issue).collect::<Vec<_>>()})
}
fn snapshot(value: &Workspace) -> Run<Value> {
    Ok(
        json!({"head":head(value.head()?),"namespaceHex":hex(&value.namespace()?),"bundleHex":hex(&value.export_bundle()?),"policy":value.policy()?}),
    )
}
fn inspect(value: &PreparedChange) -> Run<Value> {
    Ok(
        json!({"heads":{"expectedHead":head(value.expected_head()?),"candidateHead":head(value.candidate_head()?)},"previews":value.previews()?.iter().map(preview).collect::<Vec<_>>(),"snapshotFormat":value.snapshot_format()?,"policyPreview":value.policy_preview()?,"candidateBundleHex":hex(value.export_candidate_bundle()?)}),
    )
}
#[derive(Default)]
struct Runtime {
    workspaces: BTreeMap<String, Workspace>,
    plans: BTreeMap<String, PreparedChange>,
    plan_texts: BTreeMap<String, Vec<String>>,
    bundles: BTreeMap<String, Vec<u8>>,
    reviews: BTreeMap<String, Value>,
}
impl Runtime {
    fn workspace(&self, command: &Value) -> Run<&Workspace> {
        self.workspaces
            .get(name(command, "workspace")?)
            .ok_or(Failure::Fixture)
    }
    fn review_input(&self, mut value: Value) -> Run<OwnedReview> {
        if let Some(corrections) = value.get_mut("corrections").and_then(Value::as_array_mut) {
            for correction in corrections {
                if let Some(selector) = correction.get("candidateFrom").cloned() {
                    let original = self
                        .reviews
                        .get(name(&selector, "review")?)
                        .ok_or(Failure::Fixture)?;
                    let matches: Vec<_> = original["candidates"]
                        .as_array()
                        .ok_or(Failure::Fixture)?
                        .iter()
                        .filter(|candidate| {
                            candidate["finding"]["entityType"] == selector["entityType"]
                                && candidate["byteRange"]["start"] == selector["start"]
                                && candidate["byteRange"]["end"] == selector["end"]
                        })
                        .collect();
                    if matches.len() != 1 {
                        return Err(Failure::Fixture);
                    }
                    let key = matches[0]["key"].clone();
                    let object = correction.as_object_mut().ok_or(Failure::Fixture)?;
                    object.remove("candidateFrom");
                    object.insert("candidateKey".into(), key);
                }
            }
        }
        let input: RawReview = parse(value)?;
        Ok(OwnedReview {
            text: input.text,
            scanner: input
                .scanner_findings
                .into_iter()
                .map(RawFinding::finding)
                .collect::<Run<_>>()?,
            user: input
                .user_selections
                .into_iter()
                .map(RawFinding::finding)
                .collect::<Run<_>>()?,
            corrections: input
                .corrections
                .into_iter()
                .map(|c| {
                    Ok(ReviewCorrection {
                        candidate_key: c.candidate_key,
                        replacement: c.replacement.finding()?,
                    })
                })
                .collect::<Run<_>>()?,
        })
    }
    fn execute(&mut self, command: &Value) -> Run<Value> {
        let call = name(command, "call")?;
        match call {
            "create" => {
                let workspace = Workspace::create(
                    id(command, "namespaceByte")?,
                    id(command, "operationByte")?,
                )?;
                let value = snapshot(&workspace)?;
                self.workspaces
                    .insert(field(command, "workspace")?, workspace);
                Ok(value)
            }
            "openBundle" => {
                let bytes = if let Some(value) = command.get("inlineBundleHex") {
                    unhex(value.as_str().ok_or(Failure::Fixture)?)?
                } else {
                    self.bundles
                        .get(name(command, "bundle")?)
                        .cloned()
                        .ok_or(Failure::Fixture)?
                };
                let workspace = Workspace::open_bundle(&bytes)?;
                let value = snapshot(&workspace)?;
                self.workspaces
                    .insert(field(command, "workspace")?, workspace);
                Ok(value)
            }
            "head" => Ok(head(self.workspace(command)?.head()?)),
            "export" => {
                let workspace = self.workspace(command)?;
                let bytes = workspace.export_bundle()?;
                let value = snapshot(workspace)?;
                self.bundles.insert(field(command, "bundle")?, bytes);
                Ok(value)
            }
            "close" => {
                self.workspaces
                    .get_mut(name(command, "workspace")?)
                    .ok_or(Failure::Fixture)?
                    .close();
                Ok(Value::Null)
            }
            "inspectPlan" => inspect(
                self.plans
                    .get(name(command, "plan")?)
                    .ok_or(Failure::Fixture)?,
            ),
            "discard" => {
                self.plans
                    .get_mut(name(command, "plan")?)
                    .ok_or(Failure::Fixture)?
                    .discard();
                Ok(Value::Null)
            }
            "activateSaved" => {
                let workspace = self
                    .workspaces
                    .get_mut(name(command, "workspace")?)
                    .ok_or(Failure::Fixture)?;
                let plan = self
                    .plans
                    .get_mut(name(command, "plan")?)
                    .ok_or(Failure::Fixture)?;
                let parent = plan.expected_head()?;
                let saved =
                    if command.get("savedHeadMode").and_then(Value::as_str) == Some("parent") {
                        parent
                    } else {
                        plan.candidate_head()?
                    };
                workspace.activate_saved(plan, parent, saved)?;
                snapshot(workspace)
            }
            "recall" => Ok(recall(
                self.workspace(command)?.recall(&text_value(command)?)?,
            )),
            "review" => {
                let input =
                    self.review_input(command.get("input").cloned().ok_or(Failure::Fixture)?)?;
                let value = review(self.workspace(command)?.review(input.input())?);
                self.reviews
                    .insert(field(command, "review")?, value.clone());
                Ok(value)
            }
            "restore" | "restoreReport" | "restorePlanText" | "restoreReportPlanText" => {
                let mut text = if call.ends_with("PlanText") {
                    self.plan_texts
                        .get(name(command, "plan")?)
                        .and_then(|texts| texts.get(field::<usize>(command, "inputIndex").ok()?))
                        .cloned()
                        .ok_or(Failure::Fixture)?
                } else {
                    text_value(command)?
                };
                if let Some(suffix) = command.get("suffix") {
                    text.push_str(suffix.as_str().ok_or(Failure::Fixture)?);
                }
                let workspace = self.workspace(command)?;
                if call.starts_with("restoreReport") {
                    Ok(report(workspace.restore_report(&text)?))
                } else {
                    Ok(json!(workspace.restore(&text)?))
                }
            }
            "prepare"
            | "prepareBatch"
            | "prepareReview"
            | "preparePolicyChanges"
            | "prepareRecovery" => {
                let workspace = self.workspace(command)?;
                let operation = id(command, "operationByte")?;
                let plan = match call {
                    "prepare" => {
                        let findings = field::<Vec<RawFinding>>(command, "findings")?
                            .into_iter()
                            .map(RawFinding::finding)
                            .collect::<Run<Vec<_>>>()?;
                        workspace.prepare(&text_value(command)?, &findings, operation)?
                    }
                    "prepareBatch" => {
                        let inputs = field::<Vec<RawInput>>(command, "inputs")?
                            .into_iter()
                            .map(|input| {
                                Ok((
                                    input.text,
                                    input
                                        .findings
                                        .into_iter()
                                        .map(RawFinding::finding)
                                        .collect::<Run<Vec<_>>>()?,
                                ))
                            })
                            .collect::<Run<Vec<_>>>()?;
                        let borrowed: Vec<_> = inputs
                            .iter()
                            .map(|(text, findings)| WorkspaceInput { text, findings })
                            .collect();
                        workspace.prepare_batch(&borrowed, operation)?
                    }
                    "prepareReview" => {
                        let inputs = field::<Vec<Value>>(command, "inputs")?
                            .into_iter()
                            .map(|input| self.review_input(input))
                            .collect::<Run<Vec<_>>>()?;
                        let borrowed: Vec<_> = inputs.iter().map(OwnedReview::input).collect();
                        workspace.prepare_review(&borrowed, operation)?
                    }
                    "preparePolicyChanges" => {
                        workspace.prepare_policy_changes(&field(command, "edits")?, operation)?
                    }
                    "prepareRecovery" => workspace.prepare_recovery(operation)?,
                    _ => return Err(Failure::Fixture),
                };
                let value = inspect(&plan)?;
                let texts = plan
                    .previews()?
                    .iter()
                    .map(|preview| preview.text.clone())
                    .collect();
                let key: String = field(command, "plan")?;
                self.plans.insert(key.clone(), plan);
                self.plan_texts.insert(key, texts);
                Ok(value)
            }
            _ => Err(Failure::Fixture),
        }
    }
}
fn run(fixture: Value) -> Run<Value> {
    if fixture["schemaVersion"] != 1 {
        return Err(Failure::Fixture);
    }
    for method in fixture["requiredMethods"]
        .as_array()
        .ok_or(Failure::Fixture)?
    {
        if !matches!(
            method.as_str(),
            Some(
                "create"
                    | "openBundle"
                    | "head"
                    | "namespace"
                    | "exportBundle"
                    | "prepare"
                    | "prepareBatch"
                    | "prepareReview"
                    | "preparePolicyChanges"
                    | "prepareRecovery"
                    | "review"
                    | "recall"
                    | "restore"
                    | "restoreReport"
                    | "activateSaved"
                    | "close"
                    | "plan.heads"
                    | "plan.previews"
                    | "plan.exportCandidateBundle"
                    | "plan.discard"
                    | "plan.snapshotFormat"
                    | "plan.policyPreview"
            )
        ) {
            return Err(Failure::Fixture);
        }
    }
    let scenarios = fixture["cases"]
        .as_array()
        .filter(|cases| !cases.is_empty())
        .ok_or(Failure::Fixture)?;
    let mut cases = Vec::new();
    for scenario in scenarios {
        let mut runtime = Runtime::default();
        let mut steps = Vec::new();
        let commands = scenario["commands"]
            .as_array()
            .filter(|commands| !commands.is_empty())
            .ok_or(Failure::Fixture)?;
        for command in commands {
            let call = name(command, "call")?;
            let expected = command.get("expectCode").and_then(Value::as_str);
            let entry = match runtime.execute(command) {
                Ok(value) => {
                    if expected.is_some() {
                        return Err(Failure::Fixture);
                    }
                    json!({"call":call,"ok":true,"value":value})
                }
                Err(Failure::Workspace(error)) => {
                    if expected != Some(error.code.as_str()) {
                        return Err(Failure::Fixture);
                    }
                    json!({"call":call,"ok":false,"error":failure(error)})
                }
                Err(Failure::Fixture) => return Err(Failure::Fixture),
            };
            steps.push(entry);
        }
        cases.push(json!({"id":name(scenario,"id")?,"steps":steps}));
    }
    Ok(json!({"schemaVersion":1,"profile":fixture["profile"],"cases":cases}))
}
fn main() {
    let result = std::env::args()
        .nth(1)
        .ok_or(Failure::Fixture)
        .and_then(|path| std::fs::read(path).map_err(|_| Failure::Fixture))
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|_| Failure::Fixture))
        .and_then(run);
    match result {
        Ok(value) => {
            if serde_json::to_writer(std::io::stdout().lock(), &value).is_err() {
                std::process::exit(1);
            }
        }
        Err(_) => {
            eprintln!("Rust workspace parity reference failed");
            std::process::exit(1);
        }
    }
}
