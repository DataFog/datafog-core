mod stream;
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
        review_conflicts: Vec::new(),
    })
}
fn operation_record_value(record: &workspace::OperationRecord) -> serde_json::Value {
    json!({"id": record.id, "kind": record.kind.as_str(), "occurredAtMs": record.occurred_at_ms,
        "preparedInputs": record.prepared_inputs, "transformedSpans": record.transformed_spans})
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
    JsValue::from_str(&json!({ "code": error.code.as_str(), "message": error.code.as_str(), "findingIndex": error.finding_index, "inputIndex": error.input_index, "markerIssues": issues, "reviewConflicts": error.review_conflicts }).to_string())
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OperationInput {
    id: Vec<u8>,
    occurred_at_ms: u64,
}
fn operation(json: &str) -> Result<workspace::OperationContext, JsValue> {
    if json.len() > 256 {
        return Err(failure(ErrorCode::InvalidOperation));
    }
    let input: OperationInput =
        serde_json::from_str(json).map_err(|_| failure(ErrorCode::InvalidOperation))?;
    if input.occurred_at_ms > workspace::MAX_OPERATION_TIME_MS {
        return Err(failure(ErrorCode::InvalidOperation));
    }
    Ok(workspace::OperationContext {
        id: id(&input.id)?,
        occurred_at_ms: input.occurred_at_ms,
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OutcomeInput {
    kind: String,
    issue_count: usize,
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
    #[serde(default)]
    source: workspace::FindingSource,
    #[serde(default = "default_needs_review")]
    needs_review: bool,
}

fn default_needs_review() -> bool {
    workspace::DEFAULT_NEEDS_REVIEW
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    text: String,
    findings: Vec<FindingInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CorrectionInput {
    candidate_key: String,
    replacement: FindingInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReviewInputData {
    text: String,
    scanner_findings: Vec<FindingInput>,
    user_selections: Vec<FindingInput>,
    #[serde(default)]
    corrections: Vec<CorrectionInput>,
}
struct AdaptedReview {
    text: String,
    scanner: Vec<WorkspaceFinding>,
    user: Vec<WorkspaceFinding>,
    corrections: Vec<workspace::ReviewCorrection>,
}
impl AdaptedReview {
    fn as_input(&self) -> workspace::ReviewInput<'_> {
        workspace::ReviewInput {
            text: &self.text,
            scanner_findings: &self.scanner,
            user_selections: &self.user,
            corrections: &self.corrections,
        }
    }
}
fn adapt_review(input: ReviewInputData) -> Result<AdaptedReview, WorkspaceError> {
    if input
        .scanner_findings
        .len()
        .saturating_add(input.user_selections.len())
        .saturating_add(input.corrections.len())
        > workspace::MAX_FINDINGS
    {
        return Err(WorkspaceError {
            code: ErrorCode::ResourceLimit,
            finding_index: None,
            input_index: None,
            marker_issues: Vec::new(),
            review_conflicts: Vec::new(),
        });
    }
    let scanner = adapt_findings(input.scanner_findings)?;
    let user = adapt_findings(input.user_selections).map_err(|mut error| {
        error.finding_index = error.finding_index.map(|index| index + scanner.len());
        error
    })?;
    let mut corrections = Vec::with_capacity(input.corrections.len());
    for (index, correction) in input.corrections.into_iter().enumerate() {
        let mut findings = adapt_findings(vec![correction.replacement]).map_err(|mut error| {
            error.finding_index = Some(scanner.len() + user.len() + index);
            error
        })?;
        corrections.push(workspace::ReviewCorrection {
            candidate_key: correction.candidate_key,
            replacement: findings.remove(0),
        });
    }
    Ok(AdaptedReview {
        text: input.text,
        scanner,
        user,
        corrections,
    })
}
fn review_value(result: workspace::ReviewResult) -> serde_json::Value {
    let candidates: Vec<_> = result.candidates.into_iter().map(|candidate| {
        let finding=candidate.finding;
        json!({ "key":candidate.key,"finding":{
            "entityType":finding.entity_type,"matchedText":finding.matched_text,
            "range":{"unit":"utf8_bytes","start":finding.byte_range.start,"end":finding.byte_range.end},
            "confidence":finding.confidence,"detectorName":finding.detector_name,"detectorVersion":finding.detector_version,
            "source":finding.source.as_str(),"needsReview":finding.needs_review
        }, "sources":candidate.sources.iter().map(|source| source.as_str()).collect::<Vec<_>>(),
           "byteRange":finding.byte_range,"codepointRange":candidate.codepoint_range,"utf16Range":candidate.utf16_range,
           "eligible":candidate.eligible,"selected":candidate.selected })
    }).collect();
    json!({"head":head_value(result.head),"candidates":candidates,"conflicts":result.conflicts})
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
                    review_conflicts: Vec::new(),
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
                source: f.source,
                needs_review: f.needs_review,
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
                "source": t.source.as_str(),
                "needsReview": t.needs_review,
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
    streams: stream::Registry,
}

#[wasm_bindgen(js_class = NativeWorkspace)]
impl WasmWorkspace {
    pub fn create(namespace: &[u8], operation: &[u8]) -> Result<WasmWorkspace, JsValue> {
        let inner = Workspace::create(id(namespace)?, id(operation)?).map_err(from_error)?;
        Ok(Self {
            inner,
            plans: BTreeMap::new(),
            next_plan: 1,
            streams: stream::Registry::default(),
        })
    }
    pub fn create_with_operation(namespace: &[u8], json: &str) -> Result<WasmWorkspace, JsValue> {
        let inner = Workspace::create_with_operation(id(namespace)?, operation(json)?)
            .map_err(from_error)?;
        Ok(Self {
            inner,
            plans: BTreeMap::new(),
            next_plan: 1,
            streams: stream::Registry::default(),
        })
    }
    pub fn operations(&self) -> Result<String, JsValue> {
        let values: Vec<_> = self
            .inner
            .operations()
            .map_err(from_error)?
            .iter()
            .map(operation_record_value)
            .collect();
        Ok(json!(values).to_string())
    }
    pub fn operations_page(&self, offset: usize, limit: usize) -> Result<String, JsValue> {
        let page = self
            .inner
            .operations_page(offset, limit)
            .map_err(from_error)?;
        let records: Vec<_> = page.records.iter().map(operation_record_value).collect();
        Ok(
            json!({"records": records, "total": page.total, "offset": page.offset,
            "nextOffset": page.next_offset})
            .to_string(),
        )
    }
    pub fn history_coverage(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner.history_coverage().map_err(from_error)?)
            .map_err(|_| failure(ErrorCode::ResourceLimit))
    }
    pub fn counts(&self) -> Result<String, JsValue> {
        let counts = self.inner.counts().map_err(from_error)?;
        Ok(json!({"operations": counts.operations, "preparedInputs": counts.prepared_inputs,
            "transformedSpans": counts.transformed_spans, "confirmedHandoffs": counts.confirmed_handoffs,
            "reportedBlockedAttempts": counts.reported_blocked_attempts}).to_string())
    }
    pub fn prepare_outcome(
        &mut self,
        outcome_json: &str,
        operation_json: &str,
    ) -> Result<u32, JsValue> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        if outcome_json.len() > 128 {
            return Err(failure(ErrorCode::InvalidOperation));
        }
        let input: OutcomeInput =
            serde_json::from_str(outcome_json).map_err(|_| failure(ErrorCode::InvalidOperation))?;
        let kind = match input.kind.as_str() {
            "copy_confirmed" => workspace::OutcomeKind::CopyConfirmed,
            "user_cancelled" => workspace::OutcomeKind::UserCancelled,
            "blocked_input" => workspace::OutcomeKind::BlockedInput,
            "restore_blocked" => workspace::OutcomeKind::RestoreBlocked,
            _ => return Err(failure(ErrorCode::InvalidOperation)),
        };
        let plan = self
            .inner
            .prepare_outcome(
                workspace::OperationOutcome {
                    kind,
                    issue_count: input.issue_count,
                },
                operation(operation_json)?,
            )
            .map_err(from_error)?;
        self.retain_plan(plan)
    }
    pub fn open_bundle(bytes: &[u8]) -> Result<WasmWorkspace, JsValue> {
        let inner = Workspace::open_bundle(bytes).map_err(from_error)?;
        Ok(Self {
            inner,
            plans: BTreeMap::new(),
            next_plan: 1,
            streams: stream::Registry::default(),
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
    pub fn policy(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner.policy().map_err(from_error)?)
            .map_err(|_| failure(ErrorCode::InvalidPolicy))
    }
    pub fn prepare_policy_changes(
        &mut self,
        edits_json: &str,
        operation: &[u8],
    ) -> Result<u32, JsValue> {
        if edits_json.len() > workspace::MAX_BUNDLE_BYTES || self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let edits: workspace::PolicyEdits =
            serde_json::from_str(edits_json).map_err(|_| failure(ErrorCode::InvalidPolicy))?;
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = self
            .inner
            .prepare_policy_changes(&edits, id(operation)?)
            .map_err(from_error)?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
    pub fn snapshot_format(&self, handle: u32) -> Result<String, JsValue> {
        serde_json::to_string(&self.plan(handle)?.snapshot_format().map_err(from_error)?)
            .map_err(|_| failure(ErrorCode::InvalidPolicy))
    }
    pub fn policy_preview(&self, handle: u32) -> Result<String, JsValue> {
        serde_json::to_string(&self.plan(handle)?.policy_preview().map_err(from_error)?)
            .map_err(|_| failure(ErrorCode::InvalidPolicy))
    }
    pub fn recall(&self, text: &str) -> Result<String, JsValue> {
        let result = self.inner.recall(text).map_err(from_error)?;
        let findings: Vec<_> = result.findings.iter().map(|span| {
            let finding = &span.finding;
            json!({
                "finding": {
                    "entityType": finding.entity_type,
                    "matchedText": finding.matched_text,
                    "range": { "unit": "utf8_bytes", "start": finding.byte_range.start, "end": finding.byte_range.end },
                    "source": finding.source.as_str(),
                    "needsReview": finding.needs_review,
                },
                "byteRange": super::TextRange::from(finding.byte_range),
                "codepointRange": super::TextRange::from(span.codepoint_range),
                "utf16Range": super::TextRange::from(span.utf16_range),
            })
        }).collect();
        Ok(
            json!({ "head": head_value(result.head), "complete": true, "findings": findings })
                .to_string(),
        )
    }
    pub fn review(&self, input_json: &str) -> Result<String, JsValue> {
        if input_json.len() > workspace::MAX_BUNDLE_BYTES {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let data: ReviewInputData =
            serde_json::from_str(input_json).map_err(|_| failure(ErrorCode::InvalidFinding))?;
        let input = adapt_review(data).map_err(from_error)?;
        let value =
            review_value(self.inner.review(input.as_input()).map_err(from_error)?).to_string();
        if value.len() > workspace::MAX_BUNDLE_BYTES {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        Ok(value)
    }
    pub fn prepare_review(&mut self, inputs_json: &str, operation: &[u8]) -> Result<u32, JsValue> {
        if inputs_json.len() > workspace::MAX_BUNDLE_BYTES || self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let data: Vec<ReviewInputData> =
            serde_json::from_str(inputs_json).map_err(|_| failure(ErrorCode::InvalidFinding))?;
        if data.len() > workspace::MAX_INPUTS {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let mut inputs = Vec::with_capacity(data.len());
        for (input_index, input) in data.into_iter().enumerate() {
            inputs.push(adapt_review(input).map_err(|mut error| {
                error.input_index = Some(input_index);
                from_error(error)
            })?);
        }
        let borrowed: Vec<_> = inputs.iter().map(AdaptedReview::as_input).collect();
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = self
            .inner
            .prepare_review(&borrowed, id(operation)?)
            .map_err(from_error)?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
    pub fn prepare_recovery(
        &mut self,
        operation_id: &[u8],
        context: Option<String>,
    ) -> Result<u32, JsValue> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let plan = if let Some(context) = context {
            let parsed = operation(&context)?;
            if parsed.id != id(operation_id)? {
                return Err(failure(ErrorCode::InvalidOperation));
            }
            self.inner.prepare_recovery_with_operation(parsed)
        } else {
            self.inner.prepare_recovery(id(operation_id)?)
        }
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
    pub fn prepare_batch(
        &mut self,
        inputs_json: &str,
        operation_id: &[u8],
        context: Option<String>,
    ) -> Result<u32, JsValue> {
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
                    review_conflicts: Vec::new(),
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
        let plan = if let Some(context) = context {
            let parsed = operation(&context)?;
            if parsed.id != id(operation_id)? {
                return Err(failure(ErrorCode::InvalidOperation));
            }
            self.inner.prepare_batch_with_operation(&borrowed, parsed)
        } else {
            self.inner.prepare_batch(&borrowed, id(operation_id)?)
        }
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
        // Check every retained parent before activation, so cleanup cannot fail
        // after the confirmed saved state has been adopted.
        let parents: BTreeMap<u32, Head> = self
            .plans
            .iter()
            .map(|(handle, plan)| plan.expected_head().map(|parent| (*handle, parent)))
            .collect::<Result<_, _>>()
            .map_err(from_error)?;
        let plan = self
            .plans
            .get_mut(&handle)
            .ok_or_else(|| failure(ErrorCode::InactivePlan))?;
        self.inner
            .activate_saved(plan, expected, saved)
            .map_err(from_error)?;
        self.plans
            .retain(|retained, _| *retained != handle && parents.get(retained) == Some(&saved));
        if expected != saved {
            self.streams.close();
        }
        Ok(())
    }
    pub fn buffered_restore(
        &mut self,
        encoding: &str,
    ) -> Result<stream::NativeRestoreStream, JsValue> {
        self.streams.buffered(&self.inner, encoding)
    }
    pub fn progressive_restore(
        &mut self,
        encoding: &str,
    ) -> Result<stream::NativeRestoreStream, JsValue> {
        self.streams.progressive(&self.inner, encoding)
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
        self.streams.close();
    }
}
impl Drop for WasmWorkspace {
    fn drop(&mut self) {
        self.close();
    }
}
impl WasmWorkspace {
    fn retain_plan(&mut self, plan: PreparedChange) -> Result<u32, JsValue> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
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
