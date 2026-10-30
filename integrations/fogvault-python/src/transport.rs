use super::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString};
use serde::de::DeserializeOwned;

pub fn value(py: Python<'_>, input: Value) -> PyResult<Py<PyAny>> {
    Ok(match input {
        Value::Null => py.None(),
        Value::Bool(v) => v.into_pyobject(py)?.to_owned().into_any().unbind(),
        Value::Number(v) => {
            if let Some(v) = v.as_u64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else if let Some(v) = v.as_i64() {
                v.into_pyobject(py)?.into_any().unbind()
            } else {
                v.as_f64()
                    .ok_or_else(|| fail(py, "invalid_input"))?
                    .into_pyobject(py)?
                    .into_any()
                    .unbind()
            }
        }
        Value::String(v) => PyString::new(py, &v).into_any().unbind(),
        Value::Array(v) => PyList::new(
            py,
            v.into_iter()
                .map(|v| value(py, v))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_any()
        .unbind(),
        Value::Object(v) => {
            let out = PyDict::new(py);
            for (key, v) in v {
                out.set_item(key, value(py, v)?)?;
            }
            out.into_any().unbind()
        }
    })
}
pub fn object(py: Python<'_>, v: &impl serde::Serialize) -> PyResult<Py<PyAny>> {
    value(
        py,
        serde_json::to_value(v).map_err(|_| fail(py, "internal_error"))?,
    )
}
fn json_input(
    py: Python<'_>,
    v: &Bound<'_, PyAny>,
    depth: usize,
    budget: &mut usize,
    nodes: &mut usize,
) -> PyResult<Value> {
    if depth > 32 || *budget == 0 || *nodes == 0 {
        return Err(fail(py, "resource_limit"));
    }
    *budget -= 1;
    *nodes -= 1;
    if v.is_none() {
        return Ok(Value::Null);
    }
    if v.is_exact_instance_of::<PyBool>() {
        return Ok(Value::Bool(
            v.extract::<bool>().map_err(|_| fail(py, "invalid_input"))?,
        ));
    }
    if v.is_exact_instance_of::<PyInt>() {
        if let Ok(n) = v.extract::<u64>() {
            return Ok(n.into());
        }
        return v
            .extract::<i64>()
            .map(Value::from)
            .map_err(|_| fail(py, "invalid_input"));
    }
    if v.is_exact_instance_of::<PyFloat>() {
        return serde_json::Number::from_f64(
            v.extract::<f64>().map_err(|_| fail(py, "invalid_input"))?,
        )
        .map(Value::Number)
        .ok_or_else(|| fail(py, "invalid_input"));
    }
    if v.is_exact_instance_of::<PyString>() {
        let text = v
            .extract::<String>()
            .map_err(|_| fail(py, "invalid_input"))?;
        if text.len() > *budget {
            return Err(fail(py, "resource_limit"));
        }
        *budget -= text.len();
        return Ok(Value::String(text));
    }
    if v.is_exact_instance_of::<PyBytes>() {
        let data = v
            .cast::<PyBytes>()
            .map_err(|_| fail(py, "invalid_input"))?
            .as_bytes();
        // Bytes in dictionary transport represent fixed public IDs only. Raw
        // bundles and stream chunks use the separate bounded bytes transport.
        if data.len() != 16 {
            return Err(fail(py, "invalid_identity"));
        }
        if data.len() > *budget {
            return Err(fail(py, "resource_limit"));
        }
        *budget -= data.len();
        return Ok(Value::Array(data.iter().map(|n| Value::from(*n)).collect()));
    }
    if v.is_exact_instance_of::<PyList>() {
        let list = v.cast::<PyList>().map_err(|_| fail(py, "invalid_input"))?;
        if list.len() > 100_000 || list.len() > *budget {
            return Err(fail(py, "resource_limit"));
        }
        return list
            .iter()
            .map(|x| json_input(py, &x, depth + 1, budget, nodes))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }
    if v.is_exact_instance_of::<PyDict>() {
        let dict = v.cast::<PyDict>().map_err(|_| fail(py, "invalid_input"))?;
        if dict.len() > 100 {
            return Err(fail(py, "resource_limit"));
        }
        let mut out = serde_json::Map::new();
        for (key, v) in dict.iter() {
            if !key.is_exact_instance_of::<PyString>() {
                return Err(fail(py, "invalid_input"));
            }
            let key = key
                .extract::<String>()
                .map_err(|_| fail(py, "invalid_input"))?;
            if key.len() > *budget {
                return Err(fail(py, "resource_limit"));
            }
            *budget -= key.len();
            out.insert(key, json_input(py, &v, depth + 1, budget, nodes)?);
        }
        return Ok(Value::Object(out));
    }
    Err(fail(py, "invalid_input"))
}
pub fn decode<T: DeserializeOwned>(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<T> {
    serde_json::from_value(json_input(
        py,
        v,
        0,
        &mut (16 * 1024 * 1024),
        &mut 1_000_000,
    )?)
    .map_err(|_| fail(py, "invalid_input"))
}
pub fn bytes<'a>(py: Python<'_>, v: &'a Bound<'_, PyAny>, max: usize) -> PyResult<&'a [u8]> {
    if !v.is_exact_instance_of::<PyBytes>() {
        return Err(fail(py, "invalid_input"));
    }
    let bytes = v
        .cast::<PyBytes>()
        .map_err(|_| fail(py, "invalid_input"))?
        .as_bytes();
    if bytes.len() > max {
        return Err(fail(py, "resource_limit"));
    }
    Ok(bytes)
}
pub fn id(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<core::PublicId> {
    let result: core::PublicId = bytes(py, v, 16)?
        .try_into()
        .map_err(|_| fail(py, "invalid_identity"))?;
    if result == [0; 16] {
        return Err(fail(py, "invalid_identity"));
    }
    Ok(result)
}
pub fn text(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<String> {
    if !v.is_exact_instance_of::<PyString>() {
        return Err(fail(py, "invalid_input"));
    }
    let value = v
        .extract::<String>()
        .map_err(|_| fail(py, "invalid_input"))?;
    if value.len() > core::MAX_BUNDLE_BYTES {
        return Err(fail(py, "resource_limit"));
    }
    Ok(value)
}
pub fn approved(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<()> {
    if !v.is_exact_instance_of::<PyBool>() || !v.extract::<bool>().unwrap_or(false) {
        return Err(fail(py, "approval_required"));
    }
    Ok(())
}
pub fn previews(py: Python<'_>, items: &[core::Preview]) -> PyResult<Py<PyAny>> {
    value(py,Value::Array(items.iter().map(|p|json!({"text":p.text,"transformations":p.transformations.iter().map(|t|json!({"source":t.source.as_str(),"needsReview":t.needs_review,"sourceByteRange":t.source_byte_range,"sourceCodepointRange":t.source_codepoint_range,"sourceUtf16Range":t.source_utf16_range,"outputByteRange":t.output_byte_range,"outputCodepointRange":t.output_codepoint_range,"outputUtf16Range":t.output_utf16_range})).collect::<Vec<_>>()})).collect()))
}
pub fn report(py: Python<'_>, r: &core::RestoreReport) -> PyResult<Py<PyAny>> {
    value(
        py,
        json!({"text":r.text,"restoredMarkers":r.restored_markers,"issues":r.issues.iter().map(|i|json!({"kind":i.kind.as_str(),"byteRange":i.byte_range,"codepointRange":i.codepoint_range,"utf16Range":i.utf16_range})).collect::<Vec<_>>()}),
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    entity_type: String,
    matched_text: String,
    range: Range,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    detector_name: Option<String>,
    #[serde(default)]
    detector_version: Option<String>,
    #[serde(default)]
    source: core::FindingSource,
    #[serde(default = "yes")]
    needs_review: bool,
}
fn yes() -> bool {
    true
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Range {
    unit: String,
    start: usize,
    end: usize,
}
impl Finding {
    pub fn into_core(self, py: Python<'_>) -> PyResult<core::WorkspaceFinding> {
        if self.range.unit != "utf8_bytes" {
            return Err(fail(py, "invalid_input"));
        }
        Ok(core::WorkspaceFinding {
            entity_type: self.entity_type,
            matched_text: self.matched_text,
            byte_range: datafog_core::TextRange {
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub text: String,
    #[serde(default)]
    scanner_findings: Vec<Finding>,
    #[serde(default)]
    user_selections: Vec<Finding>,
    #[serde(default)]
    corrections: Vec<Correction>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Correction {
    candidate_key: String,
    replacement: Finding,
}
pub struct OwnedReview {
    text: String,
    scanner: Vec<core::WorkspaceFinding>,
    user: Vec<core::WorkspaceFinding>,
    corrections: Vec<core::ReviewCorrection>,
}
impl Review {
    pub fn convert(self, py: Python<'_>) -> PyResult<OwnedReview> {
        Ok(OwnedReview {
            text: self.text,
            scanner: self
                .scanner_findings
                .into_iter()
                .map(|x| x.into_core(py))
                .collect::<PyResult<_>>()?,
            user: self
                .user_selections
                .into_iter()
                .map(|x| x.into_core(py))
                .collect::<PyResult<_>>()?,
            corrections: self
                .corrections
                .into_iter()
                .map(|x| {
                    Ok(core::ReviewCorrection {
                        candidate_key: x.candidate_key,
                        replacement: x.replacement.into_core(py)?,
                    })
                })
                .collect::<PyResult<_>>()?,
        })
    }
}
impl OwnedReview {
    pub fn borrowed(&self) -> core::ReviewInput<'_> {
        core::ReviewInput {
            text: &self.text,
            scanner_findings: &self.scanner,
            user_selections: &self.user,
            corrections: &self.corrections,
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conversation {
    id: [u8; 16],
    surface: String,
    site_id: Option<String>,
    site_id_byte_budget: usize,
}
impl Conversation {
    pub fn into_core(self) -> core::Conversation {
        core::Conversation {
            id: self.id,
            surface: self.surface,
            site_id: self.site_id,
            site_id_byte_budget: self.site_id_byte_budget,
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationInput {
    pub input: Review,
    pub conversation: Conversation,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    id: [u8; 16],
    occurred_at_ms: u64,
}
impl Operation {
    pub fn into_core(self) -> core::OperationContext {
        core::OperationContext {
            id: self.id,
            occurred_at_ms: self.occurred_at_ms,
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intent {
    preparation_id: [u8; 16],
    input_index: usize,
}
impl Intent {
    pub fn into_core(self) -> core::SendIntentId {
        core::SendIntentId {
            preparation_id: self.preparation_id,
            input_index: self.input_index,
        }
    }
}

pub fn finding_value(f: &core::WorkspaceFinding) -> Value {
    json!({"entityType":f.entity_type,"matchedText":f.matched_text,"range":{"unit":"utf8_bytes","start":f.byte_range.start,"end":f.byte_range.end},"confidence":f.confidence,"detectorName":f.detector_name,"detectorVersion":f.detector_version,"source":f.source.as_str(),"needsReview":f.needs_review})
}
pub fn review(py: Python<'_>, r: core::ReviewResult) -> PyResult<Py<PyAny>> {
    value(
        py,
        json!({"head":{"revision":r.head.revision,"commitId":r.head.commit_id},"candidates":r.candidates.iter().map(|c|json!({"key":c.key,"finding":finding_value(&c.finding),"sources":c.sources.iter().map(|s|s.as_str()).collect::<Vec<_>>(),"byteRange":c.finding.byte_range,"codepointRange":c.codepoint_range,"utf16Range":c.utf16_range,"eligible":c.eligible,"selected":c.selected})).collect::<Vec<_>>(),"conflicts":r.conflicts}),
    )
}
pub fn recall(py: Python<'_>, r: core::RecallResult) -> PyResult<Py<PyAny>> {
    value(
        py,
        json!({"head":{"revision":r.head.revision,"commitId":r.head.commit_id},"findings":r.findings.iter().map(|f|json!({"finding":finding_value(&f.finding),"byteRange":f.finding.byte_range,"codepointRange":f.codepoint_range,"utf16Range":f.utf16_range})).collect::<Vec<_>>()}),
    )
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchInput {
    pub text: String,
    pub findings: Vec<Finding>,
}
