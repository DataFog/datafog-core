use ::datafog_core as core;
use pyo3::create_exception;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyList, PyTuple};

create_exception!(datafog_core, DataFogConfigurationError, PyValueError);
create_exception!(datafog_core, DataFogFindingError, PyValueError);
create_exception!(datafog_core, DataFogInternalError, PyRuntimeError);
create_exception!(datafog_core, DataFogKeyProviderError, PyRuntimeError);

/// A zero-based, end-exclusive text range.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone, PartialEq, Eq)]
struct TextRange {
    #[pyo3(get)]
    start: usize,

    #[pyo3(get)]
    end: usize,
}

impl From<core::TextRange> for TextRange {
    fn from(range: core::TextRange) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

#[pymethods]
impl TextRange {
    #[new]
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    fn __repr__(&self) -> String {
        format!("TextRange(start={}, end={})", self.start, self.end)
    }

    fn __eq__(&self, other: PyRef<'_, TextRange>) -> bool {
        self == &*other
    }
}

/// A piece of potentially sensitive content detected in input text.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone, PartialEq)]
struct Finding {
    #[pyo3(get)]
    entity_type: String,

    #[pyo3(get)]
    matched_text: String,

    #[pyo3(get)]
    byte_range: TextRange,

    #[pyo3(get)]
    codepoint_range: TextRange,

    #[pyo3(get)]
    confidence: Option<f32>,

    #[pyo3(get)]
    detector_name: String,

    #[pyo3(get)]
    detector_version: Option<String>,
}

impl From<core::Finding> for Finding {
    fn from(finding: core::Finding) -> Self {
        Self {
            entity_type: finding.entity_type,
            matched_text: finding.matched_text,
            byte_range: finding.byte_range.into(),
            codepoint_range: finding.codepoint_range.into(),
            confidence: finding.confidence,
            detector_name: finding.detector_name,
            detector_version: finding.detector_version,
        }
    }
}

#[pymethods]
impl Finding {
    #[new]
    #[pyo3(signature = (
        entity_type,
        matched_text,
        byte_range,
        codepoint_range,
        detector_name,
        confidence=None,
        detector_version=None
    ))]
    fn new(
        entity_type: String,
        matched_text: String,
        byte_range: PyRef<'_, TextRange>,
        codepoint_range: PyRef<'_, TextRange>,
        detector_name: String,
        confidence: Option<f32>,
        detector_version: Option<String>,
    ) -> Self {
        Self {
            entity_type,
            matched_text,
            byte_range: byte_range.clone(),
            codepoint_range: codepoint_range.clone(),
            confidence,
            detector_name,
            detector_version,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Finding(entity_type={:?}, matched_text={:?}, byte_range={:?}, \
             codepoint_range={:?}, confidence={:?}, detector_name={:?}, \
             detector_version={:?})",
            self.entity_type,
            self.matched_text,
            (self.byte_range.start, self.byte_range.end),
            (self.codepoint_range.start, self.codepoint_range.end),
            self.confidence,
            self.detector_name,
            self.detector_version,
        )
    }

    fn __eq__(&self, other: PyRef<'_, Finding>) -> bool {
        self.entity_type == other.entity_type
            && self.matched_text == other.matched_text
            && self.byte_range == other.byte_range
            && self.codepoint_range == other.codepoint_range
            && self.confidence == other.confidence
            && self.detector_name == other.detector_name
            && self.detector_version == other.detector_version
    }
}

impl Finding {
    fn to_core(&self) -> core::Finding {
        core::Finding {
            entity_type: self.entity_type.clone(),
            matched_text: self.matched_text.clone(),
            byte_range: core::TextRange {
                start: self.byte_range.start,
                end: self.byte_range.end,
            },
            codepoint_range: core::TextRange {
                start: self.codepoint_range.start,
                end: self.codepoint_range.end,
            },
            confidence: self.confidence,
            detector_name: self.detector_name.clone(),
            detector_version: self.detector_version.clone(),
        }
    }
}

/// One privacy transformation applied to source text.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone, PartialEq)]
struct Transformation {
    #[pyo3(get)]
    entity_type: String,

    #[pyo3(get)]
    source_byte_range: TextRange,

    #[pyo3(get)]
    source_codepoint_range: TextRange,

    #[pyo3(get)]
    confidence: Option<f32>,

    #[pyo3(get)]
    detector_name: String,

    #[pyo3(get)]
    detector_version: Option<String>,

    #[pyo3(get)]
    strategy: String,

    #[pyo3(get)]
    replacement: String,

    #[pyo3(get)]
    output_byte_range: TextRange,

    #[pyo3(get)]
    output_codepoint_range: TextRange,

    #[pyo3(get)]
    key_ref: Option<String>,

