# GLiNER2 Browser Benchmark Matrix

This document details the benchmark matrix for evaluating GLiNER2 model variants for browser inference.

## Model Candidates

| Model | Variant | Download Size | Architecture | Notes |
|-------|---------|---------------|--------------|-------|
| okasi/gliner2-privacy-filter-pii-multi-onnx | Q8 | ~525 MB | mDeBERTa-v3-base (307M params) | Rust core pinned model, requires ORT 1.22+ for 8-bit MatMulNBits |
| okasi/gliner2-privacy-filter-pii-multi-onnx | Q4 | ~473 MB | mDeBERTa-v3-base (307M params) | Rust core pinned model, 4-bit quantization |
| onnx-community/GLiNER2.5-Decide-ONNX | Q4F16 | ~499 MB | Larger GLiNER2.5 architecture | General entity extraction, not PII-specific |
| onnx-community/GLiNER2.5-Decide-mobile-ONNX | Q4F16 | ~329 MB | Mobile-optimized GLiNER2.5 | Smallest variant, reduced accuracy |

## Measurement Methodology

### Test Setup

- **Platform**: Node.js (onnxruntime-node) for baseline, Playwright + Chromium for browser
- **Sample Text**: 1-2 KB text with mixed PII entities
- **Fixtures**: D4 dataset (85 examples with PERSON and STREET_ADDRESS entities)
- **Threshold**: 0.5 confidence for entity extraction

### Metrics Collected

| Metric | Description |
|--------|-------------|
| Download Size | Total bytes for ONNX model + weights + tokenizer |
| Cold Load Time | First session creation including model loading |
| Warm Load Time | Subsequent session creation (model cached) |
| Warm Latency | Average inference time after warmup |
| Peak Memory | Heap memory usage during inference |
| PERSON Recall | True positive rate for PERSON entities |
| PERSON F1 | Harmonic mean of precision and recall for PERSON |
| STREET_ADDRESS Recall | True positive rate for address entities |
| STREET_ADDRESS F1 | F1 score for address entities |
| Overall F1 | Aggregate F1 across all entity types |

### ADR-004 Compliance

The ADR-004 requirement specifies:
- **Minimum 90% PERSON recall** for production use

## Runtime Compatibility Notes

### ONNX Runtime Version Requirements

| Model Variant | Minimum ORT Version | Notes |
|---------------|---------------------|-------|
| Q4 (4-bit) | 1.18+ | MatMulNBits supported |
| Q8 (8-bit) | 1.22+ | 8-bit MatMulNBits added in 1.22 |
| FP16 | 1.14+ | Standard fp16 support |
| FP32 | 1.10+ | Full precision, largest size |

### Known Issues

1. **onnxruntime-node 1.21.x**: Only supports 4-bit MatMulNBits; Q8 models fail to load
2. **onnxruntime-web WASM**: May have different MatMulNBits support than native Node
3. **External Data Files**: Auto-resolved when in same directory; explicit buffer passing may fail

## Running the Benchmark

```bash
# Install dependencies
cd spike/gliner2-browser
npm install

# Run benchmark for all models
npm run benchmark-matrix

# Run for specific model
npm run benchmark-matrix -- --models "okasi/gliner2-privacy-filter-pii-multi-onnx/q4"

# Skip download (use cached models)
npm run benchmark-matrix -- --skip-download

# Verbose output
npm run benchmark-matrix -- --verbose
```

### Adding New Model Candidates

Edit `scripts/benchmark-matrix.mjs` and add to `MODEL_CANDIDATES`:

```javascript
{
  id: "your-org/model-name",
  variant: "quantization-type",
  files: ["model.onnx", "model.onnx.data", "tokenizer.json", "tokenizer_config.json"],
  onnxModel: "model.onnx",
  dataFile: "model.onnx.data", // or null if embedded
  description: "Brief description",
  isRustCoreModel: false,
  labels: ["name", "address", "email", ...], // Entity labels supported
}
```

## Results

### Preliminary Measurements (Node.js, CPU)

| Model | Size | Cold Load | Warm Latency | Status |
|-------|------|-----------|--------------|--------|
| okasi Q8 | 525 MB | N/A | N/A | Requires ORT 1.22+ |
| okasi Q4 | 473 MB | ~1.5s | ~200ms | Loads but produces constant scores |
| GLiNER2.5-Decide Q4F16 | 499 MB | TBD | TBD | Not yet tested |
| GLiNER2.5-mobile Q4F16 | 329 MB | TBD | TBD | Not yet tested |

### Browser WASM Measurements

*Pending browser testing with Playwright*

### Accuracy Metrics (D4 Dataset)

*Pending valid model inference*

## Recommendations

### Current State

The okasi ONNX export is designed for Python onnxruntime with specific MatMulNBits support. Browser/Node.js compatibility requires:

1. **For Q8 (recommended accuracy)**: Wait for onnxruntime-web 1.22+ or use WebGPU backend
2. **For Q4 (smaller size)**: Investigate why scores are constant; may need different quantization
3. **Alternative**: Create FP16 export without MatMulNBits quantization

### Sweet Spot Candidates

Based on available data:

1. **Recommended**: okasi Q8 (~525 MB) - highest accuracy per model card, but requires newer ORT
2. **Runner-up**: onnx-community/GLiNER2.5-Decide-mobile Q4F16 (~329 MB) - smallest download, general NER

### Trade-offs

| Factor | okasi Q8 | okasi Q4 | GLiNER2.5-mobile |
|--------|----------|----------|------------------|
| Download Size | Large (525 MB) | Medium (473 MB) | Small (329 MB) |
| PII-specific Training | Yes | Yes | No |
| Rust Core Parity | Exact | Exact | Different model |
| Browser Compatibility | ORT 1.22+ | ORT 1.18+ | ORT 1.18+ |

## Next Steps

1. Test with onnxruntime-web 1.22+ when available
2. Create FP16 ONNX export for broader compatibility
3. Measure browser WASM latency with Playwright
4. Complete D4 accuracy evaluation
5. Test GLiNER2.5-Decide variants with their preprocessing
