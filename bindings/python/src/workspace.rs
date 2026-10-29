//! Thin ownership and value adapters around the shared Core workspace.
use super::*;
use ::datafog_core::workspace as w;
use pyo3::types::PyBytes;
use serde_json::{Value, json};
mod conversation;
mod operations;
mod stream;

create_exception!(datafog_core, DataFogWorkspaceError, PyValueError);

fn json_object(py: Python<'_>, value: &impl serde::Serialize) -> PyResult<Py<PyAny>> {
    let value = serde_json::to_value(value)
        .map_err(|_| PyRuntimeError::new_err("workspace result serialization failed"))?;
    json_value(py, value)
}
fn json_value(py: Python<'_>, value: Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(value) => Ok(value.into_pyobject(py)?.to_owned().into_any().unbind()),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else if let Some(value) = value.as_u64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else if let Some(value) = value.as_f64() {
                Ok(value.into_pyobject(py)?.into_any().unbind())
            } else {
                Err(PyRuntimeError::new_err(
                    "workspace result number is invalid",
                ))
            }
        }
        Value::String(value) => Ok(pyo3::types::PyString::new(py, &value).into_any().unbind()),
        Value::Array(values) => {
            let values = values
                .into_iter()
                .map(|value| json_value(py, value))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, values)?.into_any().unbind())
        }
        Value::Object(values) => {
            let result = PyDict::new(py);
            for (key, value) in values {
                result.set_item(key, json_value(py, value)?)?;
            }
            Ok(result.into_any().unbind())
        }
    }
}
fn issue_value(issue: &w::MarkerIssue) -> Value {
    json!({"kind":issue.kind.as_str(),"byteRange":issue.byte_range,
        "codepointRange":issue.codepoint_range,"utf16Range":issue.utf16_range})
}
fn workspace_error(py: Python<'_>, error: w::WorkspaceError) -> PyErr {
    workspace_exception(
        py,
        PyErr::new::<DataFogWorkspaceError, _>(error.code.as_str()),
        error,
    )
}
fn workspace_exception(py: Python<'_>, exception: PyErr, error: w::WorkspaceError) -> PyErr {
    let attach = || -> PyResult<()> {
        let value = exception.value(py);
        value.setattr("code", error.code.as_str())?;
        value.setattr("finding_index", error.finding_index)?;
        value.setattr("input_index", error.input_index)?;
        value.setattr(
            "marker_issues",
            json_object(
                py,
                &error
                    .marker_issues
                    .iter()
                    .map(issue_value)
                    .collect::<Vec<_>>(),
            )?,
        )?;
        value.setattr(
            "review_conflicts",
            json_object(py, &error.review_conflicts)?,
        )?;
        Ok(())
    };
    match attach() {
        Ok(()) => exception,
        Err(error) => error,
    }
}
fn core_error(code: w::ErrorCode) -> w::WorkspaceError {
    w::WorkspaceError {
        code,
        finding_index: None,
        input_index: None,
        marker_issues: vec![],
        review_conflicts: vec![],
    }
}
fn failure(py: Python<'_>, code: w::ErrorCode) -> PyErr {
    workspace_error(py, core_error(code))
}

fn id(py: Python<'_>, bytes: &[u8]) -> PyResult<w::PublicId> {
    bytes
        .try_into()
        .map_err(|_| failure(py, w::ErrorCode::InvalidId))
}
fn head_value(head: w::Head) -> Value {
    json!({"revision":head.revision,"commitId":head.commit_id})
}
fn source(py: Python<'_>, value: &str) -> PyResult<w::FindingSource> {
    match value {
        "scanner" => Ok(w::FindingSource::Scanner),
        "user" => Ok(w::FindingSource::User),
        "recall" => Ok(w::FindingSource::Recall),
        _ => Err(failure(py, w::ErrorCode::InvalidFinding)),
    }
}
fn finding_value(f: &w::WorkspaceFinding) -> Value {
    json!({"entityType":f.entity_type,"matchedText":f.matched_text,
        "range":{"unit":"utf8_bytes","start":f.byte_range.start,"end":f.byte_range.end},
        "confidence":f.confidence,"detectorName":f.detector_name,"detectorVersion":f.detector_version,
        "source":f.source.as_str(),"needsReview":f.needs_review})
}
fn preview_value(value: &w::Preview) -> Value {
    let transformations: Vec<_> = value
        .transformations
        .iter()
        .map(|t| {
            json!({
        "source":t.source.as_str(),"needsReview":t.needs_review,
        "sourceByteRange":t.source_byte_range,"sourceCodepointRange":t.source_codepoint_range,
        "sourceUtf16Range":t.source_utf16_range,"outputByteRange":t.output_byte_range,
        "outputCodepointRange":t.output_codepoint_range,"outputUtf16Range":t.output_utf16_range})
        })
        .collect();
    json!({"text":value.text,"transformations":transformations})
}
fn review_value(value: w::ReviewResult) -> Value {
    let candidates:Vec<_>=value.candidates.into_iter().map(|c|json!({
        "key":c.key,"finding":finding_value(&c.finding),
        "sources":c.sources.iter().map(|s|s.as_str()).collect::<Vec<_>>(),
        "byteRange":c.finding.byte_range,"codepointRange":c.codepoint_range,"utf16Range":c.utf16_range,
        "eligible":c.eligible,"selected":c.selected})).collect();
    json!({"head":head_value(value.head),"candidates":candidates,"conflicts":value.conflicts})
}