    #[pyo3(get)]
    resolved_key_version: Option<String>,

    #[pyo3(get)]
    token_ref: Option<String>,

    #[pyo3(get)]
    resolved_token_version: Option<String>,
}

impl From<core::Transformation> for Transformation {
    fn from(transformation: core::Transformation) -> Self {
        Self {
            entity_type: transformation.entity_type,
            source_byte_range: transformation.source_byte_range.into(),
            source_codepoint_range: transformation.source_codepoint_range.into(),
            confidence: transformation.confidence,
            detector_name: transformation.detector_name,
            detector_version: transformation.detector_version,
            strategy: match transformation.strategy {
                core::TransformationStrategy::Redact => "redact".to_owned(),
                core::TransformationStrategy::Remove => "remove".to_owned(),
                core::TransformationStrategy::Mask(_) => "mask".to_owned(),
                core::TransformationStrategy::Pseudonymize(_) => "pseudonymize".to_owned(),
                core::TransformationStrategy::Tokenize(_) => "tokenize".to_owned(),
            },
            replacement: transformation.replacement,
            output_byte_range: transformation.output_byte_range.into(),
            output_codepoint_range: transformation.output_codepoint_range.into(),
            key_ref: transformation.key_ref,
            resolved_key_version: transformation.resolved_key_version,
            token_ref: transformation.token_ref,
            resolved_token_version: transformation.resolved_token_version,
        }
    }
}

#[pymethods]
impl Transformation {
    fn __eq__(&self, other: PyRef<'_, Transformation>) -> bool {
        self.entity_type == other.entity_type
            && self.source_byte_range == other.source_byte_range
            && self.source_codepoint_range == other.source_codepoint_range
            && self.confidence == other.confidence
            && self.detector_name == other.detector_name
            && self.detector_version == other.detector_version
            && self.strategy == other.strategy
            && self.replacement == other.replacement
            && self.output_byte_range == other.output_byte_range
            && self.output_codepoint_range == other.output_codepoint_range
            && self.key_ref == other.key_ref
            && self.resolved_key_version == other.resolved_key_version
            && self.token_ref == other.token_ref
            && self.resolved_token_version == other.resolved_token_version
    }
}

/// Text and audit records produced by a transformation.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct TransformResult {
    #[pyo3(get)]
    text: String,

    #[pyo3(get)]
    transformations: Vec<Transformation>,
}

impl From<core::TransformResult> for TransformResult {
    fn from(result: core::TransformResult) -> Self {
        Self {
            text: result.text,
            transformations: result
                .transformations
                .into_iter()
                .map(Transformation::from)
                .collect(),
        }
    }
}

#[pymethods]
impl TransformResult {
    fn __eq__(&self, other: PyRef<'_, TransformResult>) -> bool {
        self.text == other.text && self.transformations == other.transformations
    }
}

#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct Restoration {
    #[pyo3(get)]
    source_byte_range: TextRange,
    #[pyo3(get)]
    source_codepoint_range: TextRange,
    #[pyo3(get)]
    output_byte_range: TextRange,
    #[pyo3(get)]
    output_codepoint_range: TextRange,
    #[pyo3(get)]
    token_ref: String,
    #[pyo3(get)]
    resolved_token_version: String,
}

#[pyclass(frozen, skip_from_py_object)]
struct RestoreResult {
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    restorations: Vec<Restoration>,
}

impl From<core::RestoreResult> for RestoreResult {
    fn from(result: core::RestoreResult) -> Self {
        Self {
            text: result.text,
            restorations: result
                .restorations
                .into_iter()
                .map(Restoration::from)
                .collect(),
        }
    }
}

struct PythonKeyProvider {
    provider: Option<Py<PyAny>>,
}

fn provider_error_kind(error: &PyErr) -> core::KeyProviderErrorKind {
    Python::attach(|py| {
        let code = error
            .value(py)
            .getattr("code")
            .and_then(|value| value.extract::<String>())
            .ok();
        match code.as_deref() {
            Some("key_not_found") => core::KeyProviderErrorKind::NotFound,
            Some("key_access_denied") => core::KeyProviderErrorKind::AccessDenied,
            Some("key_provider_unavailable") => core::KeyProviderErrorKind::Unavailable,
            _ => core::KeyProviderErrorKind::ProviderError,
        }
    })
}

fn provider_field<'py>(
    value: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if let Ok(dictionary) = value.cast::<PyDict>() {
        dictionary.get_item(name)
    } else {
        value.getattr(name).map(Some)
    }
}

fn required_provider_string(value: &Bound<'_, PyAny>, name: &str) -> PyResult<String> {
    provider_field(value, name)?
        .ok_or_else(|| PyErr::new::<PyTypeError, _>(format!("provider response requires {name}")))?
        .extract()
}

