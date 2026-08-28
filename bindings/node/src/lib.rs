//! Node binding for datafog-core.

use napi::{Error, Status};
use napi_derive::napi;

#[napi(object, object_from_js = false)]
pub struct TextRange {
    #[napi(readonly)]
    pub start: u32,

    #[napi(readonly)]
    pub end: u32,
}

#[napi(object, object_from_js = false)]
pub struct Finding {
    #[napi(readonly, ts_type = "EntityType")]
    pub entity_type: String,

    #[napi(readonly)]
    pub matched_text: String,

    #[napi(readonly)]
    pub byte_range: TextRange,

    #[napi(readonly)]
    pub codepoint_range: TextRange,

    #[napi(readonly)]
    pub confidence: Option<f64>,

    #[napi(readonly)]
    pub detector_name: String,

    #[napi(readonly)]
    pub detector_version: Option<String>,
}

fn js_offset(offset: usize) -> napi::Result<u32> {
    u32::try_from(offset).map_err(|_| {
        Error::new(
            Status::GenericFailure,
            "entity offset exceeds the JavaScript binding limit",
        )
    })
}

fn js_range(range: datafog_core::TextRange) -> napi::Result<TextRange> {
    Ok(TextRange {
        start: js_offset(range.start)?,
        end: js_offset(range.end)?,
    })
}

/// Scan text for supported PII findings.
#[napi(strict, catch_unwind)]
pub fn scan(text: String) -> napi::Result<Vec<Finding>> {
    datafog_core::scan(&text)
        .into_iter()
        .map(|finding| {
            Ok(Finding {
                entity_type: finding.entity_type,
                matched_text: finding.matched_text,
                byte_range: js_range(finding.byte_range)?,
                codepoint_range: js_range(finding.codepoint_range)?,
                confidence: finding.confidence.map(f64::from),
                detector_name: finding.detector_name,
                detector_version: finding.detector_version,
            })
        })
        .collect()
}