#[pyclass(frozen, skip_from_py_object, name = "WorkspaceHead")]
#[derive(Clone)]
pub(super) struct WorkspaceHead {
    inner: w::Head,
}
#[pymethods]
impl WorkspaceHead {
    #[new]
    fn new(py: Python<'_>, revision: u64, commit_id: &[u8]) -> PyResult<Self> {
        Ok(Self {
            inner: w::Head {
                revision,
                commit_id: id(py, commit_id)?,
            },
        })
    }
    #[getter]
    fn revision(&self) -> u64 {
        self.inner.revision
    }
    #[getter]
    fn commit_id<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.commit_id)
    }
    fn __eq__(&self, other: PyRef<'_, Self>) -> bool {
        self.inner == other.inner
    }
    fn __repr__(&self) -> String {
        format!("WorkspaceHead(revision={})", self.inner.revision)
    }
}

#[pyclass(frozen, skip_from_py_object, name = "WorkspaceFinding")]
#[derive(Clone)]
pub(super) struct WorkspaceFinding {
    inner: w::WorkspaceFinding,
}
#[pymethods]
impl WorkspaceFinding {
    #[new]
    #[pyo3(signature=(entity_type,matched_text,byte_range,*,confidence=None,detector_name=None,detector_version=None,source="scanner",needs_review=true))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        entity_type: String,
        matched_text: String,
        byte_range: PyRef<'_, TextRange>,
        confidence: Option<f32>,
        detector_name: Option<String>,
        detector_version: Option<String>,
        source: &str,
        needs_review: bool,
    ) -> PyResult<Self> {
        if matched_text.len() > w::MAX_SOURCE_BYTES {
            return Err(failure(py, w::ErrorCode::ResourceLimit));
        }
        Ok(Self {
            inner: w::WorkspaceFinding {
                entity_type,
                matched_text,
                byte_range: core::TextRange {
                    start: byte_range.start,
                    end: byte_range.end,
                },
                confidence,
                detector_name,
                detector_version,
                source: self::source(py, source)?,
                needs_review,
            },
        })
    }
    #[staticmethod]
    fn from_finding(finding: PyRef<'_, Finding>) -> Self {
        Self {
            inner: w::WorkspaceFinding {
                entity_type: finding.entity_type.clone(),
                matched_text: finding.matched_text.clone(),
                byte_range: core::TextRange {
                    start: finding.byte_range.start,
                    end: finding.byte_range.end,
                },
                confidence: finding.confidence,
                detector_name: Some(finding.detector_name.clone()),
                detector_version: finding.detector_version.clone(),
                source: w::FindingSource::Scanner,
                needs_review: w::DEFAULT_NEEDS_REVIEW,
            },
        }
    }
    #[getter]
    fn entity_type(&self) -> String {
        self.inner.entity_type.clone()
    }
    #[getter]
    fn matched_text(&self) -> String {
        self.inner.matched_text.clone()
    }
    #[getter]
    fn byte_range(&self) -> TextRange {
        self.inner.byte_range.into()
    }
    #[getter]
    fn confidence(&self) -> Option<f32> {
        self.inner.confidence
    }
    #[getter]
    fn detector_name(&self) -> Option<String> {
        self.inner.detector_name.clone()
    }
    #[getter]
    fn detector_version(&self) -> Option<String> {
        self.inner.detector_version.clone()
    }
    #[getter]
    fn source(&self) -> &'static str {
        self.inner.source.as_str()
    }
    #[getter]
    fn needs_review(&self) -> bool {
        self.inner.needs_review
    }
}
fn finding_bytes(f: &w::WorkspaceFinding) -> usize {
    f.matched_text
        .len()
        .saturating_add(f.entity_type.len())
        .saturating_add(f.detector_name.as_ref().map_or(0, String::len))
        .saturating_add(f.detector_version.as_ref().map_or(0, String::len))
}
fn material_limit(py: Python<'_>, bytes: usize) -> PyResult<()> {
    if bytes > w::MAX_BUNDLE_BYTES {
        Err(failure(py, w::ErrorCode::ResourceLimit))
    } else {
        Ok(())
    }
}
fn findings(
    py: Python<'_>,
    items: Vec<Py<WorkspaceFinding>>,
) -> PyResult<Vec<w::WorkspaceFinding>> {
    if items.len() > w::MAX_FINDINGS {
        return Err(failure(py, w::ErrorCode::ResourceLimit));
    }
    let mut bytes = 0usize;
    for item in &items {
        bytes = bytes.saturating_add(finding_bytes(&item.borrow(py).inner));
        material_limit(py, bytes)?;
    }
    Ok(items.iter().map(|f| f.borrow(py).inner.clone()).collect())
}

