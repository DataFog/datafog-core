use datafog_core::workspace::{
    self, ErrorCode, Head, PreparedChange, Workspace, WorkspaceError, WorkspaceFinding,
    WorkspaceInput,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

fn failure(code: ErrorCode) -> JsValue {
    from_error(WorkspaceError {
        code,
        finding_index: None,
        input_index: None,
        marker_issues: Vec::new(),
    })
}
fn marker_issue_value(issue: &workspace::MarkerIssue) -> serde_json::Value {
    json!({
        "kind": issue.kind.as_str(),
        "byteRange": super::TextRange::from(issue.byte_range),
        "codepointRange": super::TextRange::from(issue.codepoint_range),
        "utf16Range": super::TextRange::from(issue.utf16_range),
    })
}
fn from_error(error: WorkspaceError) -> JsValue {
    let issues: Vec<_> = error.marker_issues.iter().map(marker_issue_value).collect();
    JsValue::from_str(&json!({ "code": error.code.as_str(), "message": error.code.as_str(), "findingIndex": error.finding_index, "inputIndex": error.input_index, "markerIssues": issues }).to_string())
}
fn id(bytes: &[u8]) -> Result<workspace::PublicId, JsValue> {
    bytes.try_into().map_err(|_| failure(ErrorCode::InvalidId))
}
fn head_value(head: Head) -> serde_json::Value {
    json!({ "revision": head.revision.to_string(), "commitId": head.commit_id })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HeadInput {
    revision: String,
    commit_id: Vec<u8>,
}
fn parse_head(json: &str) -> Result<Head, JsValue> {
    if json.len() > 256 {
        return Err(failure(ErrorCode::WorkspaceConflict));
    }
    let input: HeadInput =
        serde_json::from_str(json).map_err(|_| failure(ErrorCode::WorkspaceConflict))?;
    let revision: u64 = input
        .revision
        .parse()
        .map_err(|_| failure(ErrorCode::WorkspaceConflict))?;
    if revision.to_string() != input.revision {
        return Err(failure(ErrorCode::WorkspaceConflict));
    }
    Ok(Head {
        revision,
        commit_id: id(&input.commit_id)?,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RangeInput {
    unit: String,
    start: usize,
    end: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FindingInput {
    entity_type: String,
    matched_text: String,
    range: RangeInput,
    confidence: Option<f32>,
    detector_name: Option<String>,
    detector_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    text: String,
    findings: Vec<FindingInput>,
}

fn adapt_findings(inputs: Vec<FindingInput>) -> Result<Vec<WorkspaceFinding>, WorkspaceError> {
    inputs
        .into_iter()
        .enumerate()
        .map(|(index, f)| {
            if f.range.unit != "utf8_bytes" {
                return Err(WorkspaceError {
                    code: ErrorCode::InvalidFinding,
                    finding_index: Some(index),
                    input_index: None,
                    marker_issues: Vec::new(),
                });
            }
            Ok(WorkspaceFinding {
                entity_type: f.entity_type,
                matched_text: f.matched_text,
                byte_range: datafog_core::TextRange {
                    start: f.range.start,
                    end: f.range.end,
                },
                confidence: f.confidence,
                detector_name: f.detector_name,
                detector_version: f.detector_version,
            })
        })
        .collect()
}

fn preview_value(preview: &workspace::Preview) -> serde_json::Value {
    let ranges: Vec<_> = preview
        .transformations
        .iter()
        .map(|t| {
            json!({
                "sourceByteRange": super::TextRange::from(t.source_byte_range),
                "sourceCodepointRange": super::TextRange::from(t.source_codepoint_range),
                "sourceUtf16Range": super::TextRange::from(t.source_utf16_range),
                "outputByteRange": super::TextRange::from(t.output_byte_range),
                "outputCodepointRange": super::TextRange::from(t.output_codepoint_range),
                "outputUtf16Range": super::TextRange::from(t.output_utf16_range),
            })
        })
        .collect();
    json!({"text": preview.text, "transformations": ranges})
}

/// Raw binding; the package wrapper owns opaque plan objects and safe error conversion.
#[wasm_bindgen(js_name = NativeWorkspace)]
pub struct WasmWorkspace {
    inner: Workspace,
    plans: BTreeMap<u32, PreparedChange>,
    next_plan: u32,
}

#[wasm_bindgen(js_class = NativeWorkspace)]
impl WasmWorkspace {
    pub fn create(namespace: &[u8], operation: &[u8]) -> Result<WasmWorkspace, JsValue> {
        let inner = Workspace::create(id(namespace)?, id(operation)?).map_err(from_error)?;
        Ok(Self {
            inner,
            plans: BTreeMap::new(),
            next_plan: 1,
        })
    }
    pub fn open_bundle(bytes: &[u8]) -> Result<WasmWorkspace, JsValue> {
        let inner = Workspace::open_bundle(bytes).map_err(from_error)?;
        Ok(Self {
            inner,
            plans: BTreeMap::new(),
            next_plan: 1,
        })
    }
    pub fn head(&self) -> Result<String, JsValue> {
        Ok(head_value(self.inner.head().map_err(from_error)?).to_string())
    }
    pub fn namespace(&self) -> Result<Vec<u8>, JsValue> {
        Ok(self.inner.namespace().map_err(from_error)?.to_vec())
    }
    pub fn export_bundle(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.export_bundle().map_err(from_error)
    }
    pub fn prepare_recovery(&mut self, operation: &[u8]) -> Result<u32, JsValue> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = self
            .inner
            .prepare_recovery(id(operation)?)
            .map_err(from_error)?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
    pub fn prepare(
        &mut self,
        text: &str,
        findings_json: &str,
        operation: &[u8],
    ) -> Result<u32, JsValue> {
        if findings_json.len() > workspace::MAX_BUNDLE_BYTES
            || text.len() > workspace::MAX_SOURCE_BYTES
            || self.plans.len() >= 32
        {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let inputs: Vec<FindingInput> =
            serde_json::from_str(findings_json).map_err(|_| failure(ErrorCode::InvalidFinding))?;
        if inputs.len() > workspace::MAX_FINDINGS {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let findings = adapt_findings(inputs).map_err(from_error)?;
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = self
            .inner
            .prepare(text, &findings, id(operation)?)
            .map_err(from_error)?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
    pub fn prepare_batch(&mut self, inputs_json: &str, operation: &[u8]) -> Result<u32, JsValue> {
        if inputs_json.len() > workspace::MAX_BUNDLE_BYTES || self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let inputs: Vec<Input> =
            serde_json::from_str(inputs_json).map_err(|_| failure(ErrorCode::InvalidFinding))?;
        if inputs.len() > workspace::MAX_INPUTS {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let mut findings = Vec::with_capacity(inputs.len());
        for (input_index, input) in inputs.iter().enumerate() {
            if input.findings.len() > workspace::MAX_FINDINGS {
                return Err(from_error(WorkspaceError {
                    code: ErrorCode::ResourceLimit,
                    input_index: Some(input_index),
                    marker_issues: Vec::new(),
                    finding_index: None,
                }));
            }
        }
        for (input_index, input) in inputs.into_iter().enumerate() {
            let adapted = adapt_findings(input.findings).map_err(|mut error| {
                error.input_index = Some(input_index);
                from_error(error)
            })?;
            findings.push((input.text, adapted));
        }
        let borrowed: Vec<_> = findings
            .iter()
            .map(|(text, findings)| WorkspaceInput { text, findings })
            .collect();
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = self
            .inner
            .prepare_batch(&borrowed, id(operation)?)
            .map_err(from_error)?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
    pub fn preview(&self, handle: u32) -> Result<String, JsValue> {
        Ok(preview_value(self.plan(handle)?.preview().map_err(from_error)?).to_string())
    }
    pub fn previews(&self, handle: u32) -> Result<String, JsValue> {
        let previews: Vec<_> = self
            .plan(handle)?
            .previews()
            .map_err(from_error)?
            .iter()
            .map(preview_value)
            .collect();
        Ok(json!(previews).to_string())
    }
    pub fn plan_heads(&self, handle: u32) -> Result<String, JsValue> {
        let plan = self.plan(handle)?;
        Ok(json!({"expectedHead": head_value(plan.expected_head().map_err(from_error)?), "candidateHead": head_value(plan.candidate_head().map_err(from_error)?)}).to_string())
    }
    pub fn export_candidate_bundle(&self, handle: u32) -> Result<Vec<u8>, JsValue> {
        Ok(self
            .plan(handle)?
            .export_candidate_bundle()
            .map_err(from_error)?
            .to_vec())
    }
    pub fn discard(&mut self, handle: u32) -> Result<(), JsValue> {
        let mut plan = self
            .plans
            .remove(&handle)
            .ok_or_else(|| failure(ErrorCode::InactivePlan))?;
        plan.discard();
        Ok(())
    }
    pub fn activate_saved(
        &mut self,
        handle: u32,
        expected_json: &str,
        saved_json: &str,
    ) -> Result<(), JsValue> {
        let expected = parse_head(expected_json)?;
        let saved = parse_head(saved_json)?;
        let plan = self
            .plans
            .get_mut(&handle)
            .ok_or_else(|| failure(ErrorCode::InactivePlan))?;
        self.inner
            .activate_saved(plan, expected, saved)
            .map_err(from_error)?;
        self.plans.remove(&handle);
        Ok(())
    }
    pub fn restore(&self, text: &str) -> Result<String, JsValue> {
        self.inner.restore(text).map_err(from_error)
    }
    pub fn restore_report(&self, text: &str) -> Result<String, JsValue> {
        let report = self.inner.restore_report(text).map_err(from_error)?;
        let issues: Vec<_> = report.issues.iter().map(marker_issue_value).collect();
        Ok(json!({
            "text": report.text,
            "restoredMarkers": report.restored_markers,
            "issues": issues,
        })
        .to_string())
    }
    pub fn close(&mut self) {
        self.plans.clear();
        self.inner.close();
    }
}
impl WasmWorkspace {
    fn plan(&self, handle: u32) -> Result<&PreparedChange, JsValue> {
        self.plans
            .get(&handle)
            .ok_or_else(|| failure(ErrorCode::InactivePlan))
    }
}

#[wasm_bindgen]
pub fn workspace_byte_range_from_utf16(
    text: &str,
    start: usize,
    end: usize,
) -> Result<String, JsValue> {
    let range = workspace::byte_range_from_utf16(text, datafog_core::TextRange { start, end })
        .map_err(from_error)?;
    Ok(json!({"unit": "utf8_bytes", "start": range.start, "end": range.end}).to_string())
}
