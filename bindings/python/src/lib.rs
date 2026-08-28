use ::datafog_core as core;
use pyo3::create_exception;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict, PyList, PyTuple};

create_exception!(datafog_core, DataFogConfigurationError, PyValueError);
create_exception!(datafog_core, DataFogFindingError, PyValueError);
create_exception!(datafog_core, DataFogInternalError, PyRuntimeError);

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
    finding: Finding,

    #[pyo3(get)]
    strategy: String,

    #[pyo3(get)]
    replacement: String,

    #[pyo3(get)]
    output_byte_range: TextRange,

    #[pyo3(get)]
    output_codepoint_range: TextRange,
}

impl From<core::Transformation> for Transformation {
    fn from(transformation: core::Transformation) -> Self {
        Self {
            finding: transformation.finding.into(),
            strategy: match transformation.strategy {
                core::TransformationStrategy::Redact => "redact".to_owned(),
                core::TransformationStrategy::Remove => "remove".to_owned(),
                core::TransformationStrategy::Mask(_) => "mask".to_owned(),
            },
            replacement: transformation.replacement,
            output_byte_range: transformation.output_byte_range.into(),
            output_codepoint_range: transformation.output_codepoint_range.into(),
        }
    }
}

#[pymethods]
impl Transformation {
    fn __eq__(&self, other: PyRef<'_, Transformation>) -> bool {
        self.finding.entity_type == other.finding.entity_type
            && self.finding.matched_text == other.finding.matched_text
            && self.finding.byte_range == other.finding.byte_range
            && self.finding.codepoint_range == other.finding.codepoint_range
            && self.finding.confidence == other.finding.confidence
            && self.finding.detector_name == other.finding.detector_name
            && self.finding.detector_version == other.finding.detector_version
            && self.strategy == other.strategy
            && self.replacement == other.replacement
            && self.output_byte_range == other.output_byte_range
            && self.output_codepoint_range == other.output_codepoint_range
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
    module.add_class::<TextRange>()?;
    module.add_class::<Finding>()?;
    module.add_class::<Transformation>()?;
    module.add_class::<TransformResult>()?;
    module.add_function(wrap_pyfunction!(scan, module)?)?;
    module.add_function(wrap_pyfunction!(transform, module)?)?;
    module.add_function(wrap_pyfunction!(scan_and_transform, module)?)?;
    Ok(())
}