#[pyclass(frozen, skip_from_py_object, name = "WorkspaceInput")]
#[derive(Clone)]
pub(super) struct WorkspaceInput {
    text: String,
    findings: Vec<w::WorkspaceFinding>,
}
#[pymethods]
impl WorkspaceInput {
    #[new]
    fn new(py: Python<'_>, text: String, findings: Vec<Py<WorkspaceFinding>>) -> PyResult<Self> {
        if text.len() > w::MAX_SOURCE_BYTES {
            return Err(failure(py, w::ErrorCode::ResourceLimit));
        }
        Ok(Self {
            text,
            findings: self::findings(py, findings)?,
        })
    }
}

fn workspace_inputs(
    py: Python<'_>,
    inputs: Vec<Py<WorkspaceInput>>,
) -> PyResult<Vec<WorkspaceInput>> {
    if inputs.len() > w::MAX_INPUTS {
        return Err(failure(py, w::ErrorCode::ResourceLimit));
    }
    let mut bytes = 0usize;
    for item in &inputs {
        let item = item.borrow(py);
        bytes = bytes.saturating_add(item.text.len());
        for f in &item.findings {
            bytes = bytes.saturating_add(finding_bytes(f));
        }
        material_limit(py, bytes)?;
    }
    let inputs: Vec<_> = inputs
        .iter()
        .map(|input| input.borrow(py).clone())
        .collect();
    Ok(inputs)
}

#[pyclass(frozen, skip_from_py_object, name = "WorkspaceReviewCorrection")]
#[derive(Clone)]
pub(super) struct WorkspaceReviewCorrection {
    candidate_key: String,
    replacement: w::WorkspaceFinding,
}
#[pymethods]
impl WorkspaceReviewCorrection {
    #[new]
    fn new(candidate_key: String, replacement: PyRef<'_, WorkspaceFinding>) -> Self {
        Self {
            candidate_key,
            replacement: replacement.inner.clone(),
        }
    }
}
struct ReviewOwned {
    text: String,
    scanner: Vec<w::WorkspaceFinding>,
    user: Vec<w::WorkspaceFinding>,
    corrections: Vec<w::ReviewCorrection>,
}
impl ReviewOwned {
    fn as_input(&self) -> w::ReviewInput<'_> {
        w::ReviewInput {
            text: &self.text,
            scanner_findings: &self.scanner,
            user_selections: &self.user,
            corrections: &self.corrections,
        }
    }
}
#[pyclass(frozen, skip_from_py_object, name = "WorkspaceReviewInput")]
#[derive(Clone)]
pub(super) struct WorkspaceReviewInput {
    text: String,
    scanner: Vec<w::WorkspaceFinding>,
    user: Vec<w::WorkspaceFinding>,
    corrections: Vec<WorkspaceReviewCorrection>,
}
impl WorkspaceReviewInput {
    fn owned(&self) -> ReviewOwned {
        ReviewOwned {
            text: self.text.clone(),
            scanner: self.scanner.clone(),
            user: self.user.clone(),
            corrections: self
                .corrections
                .iter()
                .map(|c| w::ReviewCorrection {
                    candidate_key: c.candidate_key.clone(),
                    replacement: c.replacement.clone(),
                })
                .collect(),
        }
    }
}
#[pymethods]
impl WorkspaceReviewInput {
    #[new]
    #[pyo3(signature=(text,*,scanner_findings=None,user_selections=None,corrections=None))]
    fn new(
        py: Python<'_>,
        text: String,
        scanner_findings: Option<Vec<Py<WorkspaceFinding>>>,
        user_selections: Option<Vec<Py<WorkspaceFinding>>>,
        corrections: Option<Vec<Py<WorkspaceReviewCorrection>>>,
    ) -> PyResult<Self> {
        let scanner = scanner_findings.unwrap_or_default();
        let user = user_selections.unwrap_or_default();
        let corrections = corrections.unwrap_or_default();
        if text.len() > w::MAX_SOURCE_BYTES
            || scanner
                .len()
                .saturating_add(user.len())
                .saturating_add(corrections.len())
                > w::MAX_FINDINGS
        {
            return Err(failure(py, w::ErrorCode::ResourceLimit));
        }
        let mut bytes = text.len();
        for item in scanner.iter().chain(&user) {
            bytes = bytes.saturating_add(finding_bytes(&item.borrow(py).inner));
            material_limit(py, bytes)?;
        }
        for item in &corrections {
            let item = item.borrow(py);
            bytes = bytes
                .saturating_add(finding_bytes(&item.replacement))
                .saturating_add(item.candidate_key.len());
            material_limit(py, bytes)?;
        }
        Ok(Self {
            text,
            scanner: findings(py, scanner)?,
            user: findings(py, user)?,
            corrections: corrections.iter().map(|c| c.borrow(py).clone()).collect(),
        })
    }
}

