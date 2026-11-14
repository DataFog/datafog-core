//! Embedded statements-v1 CNN: statement recipient names and mailing-address
//! blocks. Weights are compiled into the extension, so there is no model
//! download, no files, no subprocess and no inference runtime.
//!
//! The browser binding owns this in a Worker. Python has no UI thread to
//! protect, so the detector is exposed directly and callers are expected to keep
//! it on a worker thread for sustained throughput.

use super::*;
use ::datafog_core::cnn::CnnDetector;
use models::{ModelScanResult, model_error};

/// Explicit embedded detector. Construction authenticates the embedded weights
/// once; `open()` is the only way to create one.
#[pyclass(frozen, skip_from_py_object)]
pub(super) struct EmbeddedCnnDetector {
    detector: CnnDetector,
}

#[pymethods]
impl EmbeddedCnnDetector {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            detector: CnnDetector::open().map_err(|error| model_error(py, error))?,
        })
    }

    /// Model identity, digest, labels, limits and thresholds.
    fn describe(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        models::json_object(py, &self.detector.describe())
    }

    /// Rules plus requested model labels. Omitting `config` equals `{}`: every
    /// enabled rule detector plus the model's remaining labels.
    ///
    /// Inference is synchronous and in-process; `cancellation` cannot interrupt
    /// it, unlike `ModelSession.scan()`. Errors never return partial detections.
    #[pyo3(signature = (text, config=None))]
    fn scan(
        &self,
        py: Python<'_>,
        text: String,
        config: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<ModelScanResult> {
        let config = match config {
            Some(config) => py_to_json(py, config, "")?,
            None => serde_json::Value::Object(serde_json::Map::new()),
        };
        self.detector
            .scan(&text, &config)
            .map(ModelScanResult::from)
            .map_err(|error| model_error(py, error))
    }

    /// Model-only findings, with no rule detectors merged in.
    fn scan_model(&self, py: Python<'_>, text: String) -> PyResult<ModelScanResult> {
        self.detector
            .scan_model(&text)
            .map(ModelScanResult::from)
            .map_err(|error| model_error(py, error))
    }

    /// Validate the same entity selection while skipping weight authentication
    /// and inference. Model-only requests return no findings.
    #[staticmethod]
    fn scan_disabled(
        py: Python<'_>,
        text: String,
        config: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<ModelScanResult> {
        let config = match config {
            Some(config) => py_to_json(py, config, "")?,
            None => serde_json::Value::Object(serde_json::Map::new()),
        };
        CnnDetector::scan_disabled(&text, &config)
            .map(ModelScanResult::from)
            .map_err(|error| model_error(py, error))
    }
}
