use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

impl From<Finding> for datafog_core::Finding {
    fn from(finding: Finding) -> Self {
        Self {
            entity_type: finding.entity_type,
            matched_text: finding.matched_text,
            byte_range: datafog_core::TextRange {
                start: finding.byte_range.start,
                end: finding.byte_range.end,
            },
            codepoint_range: datafog_core::TextRange {
                start: finding.codepoint_range.start,
                end: finding.codepoint_range.end,
            },
            confidence: finding.confidence,
            detector_name: finding.detector_name,
            detector_version: finding.detector_version,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Transformation {
    finding: Finding,
    strategy: &'static str,
    replacement: String,
    output_byte_range: TextRange,
    output_codepoint_range: TextRange,
}

#[derive(Serialize)]
struct TransformResult {
    text: String,
    transformations: Vec<Transformation>,
}

fn finding_from_core(finding: datafog_core::Finding) -> Finding {
    Finding {
        entity_type: finding.entity_type,
        matched_text: finding.matched_text,
        byte_range: finding.byte_range.into(),
        codepoint_range: finding.codepoint_range.into(),
        confidence: finding.confidence,
        detector_name: finding.detector_name,
        detector_version: finding.detector_version,
    }
}

fn privacy_error(error: datafog_core::PrivacyError) -> JsValue {
    JsValue::from_str(
        &serde_json::json!({
            "code": error.code().as_str(),
            "reason": error.reason().map(datafog_core::PrivacyErrorReason::as_str),
            "message": error.to_string(),
            "path": error.path(),
            "findingIndex": error.finding_index(),
        })
        .to_string(),
    )
}

fn config_value(config: JsValue) -> Result<serde_json::Value, JsValue> {
    serde_wasm_bindgen::from_value(config).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn result_to_js(result: datafog_core::TransformResult) -> Result<JsValue, JsValue> {
    let result = TransformResult {
        text: result.text,
        transformations: result
            .transformations
            .into_iter()
            .map(|transformation| Transformation {
                finding: finding_from_core(transformation.finding),
                strategy: match transformation.strategy {
                    datafog_core::TransformationStrategy::Redact => "redact",
                    datafog_core::TransformationStrategy::Remove => "remove",
                    datafog_core::TransformationStrategy::Mask(_) => "mask",
                },
                replacement: transformation.replacement,
                output_byte_range: transformation.output_byte_range.into(),
                output_codepoint_range: transformation.output_codepoint_range.into(),
            })
            .collect(),
    };

    serde_wasm_bindgen::to_value(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn scan(text: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    let config = if let Some(config) = config {
        let config = config_value(config)?;
        datafog_core::parse_scan_config(&config).map_err(privacy_error)?
    } else {
        datafog_core::ScanConfig::default()
    };
    let findings: Vec<Finding> = datafog_core::scan_with_config(text, &config)
        .into_iter()
        .map(finding_from_core)
        .collect();

    serde_wasm_bindgen::to_value(&findings).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn transform(text: &str, findings: JsValue, config: JsValue) -> Result<JsValue, JsValue> {
    let findings: Vec<Finding> = serde_wasm_bindgen::from_value(findings)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let findings = findings
        .into_iter()
        .map(datafog_core::Finding::from)
        .collect::<Vec<_>>();
    let config = config_value(config)?;
    let config = datafog_core::parse_transformation_config(&config).map_err(privacy_error)?;
    let result = datafog_core::transform(text, &findings, &config).map_err(privacy_error)?;
    result_to_js(result)
}

#[wasm_bindgen]
pub fn scan_and_transform(text: &str, config: JsValue) -> Result<JsValue, JsValue> {
    let config = config_value(config)?;
    let config = datafog_core::parse_scan_and_transform_config(&config).map_err(privacy_error)?;
    let result = datafog_core::scan_and_transform(text, &config).map_err(privacy_error)?;
    result_to_js(result)
}