type PlanCell = std::sync::Arc<std::sync::Mutex<w::PreparedChange>>;
type WeakPlan = std::sync::Weak<std::sync::Mutex<w::PreparedChange>>;
const MAX_LIVE_PLANS: usize = 32;
enum NativeError {
    Core(w::WorkspaceError),
    Unavailable,
}
type NativeResult<T> = std::result::Result<T, NativeError>;
impl From<w::WorkspaceError> for NativeError {
    fn from(error: w::WorkspaceError) -> Self {
        Self::Core(error)
    }
}
impl NativeError {
    fn into_python(self, py: Python<'_>) -> PyErr {
        match self {
            Self::Core(error) => workspace_error(py, error),
            Self::Unavailable => PyRuntimeError::new_err("workspace handle is unavailable"),
        }
    }
}
fn lock<T>(value: &std::sync::Mutex<T>) -> NativeResult<std::sync::MutexGuard<'_, T>> {
    value.lock().map_err(|_| NativeError::Unavailable)
}
#[pyclass(name = "WorkspacePreparedChange")]
pub(super) struct WorkspacePreparedChange {
    inner: PlanCell,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl WorkspacePreparedChange {
    fn read<T>(
        &self,
        py: Python<'_>,
        call: impl FnOnce(&w::PreparedChange) -> std::result::Result<T, w::WorkspaceError>,
    ) -> PyResult<T> {
        // Python error normalization may execute user callbacks. Every native
        // guard must drop before constructing exceptions or converting results.
        let result = (|| -> NativeResult<T> {
            if self.closed.load(std::sync::atomic::Ordering::Acquire) {
                return Err(core_error(w::ErrorCode::ClosedWorkspace).into());
            }
            let plan = lock(&self.inner)?;
            call(&plan).map_err(NativeError::from)
        })();
        result.map_err(|error| error.into_python(py))
    }
}
#[pymethods]
impl WorkspacePreparedChange {
    fn preview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self.read(py, |p| p.preview().map(preview_value))?;
        json_object(py, &value)
    }
    fn previews(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let values = self.read(py, |p| {
            p.previews()
                .map(|rows| rows.iter().map(preview_value).collect::<Vec<_>>())
        })?;
        json_object(py, &values)
    }
    fn expected_head(&self, py: Python<'_>) -> PyResult<WorkspaceHead> {
        self.read(py, w::PreparedChange::expected_head)
            .map(|inner| WorkspaceHead { inner })
    }
    fn candidate_head(&self, py: Python<'_>) -> PyResult<WorkspaceHead> {
        self.read(py, w::PreparedChange::candidate_head)
            .map(|inner| WorkspaceHead { inner })
    }
    fn snapshot_format(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self.read(py, w::PreparedChange::snapshot_format)?;
        json_object(py, &value)
    }
    /// Saved preparation identifiers are not proof of an actual transport send.
    fn send_intents(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let values = self.read(py, w::PreparedChange::send_intents)?;
        json_object(py, &values)
    }
    fn policy_preview(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self.read(py, |p| {
            p.policy_preview().and_then(|v| {
                serde_json::to_value(v).map_err(|_| w::WorkspaceError {
                    code: w::ErrorCode::InvalidPolicy,
                    finding_index: None,
                    input_index: None,
                    marker_issues: vec![],
                    review_conflicts: vec![],
                })
            })
        })?;
        json_object(py, &value)
    }
    fn export_candidate_bundle<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let mut value = self.read(py, |p| p.export_candidate_bundle().map(<[u8]>::to_vec))?;
        let result = PyBytes::new(py, &value);
        value.fill(0);
        Ok(result)
    }
    fn discard(&self, py: Python<'_>) -> PyResult<()> {
        let result = (|| -> NativeResult<()> {
            if self.closed.load(std::sync::atomic::Ordering::Acquire) {
                return Err(core_error(w::ErrorCode::ClosedWorkspace).into());
            }
            let mut plan = lock(&self.inner)?;
            plan.expected_head()?;
            plan.discard();
            Ok(())
        })();
        result.map_err(|error| error.into_python(py))
    }
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.read(slf.py(), w::PreparedChange::expected_head)?;
        Ok(slf)
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        _kind: &Bound<'_, PyAny>,
        _value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let result = (|| -> NativeResult<()> {
            lock(&self.inner)?.discard();
            Ok(())
        })();
        result
            .map(|()| false)
            .map_err(|error| error.into_python(py))
    }
}

