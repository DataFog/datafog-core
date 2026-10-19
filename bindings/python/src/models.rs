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
    model: String,
    #[pyo3(get)]
    findings: Vec<Finding>,
    evidence: Vec<models::FindingEvidence>,
}
#[pymethods]
impl ModelScanResult {
    #[getter]
    fn evidence(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_object(py, &self.evidence)
    }
}
impl From<models::ModelScanResult> for ModelScanResult {
    fn from(value: models::ModelScanResult) -> Self {
        Self {
            contract_version: value.contract_version,
            model: value.model,
            findings: value.findings.into_iter().map(Finding::from).collect(),
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
    #[pyo3(signature = (text, *, cancellation=None))]
    fn scan(
        &self,
        py: Python<'_>,
        text: String,
        cancellation: Option<PyRef<'_, ModelCancellation>>,
    ) -> PyResult<ModelScanResult> {
        let request = self
            .session
            .prepare_request()
            .map_err(|error| model_error(py, error))?;
        let cancellation = cancellation
            .map(|v| Arc::clone(&v.cancellation))
            .unwrap_or_default();
        py.detach(|| self.session.scan_request(&request, &text, &cancellation))
            .map(ModelScanResult::from)
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
    Ok(())
}
