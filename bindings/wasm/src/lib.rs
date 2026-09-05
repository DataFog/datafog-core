use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Default, Deserialize, Serialize)]
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
    #[serde(default, skip_deserializing)]
    utf16_range: TextRange,
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
    entity_type: String,
    source_byte_range: TextRange,
    source_codepoint_range: TextRange,
    source_utf16_range: TextRange,
    confidence: Option<f32>,
    detector_name: String,
    detector_version: Option<String>,
    strategy: &'static str,
    replacement: String,
    output_byte_range: TextRange,
    output_codepoint_range: TextRange,
    output_utf16_range: TextRange,
    key_ref: Option<String>,
    resolved_key_version: Option<String>,
    token_ref: Option<String>,
    resolved_token_version: Option<String>,
}

#[derive(Serialize)]
struct TransformResult {
    text: String,
    transformations: Vec<Transformation>,
}

fn utf16_range(
    index: &mut datafog_core::TextIndex<'_>,
    range: datafog_core::TextRange,
) -> Result<TextRange, JsValue> {
    index
        .utf16_range(range)
        .map(TextRange::from)
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn finding_from_core(
    index: &mut datafog_core::TextIndex<'_>,
    finding: datafog_core::Finding,
) -> Result<Finding, JsValue> {
    Ok(Finding {
        entity_type: finding.entity_type,
        matched_text: finding.matched_text,
        byte_range: finding.byte_range.into(),
        codepoint_range: finding.codepoint_range.into(),
        utf16_range: utf16_range(index, finding.byte_range)?,
        confidence: finding.confidence,
        detector_name: finding.detector_name,
        detector_version: finding.detector_version,
    })
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

fn result_to_js(
    source_text: &str,
    result: datafog_core::TransformResult,
) -> Result<JsValue, JsValue> {
    let mut source_index = datafog_core::TextIndex::new(source_text);
    let mut output_index = datafog_core::TextIndex::new(&result.text);
    let result = TransformResult {
        transformations: result
            .transformations
            .into_iter()
            .map(|transformation| {
                transformation_from_core(&mut source_index, &mut output_index, transformation)
            })
            .collect::<Result<Vec<_>, JsValue>>()?,
        text: result.text,
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
    let mut index = datafog_core::TextIndex::new(text);
    let findings: Vec<Finding> = datafog_core::scan_with_config(text, &config)
        .into_iter()
        .map(|finding| finding_from_core(&mut index, finding))
        .collect::<Result<_, _>>()?;

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
    if let Some(selector) = datafog_core::required_key_selectors(text, &findings, &config)
        .map_err(privacy_error)?
        .into_iter()
        .next()
    {
        return Err(privacy_error(
            datafog_core::PrivacyError::unsupported_strategy(selector.path()),
        ));
    }
    let placeholder_context = datafog_core::PrivacyContext::new("wasm").map_err(privacy_error)?;
    if !datafog_core::required_tokenization_items(
        text,
        &findings,
        &config,
        Some(&placeholder_context),
    )
    .map_err(privacy_error)?
    .is_empty()
    {
        return Err(privacy_error(
            datafog_core::PrivacyError::unsupported_strategy("/default/token_ref"),
        ));
    }
    let result = datafog_core::transform(text, &findings, &config).map_err(privacy_error)?;
    result_to_js(text, result)
}

#[wasm_bindgen]
pub fn scan_and_transform(text: &str, config: JsValue) -> Result<JsValue, JsValue> {
    let config = config_value(config)?;
    let config = datafog_core::parse_scan_and_transform_config(&config).map_err(privacy_error)?;
    let findings = datafog_core::scan_with_config(text, config.scan_config());
    if let Some(selector) =
        datafog_core::required_key_selectors(text, &findings, config.transformation_config())
            .map_err(privacy_error)?
            .into_iter()
            .next()
    {
        return Err(privacy_error(
            datafog_core::PrivacyError::unsupported_strategy(format!(
                "/transform{}",
                selector.path()
            )),
        ));
    }
    let placeholder_context = datafog_core::PrivacyContext::new("wasm").map_err(privacy_error)?;
    if !datafog_core::required_tokenization_items(
        text,
        &findings,
        config.transformation_config(),
        Some(&placeholder_context),
    )
    .map_err(privacy_error)?
    .is_empty()
    {
        return Err(privacy_error(
            datafog_core::PrivacyError::unsupported_strategy("/transform/default/token_ref"),
        ));
    }
    let result = datafog_core::scan_and_transform(text, &config).map_err(privacy_error)?;
    result_to_js(text, result)
}

#[wasm_bindgen]
pub fn restore(text: &str, context: JsValue) -> Result<JsValue, JsValue> {
    let context = config_value(context)?;
    let context = datafog_core::parse_privacy_context(&context).map_err(privacy_error)?;
    datafog_core::required_restore_items(text, &context).map_err(privacy_error)?;
    Err(privacy_error(
        datafog_core::PrivacyError::unsupported_strategy("/restore"),
    ))
}

#[derive(Serialize)]
struct StructuredFinding {
    path: String,
    finding: Finding,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldMapping {
    path: String,
    entity_type: String,
    source: String,
    rule: String,
}

fn field_mapping(mapping: datafog_core::structured::FieldMapping) -> FieldMapping {
    FieldMapping {
        path: mapping.path,
        entity_type: mapping.entity_type,
        source: mapping.source,
        rule: mapping.rule,
    }
}

fn structured_config(
    config: Option<JsValue>,
) -> Result<datafog_core::structured::StructuredScanConfig, JsValue> {
    match config {
        Some(config) => datafog_core::structured::parse_scan_config(&config_value(config)?)
            .map_err(privacy_error),
        None => Ok(datafog_core::structured::StructuredScanConfig::default()),
    }
}

#[wasm_bindgen]
pub fn discover_fields(data_json: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    let data = datafog_core::structured::parse_document_json(data_json).map_err(privacy_error)?;
    let mappings: Vec<_> =
        datafog_core::structured::discover_fields(&data, &structured_config(config)?)
            .map_err(privacy_error)?
            .into_iter()
            .map(field_mapping)
            .collect();
    serde_wasm_bindgen::to_value(&mappings)
        .map_err(|_| privacy_error(datafog_core::structured::invalid_data()))
}

#[wasm_bindgen]
pub fn scan_structured(data_json: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    #[derive(Serialize)]
    struct ResultValue {
        mappings: Vec<FieldMapping>,
        findings: Vec<StructuredFinding>,
    }
    let data = datafog_core::structured::parse_document_json(data_json).map_err(privacy_error)?;
    let result = datafog_core::structured::scan(&data, &structured_config(config)?)
        .map_err(privacy_error)?;
    let mut indices = std::collections::BTreeMap::new();
    let findings = result
        .findings
        .into_iter()
        .map(|located| {
            let text = data
                .pointer(&located.path)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| privacy_error(datafog_core::structured::invalid_data()))?;
            let index = indices
                .entry(located.path.clone())
                .or_insert_with(|| datafog_core::TextIndex::new(text));
            Ok(StructuredFinding {
                path: located.path,
                finding: finding_from_core(index, located.finding)?,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    serde_wasm_bindgen::to_value(&ResultValue {
        mappings: result.mappings.into_iter().map(field_mapping).collect(),
        findings,
    })
    .map_err(|_| privacy_error(datafog_core::structured::invalid_data()))
}

fn transformation_from_core(
    source_index: &mut datafog_core::TextIndex<'_>,
    output_index: &mut datafog_core::TextIndex<'_>,
    transformation: datafog_core::Transformation,
) -> Result<Transformation, JsValue> {
    Ok(Transformation {
        entity_type: transformation.entity_type,
        source_byte_range: transformation.source_byte_range.into(),
        source_codepoint_range: transformation.source_codepoint_range.into(),
        source_utf16_range: utf16_range(source_index, transformation.source_byte_range)?,
        confidence: transformation.confidence,
        detector_name: transformation.detector_name,
        detector_version: transformation.detector_version,
        strategy: match transformation.strategy {
            datafog_core::TransformationStrategy::Redact => "redact",
            datafog_core::TransformationStrategy::Remove => "remove",
            datafog_core::TransformationStrategy::Mask(_) => "mask",
            datafog_core::TransformationStrategy::Pseudonymize(_) => "pseudonymize",
            datafog_core::TransformationStrategy::Tokenize(_) => "tokenize",
        },
        replacement: transformation.replacement,
        output_byte_range: transformation.output_byte_range.into(),
        output_codepoint_range: transformation.output_codepoint_range.into(),
        output_utf16_range: utf16_range(output_index, transformation.output_byte_range)?,
        key_ref: transformation.key_ref,
        resolved_key_version: transformation.resolved_key_version,
        token_ref: transformation.token_ref,
        resolved_token_version: transformation.resolved_token_version,
    })
}

#[derive(Deserialize)]
struct StructuredFindingInput {
    path: String,
    finding: Finding,
}

fn structured_findings(
    value: JsValue,
) -> Result<Vec<datafog_core::structured::StructuredFinding>, JsValue> {
    let findings: Vec<StructuredFindingInput> = serde_wasm_bindgen::from_value(value)
        .map_err(|_| privacy_error(datafog_core::structured::invalid_data()))?;
    Ok(findings
        .into_iter()
        .map(|located| datafog_core::structured::StructuredFinding {
            path: located.path,
            finding: located.finding.into(),
        })
        .collect())
}

fn structured_operation_error(error: datafog_core::PrivacyError) -> JsValue {
    if matches!(
        error.code(),
        datafog_core::PrivacyErrorCode::KeyProviderRequired
            | datafog_core::PrivacyErrorCode::TokenProviderRequired
    ) {
        return privacy_error(datafog_core::PrivacyError::unsupported_strategy(
            error.path().unwrap_or("/default"),
        ));
    }
    privacy_error(error)
}

fn structured_result_to_js(
    data: &serde_json::Value,
    result: datafog_core::structured::StructuredTransformResult,
) -> Result<JsValue, JsValue> {
    #[derive(Serialize)]
    struct Record {
        path: String,
        transformation: Transformation,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ResultValue {
        data_json: String,
        transformations: Vec<Record>,
    }
    let mut indices = std::collections::BTreeMap::new();
    let transformations = result
        .transformations
        .into_iter()
        .map(|record| {
            let source = data
                .pointer(&record.path)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| privacy_error(datafog_core::structured::invalid_data()))?;
            let output = result
                .data
                .pointer(&record.path)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| privacy_error(datafog_core::structured::invalid_data()))?;
            let (source_index, output_index) =
                indices.entry(record.path.clone()).or_insert_with(|| {
                    (
                        datafog_core::TextIndex::new(source),
                        datafog_core::TextIndex::new(output),
                    )
                });
            Ok(Record {
                path: record.path,
                transformation: transformation_from_core(
                    source_index,
                    output_index,
                    record.transformation,
                )?,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    serde_wasm_bindgen::to_value(&ResultValue {
        data_json: result.data.to_string(),
        transformations,
    })
    .map_err(|_| privacy_error(datafog_core::structured::invalid_data()))
}

#[wasm_bindgen]
pub fn transform_structured(
    data_json: &str,
    findings: JsValue,
    config: JsValue,
) -> Result<JsValue, JsValue> {
    let data = datafog_core::structured::parse_document_json(data_json).map_err(privacy_error)?;
    let findings = structured_findings(findings)?;
    let config =
        datafog_core::parse_transformation_config(&config_value(config)?).map_err(privacy_error)?;
    let result = datafog_core::structured::transform(&data, &findings, &config)
        .map_err(structured_operation_error)?;
    structured_result_to_js(&data, result)
}

#[wasm_bindgen]
pub fn scan_and_transform_structured(data_json: &str, config: JsValue) -> Result<JsValue, JsValue> {
    let data = datafog_core::structured::parse_document_json(data_json).map_err(privacy_error)?;
    let config = datafog_core::structured::parse_scan_and_transform_config(&config_value(config)?)
        .map_err(privacy_error)?;
    let result = datafog_core::structured::scan_and_transform(&data, &config)
        .map_err(structured_operation_error)?;
    structured_result_to_js(&data, result)
}

#[wasm_bindgen]
pub fn restore_structured(data_json: &str, context: JsValue) -> Result<JsValue, JsValue> {
    let data = datafog_core::structured::parse_document_json(data_json).map_err(privacy_error)?;
    let context =
        datafog_core::parse_privacy_context(&config_value(context)?).map_err(privacy_error)?;
    datafog_core::structured::required_restore_items(&data, &context).map_err(privacy_error)?;
    Err(privacy_error(
        datafog_core::PrivacyError::unsupported_strategy("/restore"),
    ))
}