#[pyclass(name = "Workspace")]
pub(super) struct Workspace {
    inner: w::Workspace,
    plans: std::sync::Mutex<Vec<WeakPlan>>,
    streams: std::sync::Mutex<stream::Registry>,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[pymethods]
impl Workspace {
    #[staticmethod]
    fn create(py: Python<'_>, namespace: &[u8], operation_id: &[u8]) -> PyResult<Self> {
        w::Workspace::create(id(py, namespace)?, id(py, operation_id)?)
            .map(|inner| Self {
                inner,
                plans: std::sync::Mutex::new(Vec::new()),
                streams: std::sync::Mutex::new(stream::Registry::default()),
                closed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .map_err(|e| workspace_error(py, e))
    }
    #[staticmethod]
    fn create_with_operation(
        py: Python<'_>,
        namespace: &[u8],
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let operation = operations::context(py, operation)?;
        w::Workspace::create_with_operation(id(py, namespace)?, operation)
            .map(|inner| Self {
                inner,
                plans: std::sync::Mutex::new(Vec::new()),
                streams: std::sync::Mutex::new(stream::Registry::default()),
                closed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .map_err(|error| workspace_error(py, error))
    }
    #[staticmethod]
    fn open_bundle(py: Python<'_>, bundle: &[u8]) -> PyResult<Self> {
        w::Workspace::open_bundle(bundle)
            .map(|inner| Self {
                inner,
                plans: std::sync::Mutex::new(Vec::new()),
                streams: std::sync::Mutex::new(stream::Registry::default()),
                closed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .map_err(|e| workspace_error(py, e))
    }
    fn export_bundle<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self
            .inner
            .export_bundle()
            .map_err(|e| workspace_error(py, e))?;
        Ok(PyBytes::new(py, &bytes))
    }
    fn head(&self, py: Python<'_>) -> PyResult<WorkspaceHead> {
        self.inner
            .head()
            .map(|inner| WorkspaceHead { inner })
            .map_err(|e| workspace_error(py, e))
    }
    fn namespace<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(
            py,
            &self.inner.namespace().map_err(|e| workspace_error(py, e))?,
        ))
    }
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        let result = (|| -> NativeResult<()> {
            self.closed
                .store(true, std::sync::atomic::Ordering::Release);
            self.inner.close();
            lock(&self.streams)?.close()?;
            for cell in lock(&self.plans)?.drain(..).filter_map(|p| p.upgrade()) {
                lock(&cell)?.discard();
            }
            Ok(())
        })();
        result.map_err(|error| error.into_python(py))
    }
    fn prepare(
        &self,
        py: Python<'_>,
        text: &str,
        findings: Vec<Py<WorkspaceFinding>>,
        operation_id: &[u8],
    ) -> PyResult<WorkspacePreparedChange> {
        let findings = self::findings(py, findings)?;
        let operation = id(py, operation_id)?;
        self.stage(py, || self.inner.prepare(text, &findings, operation))
    }
    fn prepare_batch(
        &self,
        py: Python<'_>,
        inputs: Vec<Py<WorkspaceInput>>,
        operation_id: &[u8],
    ) -> PyResult<WorkspacePreparedChange> {
        let inputs = workspace_inputs(py, inputs)?;
        let borrowed: Vec<_> = inputs
            .iter()
            .map(|input| w::WorkspaceInput {
                text: &input.text,
                findings: &input.findings,
            })
            .collect();
        let operation = id(py, operation_id)?;
        self.stage(py, || self.inner.prepare_batch(&borrowed, operation))
    }
    fn prepare_with_operation(
        &self,
        py: Python<'_>,
        text: &str,
        findings: Vec<Py<WorkspaceFinding>>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = operations::context(py, operation)?;
        let findings = self::findings(py, findings)?;
        self.stage(py, || {
            self.inner
                .prepare_with_operation(text, &findings, operation)
        })
    }
    fn prepare_batch_with_operation(
        &self,
        py: Python<'_>,
        inputs: Vec<Py<WorkspaceInput>>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = operations::context(py, operation)?;
        let inputs = workspace_inputs(py, inputs)?;
        let borrowed: Vec<_> = inputs
            .iter()
            .map(|input| w::WorkspaceInput {
                text: &input.text,
                findings: &input.findings,
            })
            .collect();
        self.stage(py, || {
            self.inner
                .prepare_batch_with_operation(&borrowed, operation)
        })
    }
    /// Stage mappings and pending intentions; explicit approval/save/activation precede transport.
    fn prepare_conversation_send(
        &self,
        py: Python<'_>,
        inputs: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = operations::context(py, operation)?;
        let owned = conversation::inputs(py, inputs)?;
        let borrowed: Vec<_> = owned
            .iter()
            .map(|(input, conversation)| w::ConversationInput {
                input: input.as_input(),
                conversation: conversation.clone(),
            })
            .collect();
        self.stage(py, || {
            self.inner.prepare_conversation_send(&borrowed, operation)
        })
    }
    fn conversation(&self, py: Python<'_>, logical_id: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let value = self
            .inner
            .conversation(conversation::selector(py, logical_id)?)
            .map_err(|error| workspace_error(py, error))?;
        json_object(py, &value)
    }
    fn conversation_for_site<'py>(
        &self,
        py: Python<'py>,
        surface: &Bound<'_, PyAny>,
        site_id: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let surface = conversation::text(py, surface, w::ErrorCode::InvalidConversation)?;
        let site_id = conversation::text(py, site_id, w::ErrorCode::InvalidConversation)?;
        material_limit(py, surface.len().saturating_add(site_id.len()))?;
        let value = self
            .inner
            .conversation_for_site(surface, site_id)
            .map_err(|error| workspace_error(py, error))?;
        Ok(PyBytes::new(py, &value))
    }
    fn send_intent_status(&self, py: Python<'_>, intent: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let value = self
            .inner
            .send_intent_status(conversation::intent(py, intent)?)
            .map_err(|error| workspace_error(py, error))?;
        json_object(py, &value)
    }
    /// Trusted actual successful-send observation; never inferred from model text or a preview.
    fn prepare_observed_send(
        &self,
        py: Python<'_>,
        intent: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
        surface: &Bound<'_, PyAny>,
        site_id: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let surface = conversation::text(py, surface, w::ErrorCode::InvalidConversation)?;
        let site_id = conversation::text(py, site_id, w::ErrorCode::InvalidConversation)?;
        material_limit(py, surface.len().saturating_add(site_id.len()))?;
        let intent = conversation::intent(py, intent)?;
        let logical_id = conversation::selector(py, logical_id)?;
        let operation = operations::context(py, operation)?;
        self.stage(py, || {
            self.inner
                .prepare_observed_send(intent, logical_id, surface, site_id, operation)
        })
    }
    /// Trusted definite non-delivery, not an ambiguous timeout or cancellation.
    fn prepare_definitely_not_sent(
        &self,
        py: Python<'_>,
        intent: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let intent = conversation::intent(py, intent)?;
        let logical_id = conversation::selector(py, logical_id)?;
        let operation = operations::context(py, operation)?;
        self.stage(py, || {
            self.inner
                .prepare_definitely_not_sent(intent, logical_id, operation)
        })
    }
    /// Strict scoped restoration never falls back to current-workspace compatibility mode.
    fn restore_in_conversation(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
    ) -> PyResult<String> {
        let text = conversation::text(py, text, w::ErrorCode::InvalidFinding)?;
        self.inner
            .restore_in_conversation(text, conversation::selector(py, logical_id)?)
            .map_err(|error| workspace_error(py, error))
    }
    fn restore_report_in_conversation(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let text = conversation::text(py, text, w::ErrorCode::InvalidFinding)?;
        let value = self
            .inner
            .restore_report_in_conversation(text, conversation::selector(py, logical_id)?)
            .map_err(|error| workspace_error(py, error))?;
        json_object(
            py,
            &json!({"text":value.text,"restoredMarkers":value.restored_markers,"issues":value.issues.iter().map(issue_value).collect::<Vec<_>>()}),
        )
    }
    #[pyo3(signature = (logical_id, encoding="utf8"))]
    fn buffered_restore_in_conversation(
        &self,
        py: Python<'_>,
        logical_id: &Bound<'_, PyAny>,
        encoding: &str,
    ) -> PyResult<stream::WorkspaceBufferedRestore> {
        let logical_id = conversation::selector(py, logical_id)?;
        let encoding = stream::encoding(py, encoding)?;
        let result = (|| -> NativeResult<_> {
            let value = self
                .inner
                .buffered_restore_in_conversation(logical_id, encoding)?;
            lock(&self.streams)?.buffered(value, encoding)
        })();
        result.map_err(|error| error.into_python(py))
    }
    #[pyo3(signature = (logical_id, encoding="utf8"))]
    fn progressive_restore_in_conversation(
        &self,
        py: Python<'_>,
        logical_id: &Bound<'_, PyAny>,
        encoding: &str,
    ) -> PyResult<stream::WorkspaceProgressiveRestore> {
        let logical_id = conversation::selector(py, logical_id)?;
        let encoding = stream::encoding(py, encoding)?;
        let result = (|| -> NativeResult<_> {
            let value = self
                .inner
                .progressive_restore_in_conversation(logical_id, encoding)?;
            lock(&self.streams)?.progressive(value, encoding)
        })();
        result.map_err(|error| error.into_python(py))
    }
    fn operations(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let records = self
            .inner
            .operations()
            .map_err(|error| workspace_error(py, error))?;
        let values: Vec<_> = records.iter().map(operations::record).collect();
        json_object(py, &values)
    }
    fn operations_page(
        &self,
        py: Python<'_>,
        offset: &Bound<'_, PyAny>,
        limit: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let offset = operations::page_index(py, offset)?;
        let limit = operations::page_index(py, limit)?;
        let page = self
            .inner
            .operations_page(offset, limit)
            .map_err(|error| workspace_error(py, error))?;
        let records: Vec<_> = page.records.iter().map(operations::record).collect();
        json_object(
            py,
            &json!({"records":records,"total":page.total,"offset":page.offset,"nextOffset":page.next_offset}),
        )
    }
    fn counts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let counts = self
            .inner
            .counts()
            .map_err(|error| workspace_error(py, error))?;
        json_object(py, &operations::counts(counts))
    }
    fn history_coverage(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let coverage = self
            .inner
            .history_coverage()
            .map_err(|error| workspace_error(py, error))?;
        json_object(py, &coverage)
    }
    fn prepare_outcome(
        &self,
        py: Python<'_>,
        outcome: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = operations::context(py, operation)?;
        let outcome = operations::outcome(py, outcome)?;
        self.stage(py, || self.inner.prepare_outcome(outcome, operation))
    }
    fn prepare_recovery_with_operation(
        &self,
        py: Python<'_>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = operations::context(py, operation)?;
        self.stage(py, || self.inner.prepare_recovery_with_operation(operation))
    }
    fn review(
        &self,
        py: Python<'_>,
        input: PyRef<'_, WorkspaceReviewInput>,
    ) -> PyResult<Py<PyAny>> {
        let owned = input.owned();
        let value = self
            .inner
            .review(owned.as_input())
            .map_err(|e| workspace_error(py, e))?;
        json_object(py, &review_value(value))
    }
    fn prepare_review(
        &self,
        py: Python<'_>,
        inputs: Vec<Py<WorkspaceReviewInput>>,
        operation_id: &[u8],
    ) -> PyResult<WorkspacePreparedChange> {
        if inputs.len() > w::MAX_INPUTS {
            return Err(failure(py, w::ErrorCode::ResourceLimit));
        }
        let mut bytes = 0usize;
        for item in &inputs {
            let item = item.borrow(py);
            bytes = bytes.saturating_add(item.text.len());
            for f in item.scanner.iter().chain(&item.user) {
                bytes = bytes.saturating_add(finding_bytes(f));
            }
            for c in &item.corrections {
                bytes = bytes
                    .saturating_add(finding_bytes(&c.replacement))
                    .saturating_add(c.candidate_key.len());
            }
            material_limit(py, bytes)?;
        }
        let owned: Vec<_> = inputs
            .iter()
            .map(|input| input.borrow(py).owned())
            .collect();
        let borrowed: Vec<_> = owned.iter().map(ReviewOwned::as_input).collect();
        let operation = id(py, operation_id)?;
        self.stage(py, || self.inner.prepare_review(&borrowed, operation))
    }
    fn prepare_recovery(
        &self,
        py: Python<'_>,
        operation_id: &[u8],
    ) -> PyResult<WorkspacePreparedChange> {
        let operation = id(py, operation_id)?;
        self.stage(py, || self.inner.prepare_recovery(operation))
    }
    fn policy(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_object(
            py,
            &self.inner.policy().map_err(|e| workspace_error(py, e))?,
        )
    }
    fn prepare_policy_changes(
        &self,
        py: Python<'_>,
        edits: &Bound<'_, PyAny>,
        operation_id: &[u8],
    ) -> PyResult<WorkspacePreparedChange> {
        let edits = policy_edits(py, edits)?;
        let operation = id(py, operation_id)?;
        self.stage(py, || self.inner.prepare_policy_changes(&edits, operation))
    }
    fn activate_saved(
        &mut self,
        py: Python<'_>,
        change: PyRef<'_, WorkspacePreparedChange>,
        expected_head: PyRef<'_, WorkspaceHead>,
        saved_head: PyRef<'_, WorkspaceHead>,
    ) -> PyResult<()> {
        let result = (|| -> NativeResult<()> {
            self.inner.head()?;
            let mut registry = lock(&self.plans)?;
            let live: Vec<_> = registry.iter().filter_map(|weak| weak.upgrade()).collect();
            let Some(position) = live
                .iter()
                .position(|cell| std::sync::Arc::ptr_eq(cell, &change.inner))
            else {
                return Err(core_error(w::ErrorCode::WorkspaceConflict).into());
            };
            // Acquire/validate every handle before adoption, then cleanup cannot fail
            // after the host's confirmed saved state has become active.
            let mut plans = live
                .iter()
                .map(|cell| lock(cell))
                .collect::<NativeResult<Vec<_>>>()?;
            let mut parents = Vec::with_capacity(plans.len());
            for plan in &plans {
                parents.push(match plan.expected_head() {
                    Ok(head) => Some(head),
                    Err(error) if error.code == w::ErrorCode::InactivePlan => None,
                    Err(error) => return Err(error.into()),
                });
            }
            let changed_head = self.inner.head()? != saved_head.inner;
            let mut streams = lock(&self.streams)?;
            let stream_cells = streams.live();
            let mut stream_guards = stream_cells
                .iter()
                .map(|cell| lock(cell))
                .collect::<NativeResult<Vec<_>>>()?;
            self.inner.activate_saved(
                &mut plans[position],
                expected_head.inner,
                saved_head.inner,
            )?;
            if changed_head {
                for stream in &mut stream_guards {
                    stream.close();
                }
                streams.clear();
            }
            registry.clear();
            for (index, (plan, parent)) in plans.iter_mut().zip(parents).enumerate() {
                if index != position && parent == Some(saved_head.inner) {
                    registry.push(std::sync::Arc::downgrade(&live[index]));
                } else {
                    plan.discard();
                }
            }
            Ok(())
        })();
        result.map_err(|error| error.into_python(py))
    }

