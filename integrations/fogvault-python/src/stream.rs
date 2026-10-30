use super::*;
pub enum Implementation {
    Buffered(core::BufferedRestore),
    Progressive(core::ProgressiveRestore),
}
impl Implementation {
    pub fn close(&mut self) {
        match self {
            Self::Buffered(v) => v.close(),
            Self::Progressive(v) => v.close(),
        }
    }
}
fn progress(v: core::RestoreProgress) -> Value {
    json!({"bytes":v.bytes,"codepoints":v.codepoints,"utf16":v.utf16,"restoredMarkers":v.restored_markers})
}
fn chunk(py: Python<'_>, v: core::RestoreChunk) -> PyResult<Py<PyAny>> {
    transport::value(
        py,
        json!({"text":v.text,"delivered":progress(v.delivered),"complete":v.complete}),
    )
}
fn failure(py: Python<'_>, v: core::StreamFailure) -> PyErr {
    let e = native_error(py, Error::Core(*v.error));
    let attach = || -> PyResult<()> {
        e.value(py)
            .setattr("delivered", transport::value(py, progress(v.delivered))?)?;
        e.value(py).setattr("complete", v.complete)?;
        Ok(())
    };
    if let Err(e) = attach() {
        return e;
    }
    e
}
fn reject(py: Python<'_>, stream: &mut Implementation, encoding: core::RestoreEncoding) -> PyErr {
    match stream {
        Implementation::Buffered(v) => {
            let result = if encoding == core::RestoreEncoding::Utf8 {
                v.feed_utf16(&[])
            } else {
                v.feed(&[])
            };
            match result {
                Err(e) => native_error(py, Error::Core(e)),
                Ok(()) => {
                    v.close();
                    fail(py, "invalid_encoding")
                }
            }
        }
        Implementation::Progressive(v) => {
            let result = if encoding == core::RestoreEncoding::Utf8 {
                v.feed_utf16(&[])
            } else {
                v.feed(&[])
            };
            match result {
                Err(e) => failure(py, e),
                Ok(_) => {
                    v.close();
                    fail(py, "invalid_encoding")
                }
            }
        }
    }
}
fn units_transport(py: Python<'_>, units: &Bound<'_, PyAny>) -> PyResult<Zeroizing<Vec<u16>>> {
    use pyo3::types::{PyInt, PyList};
    if !units.is_exact_instance_of::<PyList>() {
        return Err(fail(py, "invalid_input"));
    }
    let list = units
        .cast::<PyList>()
        .map_err(|_| fail(py, "invalid_input"))?;
    if list.len() > core::MAX_BUNDLE_BYTES {
        return Err(fail(py, "resource_limit"));
    }
    let mut result = Zeroizing::new(Vec::new());
    result
        .try_reserve(list.len())
        .map_err(|_| fail(py, "resource_limit"))?;
    for unit in list {
        if !unit.is_exact_instance_of::<PyInt>() {
            return Err(fail(py, "invalid_input"));
        }
        result.push(
            unit.extract::<u16>()
                .map_err(|_| fail(py, "invalid_input"))?,
        );
    }
    Ok(result)
}
#[pyclass(name = "RestoreStream")]
pub struct RestoreStream {
    shared: Arc<Shared>,
    inner: Arc<Mutex<Implementation>>,
    encoding: core::RestoreEncoding,
}
pub fn make(
    py: Python<'_>,
    shared: &Arc<Shared>,
    tenant: &str,
    encoding: &str,
    progressive: bool,
    logical: Option<core::PublicId>,
) -> PyResult<RestoreStream> {
    let encoding = match encoding {
        "utf8" => core::RestoreEncoding::Utf8,
        "utf16" => core::RestoreEncoding::Utf16,
        _ => return Err(fail(py, "invalid_encoding")),
    };
    let g = shared.owner(py, tenant)?;
    let owner = g.as_ref().ok_or_else(|| fail(py, "owner_closed"))?;
    let inner = match (progressive, logical) {
        (false, None) => Implementation::Buffered(
            owner
                .buffered_restore(encoding)
                .map_err(|e| native_error(py, e))?,
        ),
        (false, Some(id)) => Implementation::Buffered(
            owner
                .buffered_restore_in_conversation(id, encoding)
                .map_err(|e| native_error(py, e))?,
        ),
        (true, None) => Implementation::Progressive(
            owner
                .progressive_restore(encoding)
                .map_err(|e| native_error(py, e))?,
        ),
        (true, Some(id)) => Implementation::Progressive(
            owner
                .progressive_restore_in_conversation(id, encoding)
                .map_err(|e| native_error(py, e))?,
        ),
    };
    let inner = Arc::new(Mutex::new(inner));
    let mut registry = lock(py, &shared.streams)?;
    registry.retain(|s| s.strong_count() > 0);
    registry.push(Arc::downgrade(&inner));
    Ok(RestoreStream {
        shared: Arc::clone(shared),
        inner,
        encoding,
    })
}
#[pymethods]
impl RestoreStream {
    fn feed(&self, py: Python<'_>, tenant: &str, bytes: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        let bytes = transport::bytes(py, bytes, core::MAX_BUNDLE_BYTES);
        let mut stream = lock(py, &self.inner)?;
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(_) => return Err(reject(py, &mut stream, self.encoding)),
        };
        match &mut *stream {
            Implementation::Buffered(v) => {
                v.feed(bytes)
                    .map_err(|e| native_error(py, Error::Core(e)))?;
                Ok(py.None())
            }
            Implementation::Progressive(v) => chunk(py, v.feed(bytes).map_err(|e| failure(py, e))?),
        }
    }
    fn feed_utf16(
        &self,
        py: Python<'_>,
        tenant: &str,
        units: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        let parsed = units_transport(py, units);
        let mut stream = lock(py, &self.inner)?;
        let units = match parsed {
            Ok(units) => units,
            Err(_) => return Err(reject(py, &mut stream, self.encoding)),
        };
        match &mut *stream {
            Implementation::Buffered(v) => {
                v.feed_utf16(&units)
                    .map_err(|e| native_error(py, Error::Core(e)))?;
                Ok(py.None())
            }
            Implementation::Progressive(v) => {
                chunk(py, v.feed_utf16(&units).map_err(|e| failure(py, e))?)
            }
        }
    }
    fn finalize(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        let mut stream = lock(py, &self.inner)?;
        match &mut *stream {
            Implementation::Buffered(v) => transport::report(
                py,
                &v.finalize().map_err(|e| native_error(py, Error::Core(e)))?,
            ),
            Implementation::Progressive(v) => chunk(py, v.finalize().map_err(|e| failure(py, e))?),
        }
    }
    fn reset(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        let _g = self.shared.owner(py, tenant)?;
        match &mut *lock(py, &self.inner)? {
            Implementation::Buffered(v) => v.reset().map_err(|e| native_error(py, Error::Core(e))),
            Implementation::Progressive(v) => v.reset().map_err(|e| failure(py, e)),
        }
    }
    fn close(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        self.shared.check(py, tenant)?;
        lock(py, &self.inner)?.close();
        Ok(())
    }
}
