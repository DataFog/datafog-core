//! Transport-only adapters for the shared content-free operation contract.
use super::*;
use pyo3::types::{PyInt, PyString};

pub(super) fn context(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<w::OperationContext> {
    let invalid = || failure(py, w::ErrorCode::InvalidOperation);
    let object = value.cast_exact::<PyDict>().map_err(|_| invalid())?;
    if object.len() != 2
        || object
            .keys()
            .iter()
            .any(|key| !key.is_exact_instance_of::<PyString>())
    {
        return Err(invalid());
    }
    let bytes = object.get_item("id")?.ok_or_else(invalid)?;
    let bytes = bytes
        .cast_exact::<PyBytes>()
        .map_err(|_| failure(py, w::ErrorCode::InvalidId))?;
    let operation_id = id(py, bytes.as_bytes())?;
    let time = object.get_item("occurredAtMs")?.ok_or_else(invalid)?;
    if !time.is_exact_instance_of::<PyInt>() {
        return Err(invalid());
    }
    let occurred_at_ms = time.extract::<u64>().map_err(|_| invalid())?;
    if occurred_at_ms > w::MAX_OPERATION_TIME_MS {
        return Err(invalid());
    }
    Ok(w::OperationContext {
        id: operation_id,
        occurred_at_ms,
    })
}

pub(super) fn outcome(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<w::OperationOutcome> {
    let invalid = || failure(py, w::ErrorCode::InvalidOperation);
    let object = value.cast_exact::<PyDict>().map_err(|_| invalid())?;
    if object.len() != 2
        || object
            .keys()
            .iter()
            .any(|key| !key.is_exact_instance_of::<PyString>())
    {
        return Err(invalid());
    }
    let kind = object.get_item("kind")?.ok_or_else(invalid)?;
    let kind = kind.cast_exact::<PyString>().map_err(|_| invalid())?;
    let kind = match kind.to_str().map_err(|_| invalid())? {
        "copy_confirmed" => w::OutcomeKind::CopyConfirmed,
        "user_cancelled" => w::OutcomeKind::UserCancelled,
        "blocked_input" => w::OutcomeKind::BlockedInput,
        "restore_blocked" => w::OutcomeKind::RestoreBlocked,
        _ => return Err(invalid()),
    };
    let count = object.get_item("issueCount")?.ok_or_else(invalid)?;
    if !count.is_exact_instance_of::<PyInt>() {
        return Err(invalid());
    }
    let issue_count = count.extract::<usize>().map_err(|_| invalid())?;
    if issue_count > w::MAX_FINDINGS {
        return Err(invalid());
    }
    Ok(w::OperationOutcome { kind, issue_count })
}

pub(super) fn record(value: &w::OperationRecord) -> Value {
    json!({"id":value.id,"kind":value.kind.as_str(),"occurredAtMs":value.occurred_at_ms,
        "preparedInputs":value.prepared_inputs,"transformedSpans":value.transformed_spans})
}

pub(super) fn counts(value: w::OperationCounts) -> Value {
    json!({"operations":value.operations,"preparedInputs":value.prepared_inputs,
        "transformedSpans":value.transformed_spans,"confirmedHandoffs":value.confirmed_handoffs,
        "reportedBlockedAttempts":value.reported_blocked_attempts})
}

// Bounds stay Core-owned; reject Python coercion before entering the native call.
pub(super) fn page_index(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
    let invalid = || failure(py, w::ErrorCode::ResourceLimit);
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(invalid());
    }
    value.extract::<usize>().map_err(|_| invalid())
}
