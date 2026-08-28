use ::datafog_core as core;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyDict};

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

fn validate_config_keys(config: &Bound<'_, PyDict>, allowed: &[&str]) -> PyResult<()> {
    for (key, _) in config.iter() {
        let key = key
            .extract::<String>()
            .map_err(|_| PyValueError::new_err("configuration keys must be strings"))?;
        if !allowed.contains(&key.as_str()) {
            return Err(PyValueError::new_err(format!(
                "unexpected configuration field: {key}"
            )));
        }
    }
    Ok(())
}

fn required_item<'py>(config: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    config
        .get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("missing configuration field: {key}")))
}

fn parse_strategy(config: &Bound<'_, PyAny>) -> PyResult<core::TransformationStrategy> {
    let config = config
        .cast::<PyDict>()
        .map_err(|_| PyValueError::new_err("strategy configuration must be a dict"))?;
    let strategy = required_item(config, "strategy")?
        .extract::<String>()
        .map_err(|_| PyValueError::new_err("strategy must be a string"))?;

    match strategy.as_str() {
        "redact" => {
            validate_config_keys(config, &["strategy"])?;
            Ok(core::TransformationStrategy::Redact)
        }
        "remove" => {
            validate_config_keys(config, &["strategy"])?;
            Ok(core::TransformationStrategy::Remove)
        }
        "mask" => {
            validate_config_keys(config, &["strategy", "character", "reveal"])?;
            let character = config
                .get_item("character")?
                .map(|value| value.extract::<String>())
                .transpose()
                .map_err(|_| PyValueError::new_err("mask character must be a string"))?
                .unwrap_or_else(|| "*".to_owned());
            let mut characters = character.chars();
            let character = characters
                .next()
                .filter(|_| characters.next().is_none())
                .ok_or_else(|| {
                    PyValueError::new_err("mask character must contain exactly one code point")
                })?;

            let reveal = match config.get_item("reveal")? {
                None => core::MaskReveal::None,
                Some(reveal) => {
                    let reveal = reveal.cast::<PyDict>().map_err(|_| {
                        PyValueError::new_err("mask reveal configuration must be a dict")
                    })?;
                    validate_config_keys(reveal, &["direction", "count"])?;
                    let direction = required_item(reveal, "direction")?
                        .extract::<String>()
                        .map_err(|_| PyValueError::new_err("reveal direction must be a string"))?;
                    let count = required_item(reveal, "count")?;
                    if count.is_instance_of::<PyBool>() {
                        return Err(PyValueError::new_err(
                            "reveal count must be a non-negative integer",
                        ));
                    }
                    let count = count.extract::<usize>().map_err(|_| {
                        PyValueError::new_err("reveal count must be a non-negative integer")
                    })?;
                    match direction.as_str() {
                        "first" => core::MaskReveal::First(count),
                        "last" => core::MaskReveal::Last(count),
                        _ => {
                            return Err(PyValueError::new_err(
                                "reveal direction must be 'first' or 'last'",
                            ));
                        }
                    }
                }
            };
            core::MaskConfig::new(character, reveal)
                .map(core::TransformationStrategy::Mask)
                .map_err(|_| {
                    PyValueError::new_err(
                        "mask character must not be whitespace or a control character",
                    )
                })
        }
        _ => Err(PyValueError::new_err(
            "strategy must be 'redact', 'mask', or 'remove'",
        )),
    }
}

/// Scan text for supported PII findings.
#[pyfunction]
fn scan(text: &str) -> PyResult<Vec<Finding>> {
    std::panic::catch_unwind(|| core::scan(text).into_iter().map(Finding::from).collect())
        .map_err(|_| PyRuntimeError::new_err("unexpected Rust scan failure"))
}

/// Transform explicit findings without scanning implicitly.
#[pyfunction]
fn transform(
    py: Python<'_>,
    text: &str,
    findings: Vec<Py<Finding>>,
    config: &Bound<'_, PyAny>,
) -> PyResult<TransformResult> {
    let strategy = parse_strategy(config)?;
    let core_findings: Vec<core::Finding> = findings
        .iter()
        .map(|finding| finding.bind(py).borrow().to_core())
        .collect();
    core::transform(text, &core_findings, strategy)
        .map(TransformResult::from)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Scan text and transform the detected findings.
#[pyfunction]
fn scan_and_transform(text: &str, config: &Bound<'_, PyAny>) -> PyResult<TransformResult> {
    core::scan_and_transform(text, parse_strategy(config)?)
        .map(TransformResult::from)
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

#[pymodule]
fn datafog_core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<TextRange>()?;
    module.add_class::<Finding>()?;
    module.add_class::<Transformation>()?;
    module.add_class::<TransformResult>()?;
    module.add_function(wrap_pyfunction!(scan, module)?)?;
    module.add_function(wrap_pyfunction!(transform, module)?)?;
    module.add_function(wrap_pyfunction!(scan_and_transform, module)?)?;
    Ok(())
}
