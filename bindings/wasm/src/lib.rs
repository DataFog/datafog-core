use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct TextRange {
    start: usize,
    end: usize,
}

impl From<datafog_core::TextRange> for TextRange {
    fn from(range: datafog_core::TextRange) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    entity_type: String,
    matched_text: String,
    byte_range: TextRange,
    codepoint_range: TextRange,
    confidence: Option<f32>,
    detector_name: String,
    detector_version: Option<String>,
}

#[wasm_bindgen]
pub fn scan(text: &str) -> Result<JsValue, JsValue> {
    let findings: Vec<Finding> = datafog_core::scan(text)
        .into_iter()
        .map(|finding| Finding {
            entity_type: finding.entity_type,
            matched_text: finding.matched_text,
            byte_range: finding.byte_range.into(),
            codepoint_range: finding.codepoint_range.into(),
            confidence: finding.confidence,
            detector_name: finding.detector_name,
            detector_version: finding.detector_version,
        })
        .collect();

    serde_wasm_bindgen::to_value(&findings).map_err(|error| JsValue::from_str(&error.to_string()))
}