fn required_provider_bytes(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<u8>> {
    provider_field(value, name)?
        .ok_or_else(|| PyErr::new::<PyTypeError, _>(format!("provider response requires {name}")))?
        .extract()
}

fn resolved_key_from_python(value: &Bound<'_, PyAny>) -> core::ResolvedKey {
    let key = provider_field(value, "key")
        .ok()
        .flatten()
        .and_then(|value| value.extract::<Vec<u8>>().ok())
        .unwrap_or_default();
    let resolved_version = provider_field(value, "resolved_version")
        .ok()
        .flatten()
        .and_then(|value| value.extract::<String>().ok())
        .unwrap_or_default();
    core::ResolvedKey::new(key, resolved_version)
}

impl core::KeyProvider for PythonKeyProvider {
    fn is_configured(&self) -> bool {
        self.provider.is_some()
    }

    fn resolve_key(&self, selector: core::KeySelector) -> core::KeyProviderFuture<'_> {
        let provider = Python::attach(|py| {
            self.provider
                .as_ref()
                .map(|provider| provider.clone_ref(py))
        });
        Box::pin(async move {
            let future = Python::attach(|py| {
                let awaitable = provider
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("key provider is required"))?
                    .bind(py)
                    .call_method1("resolve_key", (selector.key_ref(), selector.key_version()))?;
                pyo3_async_runtimes::tokio::into_future(awaitable)
            })
            .map_err(|error| core::KeyProviderError::new(provider_error_kind(&error)))?;
            let response = future
                .await
                .map_err(|error| core::KeyProviderError::new(provider_error_kind(&error)))?;
            Ok(Python::attach(|py| {
                resolved_key_from_python(response.bind(py))
            }))
        })
    }
}

struct PythonTokenProvider {
    provider: Option<Py<PyAny>>,
}

fn token_provider_error_kind(error: &PyErr) -> core::TokenProviderErrorKind {
    Python::attach(|py| {
        let code = error
            .value(py)
            .getattr("code")
            .and_then(|value| value.extract::<String>())
            .ok();
        match code.as_deref() {
            Some("token_not_found") => core::TokenProviderErrorKind::NotFound,
            Some("token_expired") => core::TokenProviderErrorKind::Expired,
            Some("token_access_denied") => core::TokenProviderErrorKind::AccessDenied,
            Some("token_provider_unavailable") => core::TokenProviderErrorKind::Unavailable,
            _ => core::TokenProviderErrorKind::ProviderError,
        }
    })
}

impl core::TokenProvider for PythonTokenProvider {
    fn is_configured(&self) -> bool {
        self.provider.is_some()
    }

    fn tokenize_batch(
        &self,
        scope: &str,
        items: Vec<core::TokenizeItem>,
    ) -> core::TokenizeProviderFuture<'_> {
        let provider = Python::attach(|py| {
            self.provider
                .as_ref()
                .map(|provider| provider.clone_ref(py))
        });
        let scope = scope.to_owned();
        Box::pin(async move {
            let future = Python::attach(|py| -> PyResult<_> {
                let provider = provider
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("token provider is required"))?;
                let requests = PyList::empty(py);
                for item in items {
                    let request = PyDict::new(py);
                    request.set_item("id", item.id())?;
                    request.set_item("exact_value", item.exact_value())?;
                    request.set_item("token_ref", item.token_ref())?;
                    requests.append(request)?;
                }
                let awaitable = provider
                    .bind(py)
                    .call_method1("tokenize_batch", (scope, requests))?;
                pyo3_async_runtimes::tokio::into_future(awaitable)
            })
            .map_err(|error| core::TokenProviderError::new(token_provider_error_kind(&error)))?;
            let response = future.await.map_err(|error| {
                core::TokenProviderError::new(token_provider_error_kind(&error))
            })?;
            Ok(Python::attach(|py| {
                let Ok(values) = response.bind(py).try_iter() else {
                    return vec![core::TokenizeResult::new("", Vec::new(), "")];
                };
                let mut parsed = Vec::new();
                for value in values {
                    let Ok(value) = value else {
                        return vec![core::TokenizeResult::new("", Vec::new(), "")];
                    };
                    let parsed_fields = (
                        required_provider_string(&value, "id"),
                        required_provider_bytes(&value, "payload"),
                        required_provider_string(&value, "resolved_version"),
                    );
                    let (Ok(id), Ok(payload), Ok(version)) = parsed_fields else {
                        return vec![core::TokenizeResult::new("", Vec::new(), "")];
                    };
                    parsed.push(core::TokenizeResult::new(id, payload, version));
                }
                parsed
            }))
        })
    }

    fn restore_batch(
        &self,
        scope: &str,
        items: Vec<core::RestoreItem>,
    ) -> core::RestoreProviderFuture<'_> {
        let provider = Python::attach(|py| {
            self.provider
                .as_ref()
                .map(|provider| provider.clone_ref(py))
        });
        let scope = scope.to_owned();
        Box::pin(async move {
            let future = Python::attach(|py| -> PyResult<_> {
                let provider = provider
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("token provider is required"))?;
                let requests = PyList::empty(py);
                for item in items {
                    let request = PyDict::new(py);
                    request.set_item("id", item.id())?;
                    request.set_item("token_ref", item.token_ref())?;
                    request.set_item("resolved_version", item.resolved_version())?;
                    request.set_item("payload", item.payload())?;
                    requests.append(request)?;
                }
                let awaitable = provider
                    .bind(py)
                    .call_method1("restore_batch", (scope, requests))?;
                pyo3_async_runtimes::tokio::into_future(awaitable)
            })
            .map_err(|error| core::TokenProviderError::new(token_provider_error_kind(&error)))?;
            let response = future.await.map_err(|error| {
                core::TokenProviderError::new(token_provider_error_kind(&error))
            })?;
            Ok(Python::attach(|py| {
                let Ok(values) = response.bind(py).try_iter() else {
                    return vec![core::RestoredValue::new("", "")];
                };
                let mut parsed = Vec::new();
                for value in values {
                    let Ok(value) = value else {
                        return vec![core::RestoredValue::new("", "")];
                    };
                    let parsed_fields = (
                        required_provider_string(&value, "id"),
                        required_provider_string(&value, "value"),
                    );
                    let (Ok(id), Ok(restored)) = parsed_fields else {
                        return vec![core::RestoredValue::new("", "")];
                    };
                    parsed.push(core::RestoredValue::new(id, restored));
                }
                parsed
            }))
        })
    }
}

