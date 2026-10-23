use datafog_core::workspace::{
    self, ErrorCode, Head, PreparedChange, Workspace, WorkspaceError, WorkspaceFinding,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

fn failure(code: ErrorCode) -> JsValue {
    from_error(WorkspaceError {
        code,
        finding_index: None,
    })
}
fn from_error(error: WorkspaceError) -> JsValue {
    JsValue::from_str(&json!({ "code": error.code.as_str(), "message": error.code.as_str(), "findingIndex": error.finding_index }).to_string())
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
        let findings = inputs
            .into_iter()
            .enumerate()
            .map(|(index, f)| {
                if f.range.unit != "utf8_bytes" {
                    return Err(from_error(WorkspaceError {
                        code: ErrorCode::InvalidFinding,
                        finding_index: Some(index),
                    }));
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
            .collect::<Result<Vec<_>, JsValue>>()?;
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
    pub fn preview(&self, handle: u32) -> Result<String, JsValue> {
        let preview = self.plan(handle)?.preview().map_err(from_error)?;
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
        Ok(json!({"text": preview.text, "transformations": ranges}).to_string())
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
