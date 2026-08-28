use ::datafog_core as core;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

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
    fn __repr__(&self) -> String {
        format!("TextRange(start={}, end={})", self.start, self.end)
    }

    fn __eq__(&self, other: PyRef<'_, TextRange>) -> bool {
        self == &*other
    }
}

/// A piece of potentially sensitive content detected in input text.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
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

/// Scan text for supported PII findings.
#[pyfunction]
fn scan(text: &str) -> PyResult<Vec<Finding>> {
    std::panic::catch_unwind(|| core::scan(text).into_iter().map(Finding::from).collect())
        .map_err(|_| PyRuntimeError::new_err("unexpected Rust scan failure"))
}

#[pymodule]
fn datafog_core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<TextRange>()?;
    module.add_class::<Finding>()?;
    module.add_function(wrap_pyfunction!(scan, module)?)?;
    Ok(())
}
