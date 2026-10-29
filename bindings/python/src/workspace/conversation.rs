//! Exact Python transport values; conversation/admission decisions stay in Core.
use super::*;
use pyo3::types::{PyInt, PyString};

fn object<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    names: &[&str],
    code: w::ErrorCode,
) -> PyResult<Bound<'py, PyDict>> {
    let invalid = || failure(py, code);
    let object = value.cast_exact::<PyDict>().map_err(|_| invalid())?;
    if object.len() != names.len()
        || object
            .keys()
            .iter()
            .any(|key| !key.is_exact_instance_of::<PyString>())
    {
        return Err(invalid());
    }
    for name in names {
        if !object.contains(*name)? {
            return Err(invalid());
        }
    }
    Ok(object.clone())
}
fn required<'py>(
    py: Python<'py>,
    object: &Bound<'py, PyDict>,
    name: &str,
    code: w::ErrorCode,
) -> PyResult<Bound<'py, PyAny>> {
    object.get_item(name)?.ok_or_else(|| failure(py, code))
}
fn public_id(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    code: w::ErrorCode,
) -> PyResult<w::PublicId> {
    let invalid = || failure(py, code);
    if let Ok(bytes) = value.cast_exact::<PyBytes>() {
        return bytes.as_bytes().try_into().map_err(|_| {
            // Match typed-byte IDs in the existing WASM transport; malformed
            // intent identifiers retain the dedicated intent category.
            failure(
                py,
                if code == w::ErrorCode::InvalidConversation {
                    w::ErrorCode::InvalidId
                } else {
                    code
                },
            )
        });
    }
    let values = value.cast_exact::<PyList>().map_err(|_| invalid())?;
    if values.len() != 16 {
        return Err(invalid());
    }
    let mut result = [0; 16];
    for (index, item) in values.iter().enumerate() {
        if !item.is_exact_instance_of::<PyInt>() {
            return Err(invalid());
        }
        result[index] = item.extract::<u8>().map_err(|_| invalid())?;
    }
    Ok(result)
}
fn string(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let code = w::ErrorCode::InvalidFinding;
    let value = value
        .cast_exact::<PyString>()
        .map_err(|_| failure(py, code))?;
    let text = value.to_str().map_err(|_| failure(py, code))?;
    material_limit(py, text.len())?;
    Ok(text.to_owned())
}
pub(super) fn intent(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<w::SendIntentId> {
    let code = w::ErrorCode::InvalidSendIntent;
    let object = object(py, value, &["preparationId", "inputIndex"], code)?;
    let preparation_id = public_id(py, &required(py, &object, "preparationId", code)?, code)?;
    let index = required(py, &object, "inputIndex", code)?;
    if !index.is_exact_instance_of::<PyInt>() {
        return Err(failure(py, code));
    }
    let input_index = index.extract::<usize>().map_err(|_| failure(py, code))?;
    if input_index >= w::MAX_INPUTS {
        return Err(failure(py, code));
    }
    Ok(w::SendIntentId {
        preparation_id,
        input_index,
    })
}
pub(super) fn inputs(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Vec<(ReviewOwned, w::Conversation)>> {
    let code = w::ErrorCode::InvalidConversation;
    let values = value
        .cast_exact::<PyList>()
        .map_err(|_| failure(py, w::ErrorCode::InvalidBatch))?;
    if values.len() > w::MAX_INPUTS {
        return Err(failure(py, w::ErrorCode::ResourceLimit));
    }
    let mut result = Vec::with_capacity(values.len());
    let mut bytes = 0usize;
    for (index, value) in values.iter().enumerate() {
        let row = (|| -> PyResult<(ReviewOwned, w::Conversation)> {
            let item = object(py, &value, &["input", "conversation"], code)?;
            let input = required(py, &item, "input", code)?;
            let input = input
                .extract::<PyRef<'_, WorkspaceReviewInput>>()
                .map_err(|_| failure(py, code))?;
            bytes = bytes.saturating_add(input.text.len());
            for finding in input.scanner.iter().chain(&input.user) {
                bytes = bytes.saturating_add(finding_bytes(finding));
            }
            for correction in &input.corrections {
                bytes = bytes
                    .saturating_add(finding_bytes(&correction.replacement))
                    .saturating_add(correction.candidate_key.len());
            }
            material_limit(py, bytes)?;
            let context = object(
                py,
                &required(py, &item, "conversation", code)?,
                &["id", "surface", "siteId", "siteIdByteBudget"],
                code,
            )?;
            let id = public_id(py, &required(py, &context, "id", code)?, code)?;
            let surface = string(py, &required(py, &context, "surface", code)?)?;
            let site = required(py, &context, "siteId", code)?;
            let site_id = if site.is_none() {
                None
            } else {
                Some(string(py, &site)?)
            };
            bytes = bytes
                .saturating_add(surface.len())
                .saturating_add(site_id.as_ref().map_or(0, String::len));
            material_limit(py, bytes)?;
            let budget = required(py, &context, "siteIdByteBudget", code)?;
            if !budget.is_exact_instance_of::<PyInt>() {
                return Err(failure(py, code));
            }
            let site_id_byte_budget = budget.extract::<usize>().map_err(|_| failure(py, code))?;
            Ok((
                input.owned(),
                w::Conversation {
                    id,
                    surface,
                    site_id,
                    site_id_byte_budget,
                },
            ))
        })();
        match row {
            Ok(row) => result.push(row),
            Err(error) => {
                error.value(py).setattr("input_index", index)?;
                return Err(error);
            }
        }
    }
    Ok(result)
}
