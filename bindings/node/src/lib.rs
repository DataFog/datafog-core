//! Node binding for datafog-core.

use napi::{Error, Status};
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

#[napi(object)]
pub struct NativeMaskRevealConfig {
    pub direction: String,
    pub count: f64,
}

#[napi(object)]
pub struct NativeTransformationConfig {
    #[napi(ts_type = "TransformationStrategy")]
    pub strategy: String,
    pub character: Option<String>,
    pub reveal: Option<NativeMaskRevealConfig>,
}

#[napi(object, object_from_js = false)]
pub struct Transformation {
    #[napi(readonly)]
    pub finding: Finding,

    #[napi(readonly, ts_type = "TransformationStrategy")]
    pub strategy: String,

    #[napi(readonly)]
    pub replacement: String,

    #[napi(readonly)]
    pub output_byte_range: TextRange,

    #[napi(readonly)]
    pub output_codepoint_range: TextRange,
}

#[napi(object, object_from_js = false)]
pub struct TransformResult {
    #[napi(readonly)]
    pub text: String,

    #[napi(readonly)]
    pub transformations: Vec<Transformation>,
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

fn core_strategy(
    config: NativeTransformationConfig,
) -> napi::Result<datafog_core::TransformationStrategy> {
    match config.strategy.as_str() {
        "redact" if config.character.is_none() && config.reveal.is_none() => {
            Ok(datafog_core::TransformationStrategy::Redact)
        }
        "remove" if config.character.is_none() && config.reveal.is_none() => {
            Ok(datafog_core::TransformationStrategy::Remove)
        }
        "mask" => {
            let character = config.character.unwrap_or_else(|| "*".to_owned());
            let mut characters = character.chars();
            let character = characters
                .next()
                .filter(|_| characters.next().is_none())
                .ok_or_else(|| {
                    Error::new(
                        Status::InvalidArg,
                        "mask character must contain exactly one code point",
                    )
                })?;
            let reveal = match config.reveal {
                None => datafog_core::MaskReveal::None,
                Some(reveal) => {
                    if !reveal.count.is_finite()
                        || reveal.count < 0.0
                        || reveal.count.fract() != 0.0
                        || reveal.count > 9_007_199_254_740_991.0
                    {
                        return Err(Error::new(
                            Status::InvalidArg,
                            "reveal count must be a non-negative safe integer",
                        ));
                    }
                    let count = reveal.count as usize;
                    match reveal.direction.as_str() {
                        "first" => datafog_core::MaskReveal::First(count),
                        "last" => datafog_core::MaskReveal::Last(count),
                        _ => {
                            return Err(Error::new(
                                Status::InvalidArg,
                                "reveal direction must be 'first' or 'last'",
                            ));
                        }
                    }
                }
            };
            datafog_core::MaskConfig::new(character, reveal)
                .map(datafog_core::TransformationStrategy::Mask)
                .map_err(|_| {
                    Error::new(
                        Status::InvalidArg,
                        "mask character must not be whitespace or a control character",
                    )
                })
        }
        "redact" | "remove" => Err(Error::new(
            Status::InvalidArg,
            "redact and remove do not accept mask configuration",
        )),
        _ => Err(Error::new(
            Status::InvalidArg,
            "strategy must be 'redact', 'mask', or 'remove'",
        )),
    }
}

fn js_transform_result(result: datafog_core::TransformResult) -> napi::Result<TransformResult> {
    Ok(TransformResult {
        text: result.text,
        transformations: result
            .transformations
            .into_iter()
            .map(|transformation| {
                Ok(Transformation {
                    finding: js_finding(transformation.finding)?,
                    strategy: match transformation.strategy {
                        datafog_core::TransformationStrategy::Redact => "redact".to_owned(),
                        datafog_core::TransformationStrategy::Remove => "remove".to_owned(),
                        datafog_core::TransformationStrategy::Mask(_) => "mask".to_owned(),
                    },
                    replacement: transformation.replacement,
                    output_byte_range: js_range(transformation.output_byte_range)?,
                    output_codepoint_range: js_range(transformation.output_codepoint_range)?,
                })
            })
            .collect::<napi::Result<Vec<_>>>()?,
    })
}

/// Scan text for supported PII findings.
#[napi(strict, catch_unwind)]
pub fn scan(text: String) -> napi::Result<Vec<Finding>> {
    datafog_core::scan(&text)
        .into_iter()
        .map(js_finding)
        .collect()
}

/// Transform explicit findings without scanning implicitly.
#[napi(strict, catch_unwind)]
pub fn transform(
    text: String,
    findings: Vec<Finding>,
    #[napi(ts_arg_type = "TransformationConfig")] config: NativeTransformationConfig,
) -> napi::Result<TransformResult> {
    let findings = findings.into_iter().map(core_finding).collect::<Vec<_>>();
    datafog_core::transform(&text, &findings, core_strategy(config)?)
        .map_err(|error| Error::new(Status::InvalidArg, error.to_string()))
        .and_then(js_transform_result)
}

/// Scan text and transform the detected findings.
#[napi(strict, catch_unwind)]
pub fn scan_and_transform(
    text: String,
    #[napi(ts_arg_type = "TransformationConfig")] config: NativeTransformationConfig,
) -> napi::Result<TransformResult> {
    datafog_core::scan_and_transform(&text, core_strategy(config)?)
        .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))
        .and_then(js_transform_result)
}
