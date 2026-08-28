//! Node binding for datafog-core.

use napi::bindgen_prelude::Buffer;
use napi::{Env, Error, Status, Unknown};
use napi_derive::napi;

#[napi(object)]
pub struct TextRange {
    #[napi(readonly)]
    pub start: u32,

    #[napi(readonly)]
    pub end: u32,
}

#[napi(object)]
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

#[napi(object, object_from_js = false)]
pub struct Transformation {
    #[napi(readonly)]
    pub entity_type: String,

    #[napi(readonly)]
    pub source_byte_range: TextRange,

    #[napi(readonly)]
    pub source_codepoint_range: TextRange,

    #[napi(readonly)]
    pub confidence: Option<f64>,

    #[napi(readonly)]
    pub detector_name: String,

    #[napi(readonly)]
    pub detector_version: Option<String>,

    #[napi(readonly, ts_type = "TransformationStrategy")]
    pub strategy: String,

    #[napi(readonly)]
    pub replacement: String,

    #[napi(readonly)]
    pub output_byte_range: TextRange,

    #[napi(readonly)]
    pub output_codepoint_range: TextRange,

    #[napi(readonly)]
    pub key_ref: Option<String>,

    #[napi(readonly)]
    pub resolved_key_version: Option<String>,
}

#[napi(object, object_from_js = false)]
pub struct TransformResult {
    #[napi(readonly)]
    pub text: String,

    #[napi(readonly)]
    pub transformations: Vec<Transformation>,
}

#[napi(object, object_from_js = false)]
pub struct KeySelector {
    #[napi(readonly)]
    pub index: u32,

    #[napi(readonly)]
    pub key_ref: String,

    #[napi(readonly)]
    pub key_version: Option<String>,

    #[napi(readonly)]
    pub path: String,
}

#[napi(object)]
pub struct ResolvedKeyInput {
    pub selector_index: u32,
    #[napi(ts_type = "Uint8Array")]
    pub key: Buffer,
    pub resolved_version: String,
}

#[napi(object, object_from_js = false)]
pub struct PreparedScanAndTransform {
    #[napi(readonly)]
    pub findings: Vec<Finding>,