/// Provider-backed asynchronous privacy manager.
#[pyclass(frozen, skip_from_py_object)]
struct PrivacyManager {
    key_provider: Option<Py<PyAny>>,
    token_provider: Option<Py<PyAny>>,
}

#[pymethods]
impl PrivacyManager {
    #[pyo3(signature = (data, findings, config, context=None))]
    fn transform_structured<'py>(
        &self,
        py: Python<'py>,
        data: Py<PyAny>,
        findings: Vec<Py<StructuredFinding>>,
        config: Py<PyAny>,
        context: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let data = structured_data(py, data.bind(py))?;
        let config_value = structured_options(py, config.bind(py), "")?;
        let config = core::parse_transformation_config(&config_value)
            .map_err(|error| privacy_error(py, error))?;
        let findings = findings
            .iter()
            .map(|finding| finding.bind(py).borrow().to_core())
            .collect::<Vec<_>>();
        let context = context
            .map(|context| structured_options(py, context.bind(py), ""))
            .transpose()?
            .map(|value| core::parse_privacy_context(&value))
            .transpose()
            .map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .transform_structured(&data, &findings, &config, context.as_ref())
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, structured_transform_result(py, result)?))
        })
    }
    #[pyo3(signature = (data, config, context=None))]
    fn scan_and_transform_structured<'py>(
        &self,
        py: Python<'py>,
        data: Py<PyAny>,
        config: Py<PyAny>,
        context: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let data = structured_data(py, data.bind(py))?;
        let config_value = structured_options(py, config.bind(py), "")?;
        let config = core::structured::parse_scan_and_transform_config(&config_value)
            .map_err(|error| privacy_error(py, error))?;
        let context = context
            .map(|context| structured_options(py, context.bind(py), ""))
            .transpose()?
            .map(|value| core::parse_privacy_context(&value))
            .transpose()
            .map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .scan_and_transform_structured(&data, &config, context.as_ref())
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, structured_transform_result(py, result)?))
        })
    }
    fn restore_structured<'py>(
        &self,
        py: Python<'py>,
        data: Py<PyAny>,
        context: Py<PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let data = structured_data(py, data.bind(py))?;
        let context = structured_options(py, context.bind(py), "")?;
        let context =
            core::parse_privacy_context(&context).map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .restore_structured(&data, &context)
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, structured_restore_result(py, result)?))
        })
    }

    #[new]
    #[pyo3(signature = (provider=None, token_provider=None))]
    fn new(
        py: Python<'_>,
        provider: Option<Py<PyAny>>,
        token_provider: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        if let Some(provider) = &provider {
            let resolve_key = provider.bind(py).getattr("resolve_key").map_err(|_| {
                PyErr::new::<PyTypeError, _>(
                    "key provider must define resolve_key(key_ref, key_version)",
                )
            })?;
            if !resolve_key.is_callable() {
                return Err(PyErr::new::<PyTypeError, _>(
                    "provider resolve_key attribute must be callable",
                ));
            }
        }
        if let Some(provider) = &token_provider {
            for method in ["tokenize_batch", "restore_batch"] {
                if !provider
                    .bind(py)
                    .getattr(method)
                    .is_ok_and(|value| value.is_callable())
                {
                    return Err(PyErr::new::<PyTypeError, _>(
                        "token provider must define tokenize_batch and restore_batch",
                    ));
                }
            }
        }
        Ok(Self {
            key_provider: provider,
            token_provider,
        })
    }

    #[pyo3(signature = (text, findings, config, context=None))]
    fn transform<'py>(
        &self,
        py: Python<'py>,
        text: String,
        findings: Vec<Py<Finding>>,
        config: Py<PyAny>,
        context: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config_value = py_to_json(py, config.bind(py), "")?;
        let config = core::parse_transformation_config(&config_value)
            .map_err(|error| privacy_error(py, error))?;
        let findings = findings
            .iter()
            .map(|finding| finding.bind(py).borrow().to_core())
            .collect::<Vec<_>>();
        let context = context
            .map(|context| py_to_json(py, context.bind(py), ""))
            .transpose()?
            .map(|value| core::parse_privacy_context(&value))
            .transpose()
            .map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .transform_with_context(&text, &findings, &config, context.as_ref())
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, TransformResult::from(result)))
        })
    }

    #[pyo3(signature = (text, config, context=None))]
    fn scan_and_transform<'py>(
        &self,
        py: Python<'py>,
        text: String,
        config: Py<PyAny>,
        context: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config_value = py_to_json(py, config.bind(py), "")?;
        let config = core::parse_scan_and_transform_config(&config_value)
            .map_err(|error| privacy_error(py, error))?;
        let context = context
            .map(|context| py_to_json(py, context.bind(py), ""))
            .transpose()?
            .map(|value| core::parse_privacy_context(&value))
            .transpose()
            .map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .scan_and_transform_with_context(&text, &config, context.as_ref())
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, TransformResult::from(result)))
        })
    }

    fn restore<'py>(
        &self,
        py: Python<'py>,
        text: String,
        context: Py<PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let context = py_to_json(py, context.bind(py), "")?;
        let context =
            core::parse_privacy_context(&context).map_err(|error| privacy_error(py, error))?;
        let key_provider = self
            .key_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        let token_provider = self
            .token_provider
            .as_ref()
            .map(|provider| provider.clone_ref(py));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let manager = core::PrivacyManager::new(PythonKeyProvider {
                provider: key_provider,
            })
            .with_token_provider(PythonTokenProvider {
                provider: token_provider,
            });
            let result = manager
                .restore(&text, &context)
                .await
                .map_err(|error| Python::attach(|py| privacy_error(py, error)))?;
            Python::attach(|py| Py::new(py, RestoreResult::from(result)))
        })
    }
}

