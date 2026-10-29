//! Owned Python transport adapters; decoding and delivery semantics remain in Core.
use super::*;
use pyo3::types::PyInt;
use std::sync::{Arc, Mutex, Weak};
use zeroize::Zeroizing;

create_exception!(
    datafog_core,
    DataFogWorkspaceStreamError,
    DataFogWorkspaceError
);
type Cell = Arc<Mutex<Adapter>>;
#[derive(Default)]
pub(super) struct Registry(Vec<Weak<Mutex<Adapter>>>);
enum Implementation {
    Buffered(w::BufferedRestore),
    Progressive(w::ProgressiveRestore),
}
pub(super) struct Adapter {
    inner: Implementation,
    encoding: w::RestoreEncoding,
}
impl Adapter {
    pub(super) fn close(&mut self) {
        match &mut self.inner {
            Implementation::Buffered(value) => value.close(),
            Implementation::Progressive(value) => value.close(),
        }
    }
    fn reject(&mut self) -> CallResult {
        // Let Core terminate this response and retain its exact delivered counts.
        match self.encoding {
            w::RestoreEncoding::Utf8 => self.feed_utf16(&[]),
            w::RestoreEncoding::Utf16 => self.feed(&[]),
        }
    }
    fn feed(&mut self, bytes: &[u8]) -> CallResult {
        match &mut self.inner {
            Implementation::Buffered(value) => value
                .feed(bytes)
                .map(|()| Output::Empty)
                .map_err(CallError::Core),
            Implementation::Progressive(value) => value
                .feed(bytes)
                .map(Output::Chunk)
                .map_err(CallError::Stream),
        }
    }
    fn feed_utf16(&mut self, units: &[u16]) -> CallResult {
        match &mut self.inner {
            Implementation::Buffered(value) => value
                .feed_utf16(units)
                .map(|()| Output::Empty)
                .map_err(CallError::Core),
            Implementation::Progressive(value) => value
                .feed_utf16(units)
                .map(Output::Chunk)
                .map_err(CallError::Stream),
        }
    }
    fn finalize(&mut self) -> CallResult {
        match &mut self.inner {
            Implementation::Buffered(value) => value
                .finalize()
                .map(Output::Report)
                .map_err(CallError::Core),
            Implementation::Progressive(value) => value
                .finalize()
                .map(Output::Chunk)
                .map_err(CallError::Stream),
        }
    }
    fn reset(&mut self) -> CallResult {
        match &mut self.inner {
            Implementation::Buffered(value) => value
                .reset()
                .map(|()| Output::Empty)
                .map_err(CallError::Core),
            Implementation::Progressive(value) => value
                .reset()
                .map(|()| Output::Empty)
                .map_err(CallError::Stream),
        }
    }
}
impl Registry {
    fn track(&mut self, inner: Implementation, encoding: w::RestoreEncoding) -> NativeResult<Cell> {
        self.0.retain(|value| value.strong_count() != 0);
        self.0
            .try_reserve(1)
            .map_err(|_| core_error(w::ErrorCode::ResourceLimit))?;
        let cell = Arc::new(Mutex::new(Adapter { inner, encoding }));
        self.0.push(Arc::downgrade(&cell));
        Ok(cell)
    }
    pub(super) fn buffered(
        &mut self,
        value: w::BufferedRestore,
        encoding: w::RestoreEncoding,
    ) -> NativeResult<WorkspaceBufferedRestore> {
        Ok(WorkspaceBufferedRestore {
            inner: self.track(Implementation::Buffered(value), encoding)?,
        })
    }
    pub(super) fn progressive(
        &mut self,
        value: w::ProgressiveRestore,
        encoding: w::RestoreEncoding,
    ) -> NativeResult<WorkspaceProgressiveRestore> {
        Ok(WorkspaceProgressiveRestore {
            inner: self.track(Implementation::Progressive(value), encoding)?,
        })
    }
    pub(super) fn live(&self) -> Vec<Cell> {
        self.0.iter().filter_map(Weak::upgrade).collect()
    }
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
    pub(super) fn close(&mut self) -> NativeResult<()> {
        let cells = self.live();
        let mut guards = cells
            .iter()
            .map(|cell| lock(cell))
            .collect::<NativeResult<Vec<_>>>()?;
        for value in &mut guards {
            value.close();
        }
        self.clear();
        Ok(())
    }
    pub(super) fn close_on_drop(&mut self) {
        for cell in self.0.drain(..).filter_map(|value| value.upgrade()) {
            match cell.lock() {
                Ok(mut value) => value.close(),
                Err(poisoned) => poisoned.into_inner().close(),
            }
        }
    }
}
pub(super) fn encoding(py: Python<'_>, value: &str) -> PyResult<w::RestoreEncoding> {
    match value {
        "utf8" => Ok(w::RestoreEncoding::Utf8),
        "utf16" => Ok(w::RestoreEncoding::Utf16),
        _ => Err(failure(py, w::ErrorCode::InvalidEncoding)),
    }
}
fn progress(value: w::RestoreProgress) -> Value {
    json!({"bytes":value.bytes,"codepoints":value.codepoints,"utf16":value.utf16,"restoredMarkers":value.restored_markers})
}
enum Output {
    Empty,
    Report(w::RestoreReport),
    Chunk(w::RestoreChunk),
}
enum CallError {
    Core(w::WorkspaceError),
    Stream(w::StreamFailure),
    Unavailable,
}
type CallResult = std::result::Result<Output, CallError>;
fn finish(py: Python<'_>, result: CallResult) -> PyResult<Py<PyAny>> {
    match result {
        Ok(Output::Empty) => Ok(py.None()),
        Ok(Output::Report(value)) => {
            let result = PyDict::new(py);
            result.set_item("text", value.text.as_deref())?;
            result.set_item("restoredMarkers", value.restored_markers)?;
            result.set_item(
                "issues",
                json_object(
                    py,
                    &value.issues.iter().map(issue_value).collect::<Vec<_>>(),
                )?,
            )?;
            Ok(result.into_any().unbind())
        }
        Ok(Output::Chunk(value)) => {
            let result = PyDict::new(py);
            result.set_item("text", value.text.as_str())?;
            result.set_item("delivered", json_object(py, &progress(value.delivered))?)?;
            result.set_item("complete", value.complete)?;
            Ok(result.into_any().unbind())
        }
        Err(CallError::Core(value)) => Err(workspace_error(py, value)),
        Err(CallError::Unavailable) => {
            Err(PyRuntimeError::new_err("workspace stream is unavailable"))
        }
        Err(CallError::Stream(value)) => {
            let error = workspace_exception(
                py,
                PyErr::new::<DataFogWorkspaceStreamError, _>(value.error.code.as_str()),
                *value.error,
            );
            let attached = (|| -> PyResult<()> {
                error
                    .value(py)
                    .setattr("delivered", json_object(py, &progress(value.delivered))?)?;
                error.value(py).setattr("complete", false)?;
                Ok(())
            })();
            Err(match attached {
                Ok(()) => error,
                Err(error) => error,
            })
        }
    }
}
fn native_call(cell: &Cell, action: impl FnOnce(&mut Adapter) -> CallResult) -> CallResult {
    match cell.lock() {
        Ok(mut guard) => action(&mut guard),
        Err(_) => Err(CallError::Unavailable),
    }
}
fn call(
    py: Python<'_>,
    cell: &Cell,
    action: impl FnOnce(&mut Adapter) -> CallResult,
) -> PyResult<Py<PyAny>> {
    finish(py, native_call(cell, action))
}
fn feed(py: Python<'_>, cell: &Cell, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    match value.cast::<PyBytes>() {
        Ok(value) => call(py, cell, |adapter| adapter.feed(value.as_bytes())),
        Err(_) => call(py, cell, Adapter::reject),
    }
}
fn feed_utf16(py: Python<'_>, cell: &Cell, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let units = (|| -> PyResult<Option<Zeroizing<Vec<u16>>>> {
        let Ok(list) = value.cast::<PyList>() else {
            return Ok(None);
        };
        // Every valid unit contributes at least one UTF-8 byte on average. Two
        // extra units cover a split surrogate, so an omitted tail cannot succeed.
        // Keep one Core feed: subdividing would incorrectly count undelivered text.
        let count = list.len().min(w::MAX_BUNDLE_BYTES + 2);
        let mut units = Zeroizing::new(Vec::new());
        units.try_reserve_exact(count).map_err(|_| {
            pyo3::exceptions::PyMemoryError::new_err("workspace stream transport allocation failed")
        })?;
        for value in list.iter().take(count) {
            if !value.is_exact_instance_of::<PyInt>() {
                return Ok(None);
            }
            let Ok(value) = value.extract::<u16>() else {
                return Ok(None);
            };
            units.push(value);
        }
        Ok(Some(units))
    })()?;
    match units {
        Some(units) => {
            let result = native_call(cell, |adapter| adapter.feed_utf16(&units));
            drop(units);
            finish(py, result)
        }
        None => call(py, cell, Adapter::reject),
    }
}
fn close(py: Python<'_>, cell: &Cell) -> PyResult<()> {
    let result = (|| -> NativeResult<()> {
        lock(cell)?.close();
        Ok(())
    })();
    result.map_err(|error| error.into_python(py))
}
#[pyclass(name = "WorkspaceBufferedRestore")]
pub(super) struct WorkspaceBufferedRestore {
    inner: Cell,
}
#[pymethods]
impl WorkspaceBufferedRestore {
    fn feed(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        feed(py, &self.inner, value)
    }
    fn feed_utf16(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        feed_utf16(py, &self.inner, value)
    }
    fn finalize(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        call(py, &self.inner, Adapter::finalize)
    }
    fn reset(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        call(py, &self.inner, Adapter::reset)
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        close(py, &self.inner)
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
#[pyclass(name = "WorkspaceProgressiveRestore")]
pub(super) struct WorkspaceProgressiveRestore {
    inner: Cell,
}
#[pymethods]
impl WorkspaceProgressiveRestore {
    fn feed(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        feed(py, &self.inner, value)
    }
    fn feed_utf16(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        feed_utf16(py, &self.inner, value)
    }
    fn finalize(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        call(py, &self.inner, Adapter::finalize)
    }
    fn reset(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        call(py, &self.inner, Adapter::reset)
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        close(py, &self.inner)
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
        "DataFogWorkspaceStreamError",
        module.py().get_type::<DataFogWorkspaceStreamError>(),
    )?;
    module.add_class::<WorkspaceBufferedRestore>()?;
    module.add_class::<WorkspaceProgressiveRestore>()?;
    Ok(())
}