    #[pyo3(signature = (encoding="utf8"))]
    fn buffered_restore(
        &self,
        py: Python<'_>,
        encoding: &str,
    ) -> PyResult<stream::WorkspaceBufferedRestore> {
        let encoding = stream::encoding(py, encoding)?;
        let result = (|| -> NativeResult<_> {
            let inner = self.inner.buffered_restore(encoding)?;
            lock(&self.streams)?.buffered(inner, encoding)
        })();
        result.map_err(|error| error.into_python(py))
    }
    #[pyo3(signature = (encoding="utf8"))]
    fn progressive_restore(
        &self,
        py: Python<'_>,
        encoding: &str,
    ) -> PyResult<stream::WorkspaceProgressiveRestore> {
        let encoding = stream::encoding(py, encoding)?;
        let result = (|| -> NativeResult<_> {
            let inner = self.inner.progressive_restore(encoding)?;
            lock(&self.streams)?.progressive(inner, encoding)
        })();
        result.map_err(|error| error.into_python(py))
    }
    fn restore(&self, py: Python<'_>, text: &str) -> PyResult<String> {
        self.inner.restore(text).map_err(|e| workspace_error(py, e))
    }
    fn restore_report(&self, py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let value = self
            .inner
            .restore_report(text)
            .map_err(|e| workspace_error(py, e))?;
        json_object(
            py,
            &json!({"text":value.text,"restoredMarkers":value.restored_markers,"issues":value.issues.iter().map(issue_value).collect::<Vec<_>>()}),
        )
    }
    fn recall(&self, py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        let result = self
            .inner
            .recall(text)
            .map_err(|e| workspace_error(py, e))?;
        let findings:Vec<_>=result.findings.iter().map(|f|json!({"finding":finding_value(&f.finding),"byteRange":f.finding.byte_range,"codepointRange":f.codepoint_range,"utf16Range":f.utf16_range})).collect();
        json_object(
            py,
            &json!({"head":head_value(result.head),"complete":true,"findings":findings}),
        )
    }
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.inner.head().map_err(|e| workspace_error(slf.py(), e))?;
        Ok(slf)
    }
    fn __exit__(
        &mut self,
        py: Python<'_>,
        _kind: &Bound<'_, PyAny>,
        _value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "DataFogWorkspaceError",
        module.py().get_type::<DataFogWorkspaceError>(),
    )?;
    stream::register(module)?;
    module.add_class::<Workspace>()?;
    module.add_class::<WorkspaceHead>()?;
    module.add_class::<WorkspaceFinding>()?;
    module.add_class::<WorkspaceInput>()?;
    module.add_class::<WorkspaceReviewCorrection>()?;
    module.add_class::<WorkspaceReviewInput>()?;
    module.add_class::<WorkspacePreparedChange>()?;
    Ok(())
}