fn configuration_conversion_error(py: Python<'_>, path: &str, message: &str) -> PyErr {
    let exception = PyErr::new::<DataFogConfigurationError, _>(message.to_owned());
    let value = exception.value(py);
    let _ = value.setattr("code", "invalid_configuration");
    let _ = value.setattr("reason", "invalid_type");
    let _ = value.setattr("path", path);
    let _ = value.setattr("finding_index", py.None());
    exception
}

fn internal_error(py: Python<'_>, message: &str) -> PyErr {
    let exception = PyErr::new::<DataFogInternalError, _>(message.to_owned());
    let value = exception.value(py);
    let _ = value.setattr("code", "internal_error");
    let _ = value.setattr("reason", py.None());
    let _ = value.setattr("path", py.None());
    let _ = value.setattr("finding_index", py.None());
    exception
}

fn py_to_json(py: Python<'_>, value: &Bound<'_, PyAny>, path: &str) -> PyResult<serde_json::Value> {
    if value.is_none() {
        return Ok(serde_json::Value::Null);
    }
    if value.is_instance_of::<PyBool>() {
        return value.extract::<bool>().map(serde_json::Value::Bool);
    }
    if let Ok(value) = value.extract::<String>() {
        return Ok(serde_json::Value::String(value));
    }
    if let Ok(value) = value.extract::<i64>() {
        return Ok(serde_json::Value::Number(value.into()));
    }
    if let Ok(value) = value.extract::<u64>() {
        return Ok(serde_json::Value::Number(value.into()));
    }
    if let Ok(value) = value.extract::<f64>() {
        return serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                configuration_conversion_error(py, path, "configuration number must be finite")
            });
    }
    if let Ok(dictionary) = value.cast::<PyDict>() {
        let mut object = serde_json::Map::new();
        for (key, value) in dictionary.iter() {
            let key = key.extract::<String>().map_err(|_| {
                configuration_conversion_error(py, path, "configuration keys must be strings")
            })?;
            let child_path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
            object.insert(key, py_to_json(py, &value, &child_path)?);
        }
        return Ok(serde_json::Value::Object(object));
    }
    if let Ok(list) = value.cast::<PyList>() {
        return list
            .iter()
            .enumerate()
            .map(|(index, value)| py_to_json(py, &value, &format!("{path}/{index}")))
            .collect::<PyResult<Vec<_>>>()
            .map(serde_json::Value::Array);
    }
    if let Ok(tuple) = value.cast::<PyTuple>() {
        return tuple
            .iter()
            .enumerate()
            .map(|(index, value)| py_to_json(py, &value, &format!("{path}/{index}")))
            .collect::<PyResult<Vec<_>>>()
            .map(serde_json::Value::Array);
    }
    Err(configuration_conversion_error(
        py,
        path,
        "configuration values must be JSON-compatible",
    ))
}

