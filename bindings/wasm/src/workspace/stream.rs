//! Transport/lifetime ownership only; all decoding and marker decisions are Core-owned.
use super::*;
use serde::Serialize;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

#[derive(Default)]
pub(super) struct Registry(Vec<Weak<RefCell<Adapter>>>);
enum Implementation {
    Buffered(workspace::BufferedRestore),
    Progressive(workspace::ProgressiveRestore),
}
struct Adapter {
    inner: Implementation,
    encoding: workspace::RestoreEncoding,
}
impl Adapter {
    fn close(&mut self) {
        match &mut self.inner {
            Implementation::Buffered(stream) => stream.close(),
            Implementation::Progressive(stream) => stream.close(),
        }
    }
}
impl Registry {
    pub(super) fn scoped(
        &mut self,
        owner: &Workspace,
        context: workspace::PublicId,
        encoding: &str,
        progressive: bool,
    ) -> Result<NativeRestoreStream, JsValue> {
        let encoding = parse_encoding(encoding)?;
        let implementation = if progressive {
            Implementation::Progressive(
                owner
                    .progressive_restore_in_conversation(context, encoding)
                    .map_err(from_error)?,
            )
        } else {
            Implementation::Buffered(
                owner
                    .buffered_restore_in_conversation(context, encoding)
                    .map_err(from_error)?,
            )
        };
        self.track(implementation, encoding)
    }
    fn track(
        &mut self,
        inner: Implementation,
        encoding: workspace::RestoreEncoding,
    ) -> Result<NativeRestoreStream, JsValue> {
        self.0.retain(|stream| stream.strong_count() != 0);
        self.0
            .try_reserve(1)
            .map_err(|_| failure(ErrorCode::ResourceLimit))?;
        let inner = Rc::new(RefCell::new(Adapter { inner, encoding }));
        self.0.push(Rc::downgrade(&inner));
        Ok(NativeRestoreStream { inner })
    }
    pub(super) fn buffered(
        &mut self,
        owner: &Workspace,
        encoding: &str,
    ) -> Result<NativeRestoreStream, JsValue> {
        let encoding = parse_encoding(encoding)?;
        self.track(
            Implementation::Buffered(owner.buffered_restore(encoding).map_err(from_error)?),
            encoding,
        )
    }
    pub(super) fn progressive(
        &mut self,
        owner: &Workspace,
        encoding: &str,
    ) -> Result<NativeRestoreStream, JsValue> {
        let encoding = parse_encoding(encoding)?;
        self.track(
            Implementation::Progressive(owner.progressive_restore(encoding).map_err(from_error)?),
            encoding,
        )
    }
    pub(super) fn close(&mut self) {
        for stream in self.0.drain(..).filter_map(|stream| stream.upgrade()) {
            stream.borrow_mut().close();
        }
    }
}
fn parse_encoding(encoding: &str) -> Result<workspace::RestoreEncoding, JsValue> {
    match encoding {
        "utf8" => Ok(workspace::RestoreEncoding::Utf8),
        "utf16" => Ok(workspace::RestoreEncoding::Utf16),
        _ => Err(failure(ErrorCode::InvalidEncoding)),
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    bytes: usize,
    codepoints: usize,
    utf16: usize,
    restored_markers: usize,
}
impl From<workspace::RestoreProgress> for Progress {
    fn from(value: workspace::RestoreProgress) -> Self {
        Self {
            bytes: value.bytes,
            codepoints: value.codepoints,
            utf16: value.utf16,
            restored_markers: value.restored_markers,
        }
    }
}
#[derive(Serialize)]
struct Chunk<'a> {
    text: &'a str,
    delivered: Progress,
    complete: bool,
}
fn value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    // Borrow private strings directly into JS instead of building a reallocating JSON buffer.
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|_| failure(ErrorCode::ResourceLimit))
}
fn chunk(chunk: workspace::RestoreChunk) -> Result<JsValue, JsValue> {
    value(&Chunk {
        text: &chunk.text,
        delivered: chunk.delivered.into(),
        complete: chunk.complete,
    })
}
fn stream_error(failure: workspace::StreamFailure) -> JsValue {
    let issues: Vec<_> = failure
        .error
        .marker_issues
        .iter()
        .map(marker_issue_value)
        .collect();
    value(&json!({"code":failure.error.code.as_str(),"message":failure.error.code.as_str(),"findingIndex":failure.error.finding_index,"inputIndex":failure.error.input_index,"markerIssues":issues,"reviewConflicts":failure.error.review_conflicts,"delivered":Progress::from(failure.delivered),"complete":false})).unwrap_or_else(|failure|failure)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    text: Option<&'a str>,
    restored_markers: usize,
    issues: Vec<serde_json::Value>,
}

enum Output {
    Empty,
    Buffered(workspace::RestoreReport),
    Progressive(workspace::RestoreChunk),
}
enum CallFailure {
    Workspace(WorkspaceError),
    Progressive(workspace::StreamFailure),
}
fn finish(result: Result<Output, CallFailure>) -> Result<JsValue, JsValue> {
    match result {
        Ok(Output::Empty) => Ok(JsValue::UNDEFINED),
        Ok(Output::Buffered(report)) => value(&Report {
            text: report.text.as_deref(),
            restored_markers: report.restored_markers,
            issues: report.issues.iter().map(marker_issue_value).collect(),
        }),
        Ok(Output::Progressive(result)) => chunk(result),
        Err(CallFailure::Workspace(error)) => Err(from_error(error)),
        Err(CallFailure::Progressive(error)) => Err(stream_error(error)),
    }
}

/// Factory-only owned adapter. Workspace close/head changes clear retained children.
#[wasm_bindgen]
pub struct NativeRestoreStream {
    inner: Rc<RefCell<Adapter>>,
}
#[wasm_bindgen]
impl NativeRestoreStream {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        // Release native ownership guards before converting any value/error into JS.
        let result = {
            let mut adapter = self.inner.borrow_mut();
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .feed(bytes)
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .feed(bytes)
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    pub fn feed_utf16(&mut self, units: &[u16]) -> Result<JsValue, JsValue> {
        let result = {
            let mut adapter = self.inner.borrow_mut();
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .feed_utf16(units)
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .feed_utf16(units)
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    pub fn finalize(&mut self) -> Result<JsValue, JsValue> {
        let result = {
            let mut adapter = self.inner.borrow_mut();
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .finalize()
                    .map(Output::Buffered)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .finalize()
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    pub fn reset(&mut self) -> Result<JsValue, JsValue> {
        let result = {
            let mut adapter = self.inner.borrow_mut();
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .reset()
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .reset()
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    /// Terminate malformed transport input without letting wasm-bindgen coerce values.
    pub fn reject_encoding(&mut self) -> Result<JsValue, JsValue> {
        let encoding = self.inner.borrow().encoding;
        match encoding {
            workspace::RestoreEncoding::Utf8 => self.feed_utf16(&[]),
            workspace::RestoreEncoding::Utf16 => self.feed(&[]),
        }
    }
    pub fn close(&mut self) {
        self.inner.borrow_mut().close();
    }
}