// The policy transport has fixed depth and bounded rows; never recurse through
// arbitrary Python containers (including self-referential dictionaries/lists).
fn policy_edits(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<w::PolicyEdits> {
    let invalid = || failure(py, w::ErrorCode::InvalidPolicy);
    let root = value.cast::<PyDict>().map_err(|_| invalid())?;
    if root.len() != 2 {
        return Err(invalid());
    }
    let rules = root.get_item("rules")?.ok_or_else(invalid)?;
    let choices = root.get_item("choices")?.ok_or_else(invalid)?;
    let rules = rules.cast::<PyList>().map_err(|_| invalid())?;
    let choices = choices.cast::<PyList>().map_err(|_| invalid())?;
    if rules.len().saturating_add(choices.len()) > w::MAX_POLICY_EDITS {
        return Err(failure(py, w::ErrorCode::ResourceLimit));
    }
    let mut budget = 0usize;
    let mut rows = |items: &Bound<'_, PyList>, fields: &[&str]| -> PyResult<Vec<Value>> {
        let mut values = Vec::with_capacity(items.len());
        for item in items.iter() {
            let row = item.cast::<PyDict>().map_err(|_| invalid())?;
            if row.len() != fields.len() {
                return Err(invalid());
            }
            let mut object = serde_json::Map::new();
            for &field in fields {
                let value = row.get_item(field)?.ok_or_else(invalid)?;
                let value = if value.is_none() && matches!(field, "rule" | "disposition") {
                    Value::Null
                } else {
                    let text = value.extract::<String>().map_err(|_| invalid())?;
                    budget = budget.saturating_add(text.len().saturating_mul(6).saturating_add(64));
                    if budget > w::MAX_BUNDLE_BYTES {
                        return Err(failure(py, w::ErrorCode::ResourceLimit));
                    }
                    Value::String(text)
                };
                object.insert(field.to_owned(), value);
            }
            values.push(Value::Object(object));
        }
        Ok(values)
    };
    let rules = rows(rules, &["entityType", "rule"])?;
    let choices = rows(choices, &["entityType", "value", "disposition"])?;
    serde_json::from_value(json!({"rules":rules,"choices":choices})).map_err(|_| invalid())
}

