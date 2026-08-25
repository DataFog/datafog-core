use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// An entity detected in input text.
#[pyclass(frozen, skip_from_py_object)]
#[derive(Clone)]
struct Entity {
    #[pyo3(get)]
    label: String,

    #[pyo3(get)]
    text: String,

    #[pyo3(get)]
    start: usize,

    #[pyo3(get)]
    end: usize,
}

impl From<datafog_scan_core::Entity> for Entity {
    fn from(entity: datafog_scan_core::Entity) -> Self {
        Self {
            label: entity.label,
            text: entity.text,
            start: entity.start,
            end: entity.end,
        }
    }
}

#[pymethods]
impl Entity {
    fn __repr__(&self) -> String {
        format!(
            "Entity(label={:?}, text={:?}, start={}, end={})",
            self.label, self.text, self.start, self.end
        )
    }

    fn __eq__(&self, other: PyRef<'_, Entity>) -> bool {
        self.label == other.label
            && self.text == other.text
            && self.start == other.start
            && self.end == other.end
    }
}

/// Scan text for supported PII entities.
#[pyfunction]
fn scan(text: &str) -> PyResult<Vec<Entity>> {
    std::panic::catch_unwind(|| {
        datafog_scan_core::scan(text)
            .into_iter()
            .map(Entity::from)
            .collect()
    })
    .map_err(|_| PyRuntimeError::new_err("unexpected Rust scan failure"))
}

#[pymodule]
fn datafog_rs(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Entity>()?;
    module.add_function(wrap_pyfunction!(scan, module)?)?;
    Ok(())
}