fn privacy_error(py: Python<'_>, error: core::PrivacyError) -> PyErr {
    let exception = match error.code() {
        core::PrivacyErrorCode::InvalidConfiguration => {
            PyErr::new::<DataFogConfigurationError, _>(error.to_string())
        }
        core::PrivacyErrorCode::InvalidFinding => {
            PyErr::new::<DataFogFindingError, _>(error.to_string())
        }
        core::PrivacyErrorCode::KeyProviderRequired
        | core::PrivacyErrorCode::KeyNotFound
        | core::PrivacyErrorCode::KeyAccessDenied
        | core::PrivacyErrorCode::KeyProviderUnavailable
        | core::PrivacyErrorCode::InvalidKeyMaterial
        | core::PrivacyErrorCode::KeyProviderError
        | core::PrivacyErrorCode::TokenProviderRequired
        | core::PrivacyErrorCode::InvalidToken
        | core::PrivacyErrorCode::UnsupportedTokenVersion
        | core::PrivacyErrorCode::TokenNotFound
        | core::PrivacyErrorCode::TokenExpired
        | core::PrivacyErrorCode::TokenAccessDenied
        | core::PrivacyErrorCode::InvalidTokenMaterial
        | core::PrivacyErrorCode::TokenProviderUnavailable
        | core::PrivacyErrorCode::TokenProviderError
        | core::PrivacyErrorCode::UnsupportedStrategy => {
            PyErr::new::<DataFogKeyProviderError, _>(error.to_string())
        }
        core::PrivacyErrorCode::InternalError => {
            PyErr::new::<DataFogInternalError, _>(error.to_string())
        }
    };
    let value = exception.value(py);
    let _ = value.setattr("code", error.code().as_str());
    let _ = value.setattr(
        "reason",
        error.reason().map(core::PrivacyErrorReason::as_str),
    );
    let _ = value.setattr("path", error.path());
    let _ = value.setattr("finding_index", error.finding_index());
    exception
}

/// Scan text for supported PII findings.
#[pyfunction]
#[pyo3(signature = (text, config=None))]
fn scan(py: Python<'_>, text: &str, config: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<Finding>> {
    let config = if let Some(config) = config {
        let config = py_to_json(py, config, "")?;
        core::parse_scan_config(&config).map_err(|error| privacy_error(py, error))?
    } else {
        core::ScanConfig::default()
    };
    std::panic::catch_unwind(|| {
        core::scan_with_config(text, &config)
            .into_iter()
            .map(Finding::from)
            .collect()
    })
    .map_err(|_| internal_error(py, "unexpected Rust scan failure"))
}

/// Transform explicit findings without scanning implicitly.
#[pyfunction]
fn transform(
    py: Python<'_>,
    text: &str,
    findings: Vec<Py<Finding>>,
    config: &Bound<'_, PyAny>,
) -> PyResult<TransformResult> {
    let config = py_to_json(py, config, "")?;
    let config =
        core::parse_transformation_config(&config).map_err(|error| privacy_error(py, error))?;
    let core_findings: Vec<core::Finding> = findings
        .iter()
        .map(|finding| finding.bind(py).borrow().to_core())
        .collect();
    core::transform(text, &core_findings, &config)
        .map(TransformResult::from)
        .map_err(|error| privacy_error(py, error))
}

/// Scan text and transform the detected findings.
#[pyfunction]
fn scan_and_transform(
    py: Python<'_>,
    text: &str,
    config: &Bound<'_, PyAny>,
) -> PyResult<TransformResult> {
    let config = py_to_json(py, config, "")?;
    let config =
        core::parse_scan_and_transform_config(&config).map_err(|error| privacy_error(py, error))?;
    core::scan_and_transform(text, &config)
        .map(TransformResult::from)
        .map_err(|error| privacy_error(py, error))
}

