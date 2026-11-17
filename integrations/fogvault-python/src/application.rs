//! Thin Python transport over the Rust application-scoped persistence helper.
use super::*;
use datafog_fogvault_native::application as app;
use std::path::PathBuf;

fn error(py: Python<'_>, e: app::Error) -> PyErr {
    match e {
        app::Error::Core(e) => native_error(py, Error::Core(e)),
        e => fail(py, e.code()),
    }
}
fn operation_error(py: Python<'_>, e: app::Error, operation: core::PublicId) -> PyErr {
    match e {
        app::Error::OutcomeUnknown | app::Error::SavedNotActivated => {
            unknown_outcome(py, operation)
        }
        e => error(py, e),
    }
}

#[pyclass(name = "ApplicationVault")]
pub struct ApplicationVault {
    owner: Mutex<app::ApplicationVault>,
}
#[pymethods]
impl ApplicationVault {
    #[staticmethod]
    #[pyo3(signature = (app_id, *, directory=None, key=None, allow_file_key=false))]
    fn open(
        py: Python<'_>,
        app_id: String,
        directory: Option<String>,
        key: Option<&[u8]>,
        allow_file_key: bool,
    ) -> PyResult<Self> {
        let mut options = app::Options::new(app_id);
        options.directory = directory.map(PathBuf::from);
        options.key = key
            .map(|k| {
                k.try_into()
                    .map(Zeroizing::new)
                    .map_err(|_| fail(py, "invalid_key"))
            })
            .transpose()?;
        options.allow_file_key = allow_file_key;
        py.detach(|| app::ApplicationVault::open(options))
            .map(|owner| Self {
                owner: Mutex::new(owner),
            })
            .map_err(|e| error(py, e))
    }
    #[getter]
    fn protection_mode(&self, py: Python<'_>) -> PyResult<String> {
        Ok(lock(py, &self.owner)?.protection_mode().as_str().into())
    }
    #[getter]
    fn directory(&self, py: Python<'_>) -> PyResult<String> {
        lock(py, &self.owner)?
            .directory()
            .to_str()
            .map(String::from)
            .ok_or_else(|| fail(py, "unsafe_path"))
    }
    #[getter]
    fn vault_id<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(py, &lock(py, &self.owner)?.vault_id()))
    }
    fn head(&self, py: Python<'_>) -> PyResult<Head> {
        lock(py, &self.owner)?
            .refresh()
            .map(|inner| Head { inner })
            .map_err(|e| error(py, e))
    }
    fn prepare_and_commit(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        findings: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
        approved: &Bound<'_, PyAny>,
    ) -> PyResult<String> {
        transport::approved(py, approved)?;
        let text = Zeroizing::new(transport::text(py, text)?);
        let findings: Vec<transport::Finding> = transport::decode(py, findings)?;
        let findings = findings
            .into_iter()
            .map(|f| f.into_core(py))
            .collect::<PyResult<Vec<_>>>()?;
        let operation = transport::id(py, operation_id)?;
        let result = py.detach(|| {
            self.owner
                .try_lock()
                .map_err(|_| app::Error::LockTimeout)?
                .prepare_and_commit(&text, &findings, operation)
        });
        let committed = result.map_err(|e| operation_error(py, e, operation))?;
        committed
            .texts
            .into_iter()
            .next()
            .ok_or_else(|| unknown_outcome(py, operation))
    }
    fn forget(
        &self,
        py: Python<'_>,
        codes: Vec<String>,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<Head> {
        let operation = transport::id(py, operation_id)?;
        py.detach(|| {
            self.owner
                .try_lock()
                .map_err(|_| app::Error::LockTimeout)?
                .forget(&codes, operation)
        })
        .map(|inner| Head { inner })
        .map_err(|e| operation_error(py, e, operation))
    }
    fn restore(&self, py: Python<'_>, text: &Bound<'_, PyAny>) -> PyResult<String> {
        let text = Zeroizing::new(transport::text(py, text)?);
        py.detach(|| {
            self.owner
                .try_lock()
                .map_err(|_| app::Error::LockTimeout)?
                .restore(&text)
        })
        .map_err(|e| error(py, e))
    }
    fn restore_report(&self, py: Python<'_>, text: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let text = Zeroizing::new(transport::text(py, text)?);
        let report = py
            .detach(|| {
                self.owner
                    .try_lock()
                    .map_err(|_| app::Error::LockTimeout)?
                    .restore_report(&text)
            })
            .map_err(|e| error(py, e))?;
        transport::report(py, &report)
    }
    fn operations(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let rows = py
            .detach(|| {
                self.owner
                    .try_lock()
                    .map_err(|_| app::Error::LockTimeout)?
                    .operations()
            })
            .map_err(|e| error(py, e))?;
        transport::value(py, Value::Array(rows.iter().map(|r| json!({"id":r.id,"kind":r.kind.as_str(),"occurredAtMs":r.occurred_at_ms,"preparedInputs":r.prepared_inputs,"transformedSpans":r.transformed_spans})).collect()))
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        lock(py, &self.owner)?.close();
        Ok(())
    }
}