    #[napi(readonly)]
    pub selectors: Vec<KeySelector>,
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

fn js_finding(finding: datafog_core::Finding) -> napi::Result<Finding> {
    Ok(Finding {
        entity_type: finding.entity_type,
        matched_text: finding.matched_text,
        byte_range: js_range(finding.byte_range)?,
        codepoint_range: js_range(finding.codepoint_range)?,
        confidence: finding.confidence.map(f64::from),
        detector_name: finding.detector_name,
        detector_version: finding.detector_version,
    })
}

fn core_finding(finding: Finding) -> datafog_core::Finding {
    datafog_core::Finding {
        entity_type: finding.entity_type,
        matched_text: finding.matched_text,
        byte_range: datafog_core::TextRange {
            start: finding.byte_range.start as usize,
            end: finding.byte_range.end as usize,
        },
        codepoint_range: datafog_core::TextRange {
            start: finding.codepoint_range.start as usize,
            end: finding.codepoint_range.end as usize,
        },
        confidence: finding.confidence.map(|confidence| confidence as f32),
        detector_name: finding.detector_name,
        detector_version: finding.detector_version,
    }
}

fn js_privacy_error(error: datafog_core::PrivacyError) -> Error {
    let payload = serde_json::json!({
        "code": error.code().as_str(),
        "reason": error.reason().map(datafog_core::PrivacyErrorReason::as_str),
        "message": error.to_string(),
        "path": error.path(),
        "findingIndex": error.finding_index(),
    });
    let status = match error.code() {
        datafog_core::PrivacyErrorCode::InternalError
        | datafog_core::PrivacyErrorCode::KeyProviderUnavailable
        | datafog_core::PrivacyErrorCode::KeyProviderError => Status::GenericFailure,
        _ => Status::InvalidArg,
    };
    Error::new(status, payload.to_string())
}

fn js_transform_result(result: datafog_core::TransformResult) -> napi::Result<TransformResult> {
    Ok(TransformResult {
        text: result.text,
        transformations: result
            .transformations
            .into_iter()
            .map(|transformation| {
                Ok(Transformation {
                    entity_type: transformation.entity_type,
                    source_byte_range: js_range(transformation.source_byte_range)?,
                    source_codepoint_range: js_range(transformation.source_codepoint_range)?,
                    confidence: transformation.confidence.map(f64::from),
                    detector_name: transformation.detector_name,
                    detector_version: transformation.detector_version,
                    strategy: match transformation.strategy {
                        datafog_core::TransformationStrategy::Redact => "redact".to_owned(),
                        datafog_core::TransformationStrategy::Remove => "remove".to_owned(),
                        datafog_core::TransformationStrategy::Mask(_) => "mask".to_owned(),
                        datafog_core::TransformationStrategy::Pseudonymize(_) => {
                            "pseudonymize".to_owned()
                        }
                    },
                    replacement: transformation.replacement,
                    output_byte_range: js_range(transformation.output_byte_range)?,
                    output_codepoint_range: js_range(transformation.output_codepoint_range)?,
                    key_ref: transformation.key_ref,
                    resolved_key_version: transformation.resolved_key_version,
                })
            })
            .collect::<napi::Result<Vec<_>>>()?,
    })
}

fn js_key_selectors(selectors: &[datafog_core::KeySelector]) -> napi::Result<Vec<KeySelector>> {
    selectors
        .iter()
        .enumerate()
        .map(|(index, selector)| {
            Ok(KeySelector {
                index: js_offset(index)?,
                key_ref: selector.key_ref().to_owned(),
                key_version: selector.key_version().map(str::to_owned),
                path: selector.path().to_owned(),
            })
        })
        .collect()
}

fn core_key_bindings(
    selectors: Vec<datafog_core::KeySelector>,
    resolved_keys: Vec<ResolvedKeyInput>,
) -> napi::Result<Vec<datafog_core::ResolvedKeyBinding>> {
    resolved_keys
        .into_iter()
        .map(|resolved| {
            let selector = selectors
                .get(resolved.selector_index as usize)
                .cloned()
                .ok_or_else(|| {
                    Error::new(Status::InvalidArg, "resolved key selector index is invalid")
                })?;
            Ok(datafog_core::ResolvedKeyBinding::new(
                selector,
                datafog_core::ResolvedKey::new(resolved.key.to_vec(), resolved.resolved_version),
            ))
        })
        .collect()
}

/// Scan text for supported PII findings.
#[napi(strict, catch_unwind)]
pub fn scan(
    env: Env,
    text: String,
    #[napi(ts_arg_type = "ScanConfig | undefined")] config: Option<Unknown<'_>>,
) -> napi::Result<Vec<Finding>> {
    let config = if let Some(config) = config {
        let config: serde_json::Value = env.from_js_value(config)?;
        datafog_core::parse_scan_config(&config).map_err(js_privacy_error)?
    } else {
        datafog_core::ScanConfig::default()
    };
    datafog_core::scan_with_config(&text, &config)
        .into_iter()
        .map(js_finding)
        .collect()
}

/// Transform explicit findings without scanning implicitly.
#[napi(strict, catch_unwind)]
pub fn transform(
    env: Env,
    text: String,
    findings: Vec<Finding>,
    #[napi(ts_arg_type = "TransformationConfig")] config: Unknown<'_>,
) -> napi::Result<TransformResult> {
    let config: serde_json::Value = env.from_js_value(config)?;
    let config = datafog_core::parse_transformation_config(&config).map_err(js_privacy_error)?;
    let findings = findings.into_iter().map(core_finding).collect::<Vec<_>>();
    datafog_core::transform(&text, &findings, &config)
        .map_err(js_privacy_error)
        .and_then(js_transform_result)
}

/// Scan text and transform the detected findings.
#[napi(strict, catch_unwind)]
pub fn scan_and_transform(
    env: Env,
    text: String,
    #[napi(ts_arg_type = "ScanAndTransformConfig")] config: Unknown<'_>,
) -> napi::Result<TransformResult> {
    let config: serde_json::Value = env.from_js_value(config)?;
    let config =
        datafog_core::parse_scan_and_transform_config(&config).map_err(js_privacy_error)?;
    datafog_core::scan_and_transform(&text, &config)
        .map_err(js_privacy_error)
        .and_then(js_transform_result)
}

#[napi(strict, catch_unwind)]
pub fn required_key_selectors(
    env: Env,
    text: String,
    findings: Vec<Finding>,
    #[napi(ts_arg_type = "TransformationConfig")] config: Unknown<'_>,
) -> napi::Result<Vec<KeySelector>> {
    let config: serde_json::Value = env.from_js_value(config)?;
    let config = datafog_core::parse_transformation_config(&config).map_err(js_privacy_error)?;
    let findings = findings.into_iter().map(core_finding).collect::<Vec<_>>();
    let selectors = datafog_core::required_key_selectors(&text, &findings, &config)
        .map_err(js_privacy_error)?;
    js_key_selectors(&selectors)
}

#[napi(strict, catch_unwind)]
pub fn transform_with_resolved_keys(
    env: Env,
    text: String,
    findings: Vec<Finding>,
    #[napi(ts_arg_type = "TransformationConfig")] config: Unknown<'_>,
    resolved_keys: Vec<ResolvedKeyInput>,
) -> napi::Result<TransformResult> {
    let config: serde_json::Value = env.from_js_value(config)?;
    let config = datafog_core::parse_transformation_config(&config).map_err(js_privacy_error)?;
    let findings = findings.into_iter().map(core_finding).collect::<Vec<_>>();
    let selectors = datafog_core::required_key_selectors(&text, &findings, &config)
        .map_err(js_privacy_error)?;
    let bindings = core_key_bindings(selectors, resolved_keys)?;
    datafog_core::transform_with_resolved_keys(&text, &findings, &config, bindings)
        .map_err(js_privacy_error)
        .and_then(js_transform_result)
}

#[napi(strict, catch_unwind)]
pub fn prepare_scan_and_transform(
    env: Env,
    text: String,
    #[napi(ts_arg_type = "ScanAndTransformConfig")] config: Unknown<'_>,
) -> napi::Result<PreparedScanAndTransform> {
    let config: serde_json::Value = env.from_js_value(config)?;
    let config =
        datafog_core::parse_scan_and_transform_config(&config).map_err(js_privacy_error)?;
    let findings = datafog_core::scan_with_config(&text, config.scan_config());
    let selectors =
        datafog_core::required_key_selectors(&text, &findings, config.transformation_config())
            .map_err(js_privacy_error)?;
    Ok(PreparedScanAndTransform {
        findings: findings
            .into_iter()
            .map(js_finding)
            .collect::<napi::Result<Vec<_>>>()?,
        selectors: js_key_selectors(&selectors)?,
    })
}
