use super::*;
use ::datafog_core::models;
use std::sync::Arc;

create_exception!(datafog_core, DataFogModelError, PyRuntimeError);

fn model_error(py: Python<'_>, error: models::ModelError) -> PyErr {
    let exception = if error.code == "invalid_configuration" {
        PyErr::new::<DataFogConfigurationError, _>(error.to_string())
    } else {
        PyErr::new::<DataFogModelError, _>(error.to_string())
    };
    let value = exception.value(py);
    // Attribute failures are surfaced rather than discarding fallible results.
    let attach = || -> PyResult<()> {
        value.setattr("code", error.code)?;
        value.setattr("reason", error.reason)?;
        value.setattr("path", error.path)?;
        value.setattr("finding_index", error.finding_index)?;
        Ok(())
    };
    match attach() {
        Ok(()) => exception,
        Err(error) => error,
    }
}

fn json_object(py: Python<'_>, value: &impl serde::Serialize) -> PyResult<Py<PyAny>> {
    let json = serde_json::to_string(value)
        .map_err(|_| internal_error(py, "model metadata serialization failed"))?;
    Ok(py.import("json")?.call_method1("loads", (json,))?.unbind())
}

#[pyclass(frozen)]
pub(super) struct ModelScanResult {
    #[pyo3(get)]
    contract_version: u32,
    #[pyo3(get)]
    configuration_id: String,
    #[pyo3(get)]
    findings: Vec<Finding>,
    #[pyo3(get)]
    mode: String,
    #[pyo3(get)]
    diagnostics: Option<Vec<Finding>>,
    evidence: Vec<models::FindingEvidence>,
}
#[pymethods]
impl ModelScanResult {
    #[getter]
    fn evidence(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_object(py, &self.evidence)
    }
}
fn mode_name(mode: models::ScanMode) -> String {
    match mode {
        models::ScanMode::ModelOnly => "model_only",
        models::ScanMode::Combined => "combined",
    }
    .into()
}
impl From<models::ModelScanResult> for ModelScanResult {
    fn from(value: models::ModelScanResult) -> Self {
        Self {
            contract_version: value.contract_version,
            configuration_id: value.configuration_id,
            findings: value.findings.into_iter().map(Finding::from).collect(),
            mode: mode_name(value.mode),
            diagnostics: value
                .diagnostics
                .map(|v| v.into_iter().map(Finding::from).collect()),
            evidence: value.evidence,
        }
    }
}

#[pyclass(frozen)]
pub(super) struct ModelStructuredScanResult {
    #[pyo3(get)]
    contract_version: u32,
    #[pyo3(get)]
    configuration_id: String,
    #[pyo3(get)]
    mode: String,
    #[pyo3(get)]
    mappings: Vec<FieldMapping>,
    #[pyo3(get)]
    findings: Vec<StructuredFinding>,
    #[pyo3(get)]
    diagnostics: Option<Vec<StructuredFinding>>,
    evidence: Vec<models::FindingEvidence>,
}
#[pymethods]
impl ModelStructuredScanResult {
    #[getter]
    fn evidence(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_object(py, &self.evidence)
    }
}
fn located(value: core::structured::StructuredFinding) -> StructuredFinding {
    StructuredFinding {
        path: value.path,
        finding: Finding::from(value.finding),
    }
}
impl From<models::ModelStructuredScanResult> for ModelStructuredScanResult {
    fn from(value: models::ModelStructuredScanResult) -> Self {
        Self {
            contract_version: value.contract_version,
            configuration_id: value.configuration_id,
            mode: mode_name(value.mode),
            mappings: value.mappings.into_iter().map(FieldMapping::from).collect(),
            findings: value.findings.into_iter().map(located).collect(),
            diagnostics: value
                .diagnostics
                .map(|v| v.into_iter().map(located).collect()),
            evidence: value.evidence,
        }
    }
}

/// Share between threads to cancel one request without waiting for the GIL.
#[pyclass(frozen)]
#[derive(Default)]
pub(super) struct ModelCancellation {
    cancellation: Arc<models::Cancellation>,
}
#[pymethods]
impl ModelCancellation {
    #[new]
    fn new() -> Self {
        Self::default()
    }
    fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[pyclass(frozen)]
pub(super) struct ModelSession {
    session: models::ModelSession,
}
fn parse_mode(py: Python<'_>, mode: &str) -> PyResult<models::ScanMode> {
    match mode {
        "model_only" => Ok(models::ScanMode::ModelOnly),
        "combined" => Ok(models::ScanMode::Combined),
        _ => Err(model_error(
            py,
            models::ModelError {
                code: "invalid_configuration".into(),
                reason: "invalid_value".into(),
                path: "/mode".into(),
                finding_index: None,
            },
        )),
    }
}
#[pymethods]
impl ModelSession {
    #[staticmethod]
    fn open(py: Python<'_>, config: &Bound<'_, PyAny>) -> PyResult<Self> {
        let started = std::time::Instant::now();
        let value = py_to_json(py, config, "")?;
        let config = models::parse_config(&value).map_err(|error| model_error(py, error))?;
        py.detach(|| models::ModelSession::open_started(config, started))
            .map(|session| Self { session })
            .map_err(|error| model_error(py, error))
    }
    fn inspect(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_object(py, &self.session.inspect())
    }
    #[pyo3(signature = (text, mode, *, diagnostics=false, cancellation=None))]
    fn scan(
        &self,
        py: Python<'_>,
        text: String,
        mode: &str,
        diagnostics: bool,
        cancellation: Option<PyRef<'_, ModelCancellation>>,
    ) -> PyResult<ModelScanResult> {
        let request = self
            .session
            .prepare_request()
            .map_err(|error| model_error(py, error))?;
        let cancellation = cancellation
            .map(|v| Arc::clone(&v.cancellation))
            .unwrap_or_default();
        let mode = parse_mode(py, mode)?;
        py.detach(|| {
            self.session
                .scan_request(&request, &text, mode, diagnostics, &cancellation)
        })
        .map(ModelScanResult::from)
        .map_err(|error| model_error(py, error))
    }
    #[pyo3(signature = (data, mode, *, diagnostics=false, cancellation=None))]
    fn scan_structured(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
        mode: &str,
        diagnostics: bool,
        cancellation: Option<PyRef<'_, ModelCancellation>>,
    ) -> PyResult<ModelStructuredScanResult> {
        let request = self
            .session
            .prepare_request()
            .map_err(|error| model_error(py, error))?;
        let cancellation = cancellation
            .map(|v| Arc::clone(&v.cancellation))
            .unwrap_or_default();
        let mode = parse_mode(py, mode)?;
        let data = structured_data(py, data)?;
        py.detach(|| {
            self.session
                .scan_structured_request(&request, &data, mode, diagnostics, &cancellation)
        })
        .map(ModelStructuredScanResult::from)
        .map_err(|error| model_error(py, error))
    }
    fn warm_up(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.warm_up())
            .map_err(|error| model_error(py, error))
    }
    fn health(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.health())
            .map_err(|error| model_error(py, error))
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.close())
            .map_err(|error| model_error(py, error))
    }
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __exit__(
        &self,
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
        "DataFogModelError",
        module.py().get_type::<DataFogModelError>(),
    )?;
    module.add_class::<ModelSession>()?;
    module.add_class::<ModelCancellation>()?;
    module.add_class::<ModelScanResult>()?;
    module.add_class::<ModelStructuredScanResult>()?;
    Ok(())
}
