//! Embedded statements-v1 CNN. No files, network, subprocesses or inference runtime.
//! This model detects statement recipient names and mailing-address blocks.
mod features;
use crate::model_scan::{
    BackendResult, CoordinateUnit, ModelError, ModelScanContract, ModelScanResult, NativeSpan,
    ResourceLimits,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MODEL_ID: &str = "statements-v1-r6split-pretrained-s43";
pub const MODEL_SHA256: &str = "47a29acb46f159fb12d33557ed6859a5157c6dd10da461e27de60d2ece4ebabd";
pub const MAX_INPUT_BYTES: usize = 65_536;
pub const MAX_TOKENS: usize = 8_192;
const BUNDLE: &[u8] = include_bytes!("assets/bundle.bin");
const DIM: usize = 96;

fn half(bits: u16) -> f32 {
    let sign = ((bits as u32) & 0x8000) << 16;
    let exp = (bits >> 10) & 31;
    let fraction = (bits & 1023) as u32;
    if exp == 0 {
        if fraction == 0 {
            f32::from_bits(sign)
        } else {
            let value = fraction as f32 * (1.0 / 16_777_216.0);
            if sign == 0 { value } else { -value }
        }
    } else if exp == 31 {
        f32::from_bits(sign | 0x7f800000 | (fraction << 13))
    } else {
        f32::from_bits(sign | (((exp as u32) + 112) << 23) | (fraction << 13))
    }
}
fn half_round(value: f32) -> f32 {
    if value == 0.0 {
        return value;
    }
    // Features are finite nonnegative values <= 1. Round nearest, ties to even.
    let bits = value.to_bits();
    let exp = ((bits >> 23) & 255) as i32 - 127;
    let shift = if exp < -14 {
        (-14 - exp + 13) as u32
    } else {
        13
    };
    if shift >= 32 {
        return 0.0;
    }
    let mantissa = (bits & 0x7fffff) | 0x800000;
    let rounded = (mantissa + ((1u32 << (shift - 1)) - 1) + ((mantissa >> shift) & 1)) >> shift;
    if exp < -14 {
        half(rounded as u16)
    } else {
        half((((exp + 15) as u32) << 10).wrapping_add(rounded - 1024) as u16)
    }
}
fn error() -> ModelError {
    ModelError::new("model_load_failed", "invalid_embedded_model", "/cnn")
}
#[derive(Deserialize)]
struct Layout {
    format: String,
    tensors: BTreeMap<String, TensorLayout>,
}
#[derive(Deserialize)]
struct TensorLayout {
    kind: String,
    shape: Vec<usize>,
    parts: Vec<[usize; 2]>,
}
struct Tensor {
    values: Vec<f32>,
}
struct Model {
    char_embedding: Tensor,
    char_conv: Tensor,
    char_bias: Tensor,
    project: Tensor,
    project_bias: Tensor,
    blocks: Vec<(Tensor, Tensor)>,
    head: Tensor,
    head_bias: Tensor,
}
fn bytes(part: [usize; 2]) -> Result<&'static [u8], ModelError> {
    BUNDLE
        .get(part[0]..part[0].checked_add(part[1]).ok_or_else(error)?)
        .ok_or_else(error)
}
fn decode(layout: &Layout, name: &str, shape: &[usize]) -> Result<Tensor, ModelError> {
    let t = layout.tensors.get(name).ok_or_else(error)?;
    if t.shape != shape {
        return Err(error());
    }
    let size = shape
        .iter()
        .try_fold(1usize, |a, b| a.checked_mul(*b))
        .ok_or_else(error)?;
    let values = match t.kind.as_str() {
        "fp16" if t.parts.len() == 1 => {
            let b = bytes(t.parts[0])?;
            if b.len() != size * 2 {
                return Err(error());
            }
            b.chunks_exact(2)
                .map(|x| half(u16::from_le_bytes([x[0], x[1]])))
                .collect()
        }
        "int8_row_sym" if t.parts.len() == 2 => {
            let b = bytes(t.parts[0])?;
            let scales = bytes(t.parts[1])?;
            if b.len() != size || scales.len() != shape[0] * 2 {
                return Err(error());
            }
            let width = size / shape[0];
            b.iter()
                .enumerate()
                .map(|(i, x)| {
                    let r = i / width;
                    *x as i8 as f32 * half(u16::from_le_bytes([scales[r * 2], scales[r * 2 + 1]]))
                })
                .collect()
        }
        _ => return Err(error()),
    };
    let values: Vec<f32> = values;
    if values.iter().any(|x| !x.is_finite()) {
        return Err(error());
    }
    Ok(Tensor { values })
}
impl Model {
    fn load() -> Result<Self, ModelError> {
        if format!("{:x}", Sha256::digest(BUNDLE)) != MODEL_SHA256 {
            return Err(error());
        }
        let l: Layout =
            serde_json::from_str(include_str!("assets/bundle.json")).map_err(|_| error())?;
        if l.format != "statements-v1 bundle v0" || l.tensors.len() != 18 {
            return Err(error());
        }
        let words = l.tensors.get("words.weight").ok_or_else(error)?;
        if words.kind != "int4_row_sym"
            || words.shape != [32768, 48]
            || words.parts != [[0, 786432], [786432, 65536]]
        {
            return Err(error());
        }
        Ok(Self {
            char_embedding: decode(&l, "chars.weight", &[1024, 16])?,
            char_conv: decode(&l, "char_conv.weight", &[32, 16, 3])?,
            char_bias: decode(&l, "char_conv.bias", &[32])?,
            project: decode(&l, "project.weight", &[96, 102])?,
            project_bias: decode(&l, "project.bias", &[96])?,
            blocks: (0..5)
                .map(|i| {
                    Ok((
                        decode(&l, &format!("blocks.{i}.weight"), &[96, 96, 3])?,
                        decode(&l, &format!("blocks.{i}.bias"), &[96])?,
                    ))
                })
                .collect::<Result<_, ModelError>>()?,
            head: decode(&l, "head.weight", &[5, 96])?,
            head_bias: decode(&l, "head.bias", &[5])?,
        })
    }
    fn window(&self, tokens: &[features::Token]) -> Vec<[f32; 5]> {
        let n = tokens.len();
        let mut x = vec![[0.0; DIM]; n];
        for (t, token) in tokens.iter().enumerate() {
            let mut input = [0.0; 102];
            let row = token.word;
            let scale = half(u16::from_le_bytes([
                BUNDLE[786432 + row * 2],
                BUNDLE[786433 + row * 2],
            ]));
            for (i, value) in input[..48].iter_mut().enumerate() {
                let packed = BUNDLE[row * 24 + i / 2];
                let q = if i % 2 == 0 { packed & 15 } else { packed >> 4 };
                *value = (q as i32 - 8) as f32 * scale;
            }
            // Conv sees 32 character positions, including learned embedding row zero.
            for out in 0..32 {
                let mut maximum = 0.0f32;
                for position in 0..token.chars.len() {
                    let mut value = self.char_bias.values[out];
                    for channel in 0..16 {
                        for kernel in 0..3 {
                            let p = position as isize + kernel as isize - 1;
                            if (0..32).contains(&p) {
                                let character = token.chars.get(p as usize).copied().unwrap_or(0);
                                value += self.char_embedding.values[character * 16 + channel]
                                    * self.char_conv.values[(out * 16 + channel) * 3 + kernel];
                            }
                        }
                    }
                    maximum = maximum.max(value);
                }
                input[48 + out] = maximum;
            }
            input[80..].copy_from_slice(&token.features);
            for out in 0..DIM {
                let mut value = self.project_bias.values[out];
                for (i, v) in input.iter().enumerate() {
                    value += v * self.project.values[out * 102 + i];
                }
                x[t][out] = value.max(0.0);
            }
        }
        for (layer, (weight, bias)) in self.blocks.iter().enumerate() {
            let dilation = 1isize << layer;
            let mut next = vec![[0.0; DIM]; n];
            for t in 0..n {
                for out in 0..DIM {
                    let mut value = bias.values[out];
                    for channel in 0..DIM {
                        for kernel in 0..3 {
                            let p = t as isize + (kernel as isize - 1) * dilation;
                            if (0..n as isize).contains(&p) {
                                value += x[p as usize][channel]
                                    * weight.values[(out * DIM + channel) * 3 + kernel];
                            }
                        }
                    }
                    next[t][out] = (x[t][out] + value).max(0.0);
                }
            }
            x = next;
        }
        x.iter()
            .map(|row| {
                let mut logits = [0.0; 5];
                for (out, value) in logits.iter_mut().enumerate() {
                    *value = self.head_bias.values[out];
                    for (i, v) in row.iter().enumerate() {
                        *value += v * self.head.values[out * DIM + i];
                    }
                }
                logits
            })
            .collect()
    }
}