#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct FieldMapping {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    entity_type: String,
    #[pyo3(get)]
    source: String,
    #[pyo3(get)]
    rule: String,
}
impl From<core::structured::FieldMapping> for FieldMapping {
    fn from(mapping: core::structured::FieldMapping) -> Self {
        Self {
            path: mapping.path,
            entity_type: mapping.entity_type,
            source: mapping.source,
            rule: mapping.rule,
        }
    }
}

#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct StructuredFinding {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    finding: Finding,
}

#[pyclass(frozen, skip_from_py_object)]
struct StructuredScanResult {
    #[pyo3(get)]
    mappings: Vec<FieldMapping>,
    #[pyo3(get)]
    findings: Vec<StructuredFinding>,
}

fn structured_data(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    // The stdlib encoder rejects cycles/deep recursion safely. Check dictionaries
    // and sequences first so it cannot silently coerce keys or tuple values.
    let mut pending = vec![value.clone()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(value) = pending.pop() {
        if value.is_none()
            || value.is_instance_of::<PyBool>()
            || value.is_instance_of::<pyo3::types::PyString>()
            || value.is_instance_of::<pyo3::types::PyInt>()
            || value.is_instance_of::<pyo3::types::PyFloat>()
        {
            continue;
        }
        if !seen.insert(value.as_ptr() as usize) {
            // Repeated containers are valid JSON trees after serialization;
            // the encoder below distinguishes shared references from cycles.
            continue;
        }
        if let Ok(object) = value.cast::<PyDict>() {
            for (key, child) in object.iter() {
                if !key.is_instance_of::<pyo3::types::PyString>() {
                    return Err(privacy_error(py, core::structured::invalid_data()));
                }
                pending.push(child);
            }
        } else if let Ok(array) = value.cast::<PyList>() {
            pending.extend(array.iter());
        } else {
            return Err(privacy_error(py, core::structured::invalid_data()));
        }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("allow_nan", false)?;
    let json: String = py
        .import("json")?
        .call_method("dumps", (value,), Some(&kwargs))
        .and_then(|value| value.extract())
        .map_err(|_| privacy_error(py, core::structured::invalid_data()))?;
    core::structured::parse_document_json(&json).map_err(|error| privacy_error(py, error))
}

fn structured_config(
    py: Python<'_>,
    config: Option<&Bound<'_, PyAny>>,
) -> PyResult<core::structured::StructuredScanConfig> {
    match config {
        Some(config) => core::structured::parse_scan_config(&structured_options(py, config, "")?)
            .map_err(|error| privacy_error(py, error)),
        None => Ok(core::structured::StructuredScanConfig::default()),
    }
}

#[pyfunction]
#[pyo3(signature = (data, config=None))]
fn discover_fields(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    config: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<FieldMapping>> {
    core::structured::discover_fields(&structured_data(py, data)?, &structured_config(py, config)?)
        .map(|mappings| mappings.into_iter().map(FieldMapping::from).collect())
        .map_err(|error| privacy_error(py, error))
}

#[pyfunction]
#[pyo3(signature = (data, config=None))]
fn scan_structured(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    config: Option<&Bound<'_, PyAny>>,
) -> PyResult<StructuredScanResult> {
    let result =
        core::structured::scan(&structured_data(py, data)?, &structured_config(py, config)?)
            .map_err(|error| privacy_error(py, error))?;
    Ok(StructuredScanResult {
        mappings: result
            .mappings
            .into_iter()
            .map(FieldMapping::from)
            .collect(),
        findings: result
            .findings
            .into_iter()
            .map(|located| StructuredFinding {
                path: located.path,
                finding: Finding::from(located.finding),
            })
            .collect(),
    })
}

#[pymethods]
impl StructuredFinding {
    #[new]
    fn new(path: String, finding: PyRef<'_, Finding>) -> Self {
        Self {
            path,
            finding: finding.clone(),
        }
    }
}
impl StructuredFinding {
    fn to_core(&self) -> core::structured::StructuredFinding {
        core::structured::StructuredFinding {
            path: self.path.clone(),
            finding: self.finding.to_core(),
        }
    }
}

#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct StructuredTransformation {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    transformation: Transformation,
}
#[pyclass(frozen, skip_from_py_object)]
struct StructuredTransformResult {
    #[pyo3(get)]
    data: Py<PyAny>,
    #[pyo3(get)]
    transformations: Vec<StructuredTransformation>,
}
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct StructuredRestoration {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    restoration: Restoration,
}
#[pyclass(frozen, skip_from_py_object)]
struct StructuredRestoreResult {
    #[pyo3(get)]
    data: Py<PyAny>,
    #[pyo3(get)]
    restorations: Vec<StructuredRestoration>,
}
fn structured_transform_result(
    py: Python<'_>,
    result: core::structured::StructuredTransformResult,
) -> PyResult<StructuredTransformResult> {
    Ok(StructuredTransformResult {
        data: py
            .import("json")?
            .call_method1("loads", (result.data.to_string(),))?
            .unbind(),
        transformations: result
            .transformations
            .into_iter()
            .map(|record| StructuredTransformation {
                path: record.path,
                transformation: Transformation::from(record.transformation),
            })
            .collect(),
    })
}
fn structured_restore_result(
    py: Python<'_>,
    result: core::structured::StructuredRestoreResult,
) -> PyResult<StructuredRestoreResult> {
    Ok(StructuredRestoreResult {
        data: py
            .import("json")?
            .call_method1("loads", (result.data.to_string(),))?
            .unbind(),
        restorations: result
            .restorations
            .into_iter()
            .map(|record| StructuredRestoration {
                path: record.path,
                restoration: Restoration::from(record.restoration),
            })
            .collect(),
    })
}

#[pyfunction]
fn transform_structured(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    findings: Vec<Py<StructuredFinding>>,
    config: &Bound<'_, PyAny>,
) -> PyResult<StructuredTransformResult> {
    let data = structured_data(py, data)?;
    let config = core::parse_transformation_config(&structured_options(py, config, "")?)
        .map_err(|error| privacy_error(py, error))?;
    let findings: Vec<_> = findings
        .iter()
        .map(|finding| finding.bind(py).borrow().to_core())
        .collect();
    let result = core::structured::transform(&data, &findings, &config)
        .map_err(|error| privacy_error(py, error))?;
    structured_transform_result(py, result)
}

#[pyfunction]
fn scan_and_transform_structured(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    config: &Bound<'_, PyAny>,
) -> PyResult<StructuredTransformResult> {
    let data = structured_data(py, data)?;
    let config =
        core::structured::parse_scan_and_transform_config(&structured_options(py, config, "")?)
            .map_err(|error| privacy_error(py, error))?;
    let result = core::structured::scan_and_transform(&data, &config)
        .map_err(|error| privacy_error(py, error))?;
    structured_transform_result(py, result)
}

#[pymodule]
fn datafog_core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "DataFogConfigurationError",
        module.py().get_type::<DataFogConfigurationError>(),
    )?;
    module.add(
        "DataFogFindingError",
        module.py().get_type::<DataFogFindingError>(),
    )?;
    module.add(
        "DataFogInternalError",
        module.py().get_type::<DataFogInternalError>(),
    )?;
    module.add(
        "DataFogKeyProviderError",
        module.py().get_type::<DataFogKeyProviderError>(),
    )?;
    module.add_class::<FieldMapping>()?;
    module.add_class::<StructuredFinding>()?;
    module.add_class::<StructuredScanResult>()?;
    module.add_class::<StructuredTransformation>()?;
    module.add_class::<StructuredTransformResult>()?;
    module.add_class::<StructuredRestoration>()?;
    module.add_class::<StructuredRestoreResult>()?;
    module.add_function(wrap_pyfunction!(transform_structured, module)?)?;
    module.add_function(wrap_pyfunction!(scan_and_transform_structured, module)?)?;
    module.add_function(wrap_pyfunction!(discover_fields, module)?)?;
    module.add_function(wrap_pyfunction!(scan_structured, module)?)?;
    module.add_class::<TextRange>()?;
    module.add_class::<Finding>()?;
    module.add_class::<Transformation>()?;
    module.add_class::<TransformResult>()?;
    module.add_class::<Restoration>()?;
    module.add_class::<RestoreResult>()?;
    module.add_class::<PrivacyManager>()?;
    module.add_function(wrap_pyfunction!(scan, module)?)?;
    module.add_function(wrap_pyfunction!(transform, module)?)?;
    module.add_function(wrap_pyfunction!(scan_and_transform, module)?)?;
    Ok(())
}

impl From<core::Restoration> for Restoration {
    fn from(record: core::Restoration) -> Self {
        Restoration {
            source_byte_range: record.source_byte_range.into(),
            source_codepoint_range: record.source_codepoint_range.into(),
            output_byte_range: record.output_byte_range.into(),
            output_codepoint_range: record.output_codepoint_range.into(),
            token_ref: record.token_ref,
            resolved_token_version: record.resolved_token_version,
        }
    }
}

fn structured_options(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    path: &str,
) -> PyResult<serde_json::Value> {
    structured_data(py, value).map_err(|_| {
        configuration_conversion_error(
            py,
            path,
            "structured request options must be JSON-compatible",
        )
    })
}