impl Workspace {
    fn stage(
        &self,
        py: Python<'_>,
        make: impl FnOnce() -> std::result::Result<w::PreparedChange, w::WorkspaceError>,
    ) -> PyResult<WorkspacePreparedChange> {
        // Core owns candidates; weak references let Python release a dropped plan
        // immediately and avoid keeping private candidates alive via the workspace.
        let result = (|| -> NativeResult<WorkspacePreparedChange> {
            self.inner.head()?;
            let mut registry = lock(&self.plans)?;
            let mut active = Vec::with_capacity(registry.len());
            for weak in registry.drain(..) {
                if let Some(cell) = weak.upgrade() {
                    match lock(&cell)?.expected_head() {
                        Ok(_) => active.push(weak),
                        Err(error) if error.code == w::ErrorCode::InactivePlan => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            *registry = active;
            if registry.len() >= MAX_LIVE_PLANS {
                return Err(core_error(w::ErrorCode::ResourceLimit).into());
            }
            let inner = std::sync::Arc::new(std::sync::Mutex::new(make()?));
            registry.push(std::sync::Arc::downgrade(&inner));
            Ok(WorkspacePreparedChange {
                inner,
                closed: std::sync::Arc::clone(&self.closed),
            })
        })();
        result.map_err(|error| error.into_python(py))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.inner.close();
        let streams = match self.streams.get_mut() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };
        streams.close_on_drop();
        let registry = match self.plans.get_mut() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };
        for cell in registry.drain(..).filter_map(|weak| weak.upgrade()) {
            match cell.lock() {
                Ok(mut plan) => plan.discard(),
                Err(poisoned) => poisoned.into_inner().discard(),
            };
        }
    }
}
