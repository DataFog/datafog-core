//! Native ownership and transport only; Core owns all decoding and marker decisions.
use super::*;
use napi::bindgen_prelude::Uint16Array;
use serde::Serialize;
use std::sync::{Arc, Mutex, Weak};

#[derive(Default)]
pub(super) struct Registry(Vec<Weak<Mutex<Adapter>>>);
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
    fn track(
        &mut self,
        inner: Implementation,
        encoding: workspace::RestoreEncoding,
    ) -> Result<NativeRestoreStream, Error> {
        self.0.retain(|stream| stream.strong_count() != 0);
        self.0
            .try_reserve(1)
            .map_err(|_| failure(ErrorCode::ResourceLimit))?;
        let inner = Arc::new(Mutex::new(Adapter { inner, encoding }));
        self.0.push(Arc::downgrade(&inner));
        Ok(NativeRestoreStream { inner })
    }
    pub(super) fn buffered(
        &mut self,
        owner: &Workspace,
        encoding: &str,
    ) -> Result<NativeRestoreStream, Error> {
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
    ) -> Result<NativeRestoreStream, Error> {
        let encoding = parse_encoding(encoding)?;
        self.track(
            Implementation::Progressive(owner.progressive_restore(encoding).map_err(from_error)?),
            encoding,
        )
    }
    pub(super) fn close(&mut self) {
        for stream in self.0.drain(..).filter_map(|stream| stream.upgrade()) {
            stream
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .close();
        }
    }
}
fn parse_encoding(encoding: &str) -> Result<workspace::RestoreEncoding, Error> {
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
fn value<T: Serialize>(value: &T) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|_| failure(ErrorCode::ResourceLimit))
}
fn stream_error(failure: workspace::StreamFailure) -> Error {
    let issues: Vec<_> = failure
        .error
        .marker_issues
        .iter()
        .map(marker_issue_value)
        .collect();
    Error::new(Status::GenericFailure, json!({"code":failure.error.code.as_str(),"message":failure.error.code.as_str(),"findingIndex":failure.error.finding_index,"inputIndex":failure.error.input_index,"markerIssues":issues,"reviewConflicts":failure.error.review_conflicts,"delivered":Progress::from(failure.delivered),"complete":false}).to_string())
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
fn finish(result: Result<Output, CallFailure>) -> Result<String, Error> {
    match result {
        Ok(Output::Empty) => Ok("null".to_owned()),
        Ok(Output::Buffered(report)) => value(&Report {
            text: report.text.as_deref(),
            restored_markers: report.restored_markers,
            issues: report.issues.iter().map(marker_issue_value).collect(),
        }),
        Ok(Output::Progressive(chunk)) => value(&Chunk {
            text: &chunk.text,
            delivered: chunk.delivered.into(),
            complete: chunk.complete,
        }),
        Err(CallFailure::Workspace(error)) => Err(from_error(error)),
        Err(CallFailure::Progressive(error)) => Err(stream_error(error)),
    }
}

/// Factory-only private stream capability; dropping the owner clears retained children.
#[napi(js_name = "NativeRestoreStream")]
pub struct NativeRestoreStream {
    inner: Arc<Mutex<Adapter>>,
}
#[napi]
impl NativeRestoreStream {
    #[napi(js_name = "feed")]
    pub fn feed(&mut self, bytes: Buffer) -> Result<String, Error> {
        let result = {
            let mut adapter = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .feed(&bytes)
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .feed(&bytes)
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    #[napi(js_name = "feed_utf16")]
    pub fn feed_utf16(&mut self, units: Uint16Array) -> Result<String, Error> {
        let result = {
            let mut adapter = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &mut adapter.inner {
                Implementation::Buffered(stream) => stream
                    .feed_utf16(&units)
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                Implementation::Progressive(stream) => stream
                    .feed_utf16(&units)
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    #[napi(js_name = "finalize")]
    pub fn finalize(&mut self) -> Result<String, Error> {
        let result = {
            let mut adapter = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    #[napi(js_name = "reset")]
    pub fn reset(&mut self) -> Result<String, Error> {
        let result = {
            let mut adapter = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    #[napi(js_name = "reject_encoding")]
    pub fn reject_encoding(&mut self) -> Result<String, Error> {
        let result = {
            let mut adapter = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let encoding = adapter.encoding;
            match (&mut adapter.inner, encoding) {
                (Implementation::Buffered(stream), workspace::RestoreEncoding::Utf8) => stream
                    .feed_utf16(&[])
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                (Implementation::Buffered(stream), workspace::RestoreEncoding::Utf16) => stream
                    .feed(&[])
                    .map(|()| Output::Empty)
                    .map_err(CallFailure::Workspace),
                (Implementation::Progressive(stream), workspace::RestoreEncoding::Utf8) => stream
                    .feed_utf16(&[])
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
                (Implementation::Progressive(stream), workspace::RestoreEncoding::Utf16) => stream
                    .feed(&[])
                    .map(Output::Progressive)
                    .map_err(CallFailure::Progressive),
            }
        };
        finish(result)
    }
    #[napi(js_name = "close")]
    pub fn close(&mut self) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .close();
    }
}