/// Explicit embedded detector. Construction authenticates embedded weights once.
/// Create this in a Worker to keep synchronous CPU work off the browser UI thread.
pub struct CnnDetector {
    model: Model,
    contract: ModelScanContract,
}
impl CnnDetector {
    pub fn open() -> Result<Self, ModelError> {
        Ok(Self {
            model: Model::load()?,
            contract: Self::contract(),
        })
    }
    fn contract() -> ModelScanContract {
        ModelScanContract {
            backend: "embedded_cnn".into(),
            model: MODEL_ID.into(),
            adapter_version: "statements-cnn-v1".into(),
            labels: [
                ("PERSON".into(), "PERSON".into()),
                ("ADDRESS".into(), "ADDRESS".into()),
            ]
            .into(),
            limits: ResourceLimits {
                max_input_bytes: MAX_INPUT_BYTES,
                max_response_bytes: 4 * 1024 * 1024,
                max_findings: 1024,
                timeout_ms: 30000,
                cleanup_timeout_ms: 1000,
            },
            score_scale: "statements_bio_logits".into(),
            thresholds: BTreeMap::new(),
        }
    }
    /// Validate the same selection while skipping model authentication and inference.
    pub fn scan_disabled(
        text: &str,
        config: &serde_json::Value,
    ) -> Result<ModelScanResult, ModelError> {
        let contract = Self::contract();
        let routing = contract.routed_config(config)?;
        contract.complete(text, BackendResult { spans: Vec::new() }, Some(&routing))
    }
    pub fn describe(&self) -> crate::model_scan::ModelDescriptor {
        self.contract.describe()
    }
    /// Rules plus requested CNN labels. Errors never return partial detections.
    pub fn scan(
        &self,
        text: &str,
        config: &serde_json::Value,
    ) -> Result<ModelScanResult, ModelError> {
        self.contract.check_input(text)?;
        let routing = self.contract.routed_config(config)?;
        let spans = if routing.needs_model() {
            self.infer(text)?
        } else {
            Vec::new()
        };
        self.contract
            .complete(text, BackendResult { spans }, Some(&routing))
    }
    /// Model-only output for reproducible evaluation and cross-target parity.
    pub fn scan_model(&self, text: &str) -> Result<ModelScanResult, ModelError> {
        self.contract.check_input(text)?;
        self.contract.complete(
            text,
            BackendResult {
                spans: self.infer(text)?,
            },
            None,
        )
    }
    fn infer(&self, text: &str) -> Result<Vec<NativeSpan>, ModelError> {
        let tokens = features::encode(text)?;
        let n = tokens.len();
        if n > MAX_TOKENS {
            return Err(ModelError::new(
                "model_resource_exhausted",
                "limit_exceeded",
                "/data",
            ));
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut logits = vec![[0.0; 5]; n];
        let starts: Vec<_> = (0..n.saturating_sub(64).max(1)).step_by(192).collect();
        let mut previous = 0;
        for (j, &s) in starts.iter().enumerate() {
            let end = n.min(s + 256);
            let own_end = if j + 1 == starts.len() {
                n
            } else {
                n.min(s + 224)
            };
            let out = self.model.window(&tokens[s..end]);
            logits[previous..own_end].copy_from_slice(&out[previous - s..own_end - s]);
            previous = own_end;
        }
        if logits.iter().flatten().any(|x| !x.is_finite()) {
            return Err(ModelError::invalid_output());
        }
        let tags = viterbi(&logits);
        let mut spans: Vec<NativeSpan> = Vec::new();
        let mut current = None;
        for (token, tag) in tokens.iter().zip(tags) {
            if tag == 0 {
                current = None;
                continue;
            }
            let label = if tag <= 2 { "PERSON" } else { "ADDRESS" };
            if tag == 1 || tag == 3 || current != Some(label) {
                spans.push(NativeSpan {
                    native_label: label.into(),
                    start: token.start,
                    end: token.end,
                    coordinate_unit: CoordinateUnit::Utf8Byte,
                    raw_score: None,
                    calibrated_confidence: None,
                    matched_text: None,
                });
            } else if let Some(last) = spans.last_mut() {
                last.end = token.end;
            }
            current = Some(label);
        }
        Ok(spans)
    }
}
fn viterbi(logits: &[[f32; 5]]) -> Vec<usize> {
    if logits.is_empty() {
        return Vec::new();
    }
    // Match the reference log-softmax before dynamic programming; normalization
    // also keeps long-page path scores from accumulating large common offsets.
    let logp: Vec<_> = logits
        .iter()
        .map(|row| {
            let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let normalizer = row
                .iter()
                .map(|value| (*value - max).exp())
                .sum::<f32>()
                .ln();
            row.map(|value| value - max - normalizer)
        })
        .collect();
    let mut score = logp[0];
    score[2] -= 1e9;
    score[4] -= 1e9;
    let mut back = vec![[0; 5]; logits.len()];
    for i in 1..logits.len() {
        let mut next = [0.0; 5];
        for (b, output) in next.iter_mut().enumerate() {
            let mut best = f32::NEG_INFINITY;
            for (a, &prior) in score.iter().enumerate() {
                let allowed = b == 0
                    || b == 1
                    || b == 3
                    || (b == 2 && (a == 1 || a == 2))
                    || (b == 4 && (a == 3 || a == 4));
                let candidate = prior + if allowed { 0.0 } else { -1e9 };
                if candidate > best {
                    best = candidate;
                    back[i][b] = a;
                }
            }
            *output = best + logp[i][b];
        }
        score = next;
    }
    let mut last = 0;
    for k in 1..5 {
        if score[k] > score[last] {
            last = k;
        }
    }
    let mut path = vec![last; logits.len()];
    for i in (1..logits.len()).rev() {
        last = back[i][last];
        path[i - 1] = last;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn serialized_quantized_reference_has_identical_features_logits_and_spans() {
        let fixtures: Value = serde_json::from_str(include_str!("reference.json")).unwrap();
        let detector = CnnDetector::open().unwrap();
        for fixture in fixtures["fixtures"].as_array().unwrap() {
            let text = fixture["text"].as_str().unwrap();
            let tokens = features::encode(text).unwrap();
            let expected = fixture["tokens"].as_array().unwrap();
            assert_eq!(tokens.len(), expected.len(), "{text}");
            for (token, want) in tokens.iter().zip(expected) {
                assert_eq!(token.start, want["start"].as_u64().unwrap() as usize);
                assert_eq!(token.end, want["end"].as_u64().unwrap() as usize);
                assert_eq!(
                    token.word,
                    want["word"].as_u64().unwrap() as usize,
                    "hash {}",
                    &text[token.start..token.end]
                );
                let mut chars = token.chars.clone();
                chars.resize(32, 0);
                assert_eq!(json!(chars), want["chars"]);
                assert_eq!(
                    json!(token.features),
                    want["features"],
                    "features {}",
                    &text[token.start..token.end]
                );
            }
            if let Some(expected) = fixture["logits"].as_array() {
                let actual = detector.model.window(&tokens);
                for (row, want) in actual.iter().zip(expected) {
                    for (got, value) in row.iter().zip(want.as_array().unwrap()) {
                        let value = value.as_f64().unwrap() as f32;
                        assert!(
                            (got - value).abs() < 0.002,
                            "logit {got} != {value}, text {text}"
                        );
                    }
                }
            }
            let spans = detector.infer(text).unwrap();
            let actual: Vec<_> = spans
                .iter()
                .map(|s| json!({"label":s.native_label,"start":s.start,"end":s.end}))
                .collect();
            assert_eq!(json!(actual), fixture["spans"], "spans {text}");
        }
    }
    #[test]
    fn bounds_and_rule_only_routing_fail_closed() {
        let detector = CnnDetector::open().unwrap();
        assert!(
            detector
                .scan_model(&"x".repeat(MAX_INPUT_BYTES + 1))
                .is_err()
        );
        assert!(detector.scan_model(&"! ".repeat(MAX_TOKENS + 1)).is_err());
        assert!(detector.scan_model("").unwrap().findings.is_empty());
        let result = detector
            .scan(
                "John Smith john@example.com",
                &json!({"entities":["EMAIL"]}),
            )
            .unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].entity_type, "EMAIL");
        assert!(
            detector
                .scan("text", &json!({"entities":["INVALID"]}))
                .is_err()
        );
    }
    #[test]
    fn disabled_accepts_model_selections_without_loading_weights() {
        let result = CnnDetector::scan_disabled(
            "John Smith john@example.com",
            &json!({"entities":["PERSON","ADDRESS","EMAIL"]}),
        )
        .unwrap();
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].entity_type, "EMAIL");
        assert!(
            CnnDetector::scan_disabled("John Smith", &json!({"entities":["PERSON","ADDRESS"]}))
                .unwrap()
                .findings
                .is_empty()
        );
        assert!(CnnDetector::scan_disabled("text", &json!({"entities":[]})).is_err());
        assert!(CnnDetector::scan_disabled("text", &json!({"entities":["UNKNOWN"]})).is_err());
    }
    #[test]
    fn artifact_identity_includes_configuration() {
        assert_eq!(
            format!("{:x}", Sha256::digest(include_bytes!("assets/bundle.json"))),
            "21440a439b944492f7f90b003275f8b48d2cb8b13147f5a10b08ebc812155b80"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(include_bytes!("assets/config.json"))),
            "a3c93594f1a262ef356c4aff795bf86ff842e51cf5b384ae47895de80a6ee3b7"
        );
    }
    #[test]
    fn binary16_rounding_matches_boundary_cases() {
        for (v, expected) in [
            (1.0, 1.0),
            (0.5, 0.5),
            (1.0 / 3.0, 0.33325195),
            (0.00001, 0.00001001358),
        ] {
            assert_eq!(half_round(v), expected);
        }
    }
}
