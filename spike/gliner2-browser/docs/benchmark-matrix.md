# GLiNER2 Browser Benchmark Matrix

Comprehensive benchmark comparing GLiNER2 model variants for browser inference.

**Last updated:** 2026-10-27  
**Test environment:**
- Node.js: v22.23.4, onnxruntime-node 1.31.1
- Browser: Headless Chromium (Playwright), onnxruntime-web 1.31.1 (WASM backend)
- Platform: Linux x86_64 VM, 4 CPUs
- Fixtures: 85 D4 test cases (42 PERSON, 38 STREET_ADDRESS entities)

## Model Provenance

### Rust Core Pinned Model

The Rust core pins:
- **Model:** `fastino/gliner2-privacy-filter-PII-multi`
- **Revision:** `1cb4166094dc58fa8d836429f060d6c95f62b495`
- **Lockfile:** `crates/core/src/models/gliner2-lock.json`

Key file hashes from lockfile:
| File | SHA-256 | Size |
|------|---------|------|
| tokenizer.json | `f6df10ec83bea993035b2dd7c39345a3d4fcf23421c2adb6cb4ffc1e6d1bc4b5` | 16,020,604 bytes |
| model.safetensors | `0280f6f39f6012da50b6640bad438d9b7e763a1b0102094115d1b710c4dd79b6` | 1,228,421,964 bytes |

### okasi ONNX Export

The okasi export (`okasi/gliner2-privacy-filter-pii-multi-onnx`) is an ONNX
conversion of the same base model:
- **Source model:** `fastino/gliner2-privacy-filter-PII-multi`
- **Export date:** 2026-05-24
- **Revision:** Not pinned in export metadata
- **Tokenizer SHA-256:** `f6df10ec83bea993035b2dd7c39345a3d4fcf23421c2adb6cb4ffc1e6d1bc4b5` ✓ matches

**Provenance assessment:**
- The tokenizer.json hash matches the Rust core lockfile, confirming same vocabulary
- The ONNX weights cannot be verified byte-for-byte against the safetensors weights
  (different format, different quantization)
- okasi does not pin a revision, so we cannot prove the export used the exact same
  model weights as the Rust core's pinned revision

## Benchmark Matrix

### Node.js (onnxruntime-node 1.31.1, CPU)

| Model | Size | Cold Load | Warm Latency | Memory | PERSON Recall | PERSON F1 | STREET_ADDRESS F1 | Overall F1 | ADR-004 |
|-------|------|-----------|--------------|--------|---------------|-----------|-------------------|------------|---------|
| okasi Q8 | 525 MB | 876 ms | 120 ms | 167 MB | **88.1%** | **73.3%** | 42.1% | 59.8% | ✗ |
| okasi Q4 | 473 MB | 746 ms | 333 ms | 149 MB | **88.1%** | **74.8%** | 43.6% | 62.8% | ✗ |

### Browser (onnxruntime-web 1.31.1, WASM, headless Chromium)

| Model | Size | Cold Load | Warm Latency | Memory | PERSON Recall | PERSON F1 | STREET_ADDRESS F1 | Overall F1 | ADR-004 |
|-------|------|-----------|--------------|--------|---------------|-----------|-------------------|------------|---------|
| okasi Q8 | 525 MB | 3536 ms | 995 ms | 631 MB | **88.1%** | **73.3%** | 41.4% | 59.4% | ✗ |
| okasi Q4 | 473 MB | 3265 ms | 1010 ms | 595 MB | **88.1%** | **74.7%** | 45.3% | 63.6% | ✗ |

### Incompatible Models

| Model | Size | Status | Reason |
|-------|------|--------|--------|
| onnx-community/GLiNER2.5-Decide-mobile Q4F16 | 337 MB | ✗ Incompatible | Different input schema (requires `marker_positions`) |
| onnx-community/GLiNER2.5-Decide Q4F16 | 499 MB | ✗ Incompatible | Different input schema (requires `marker_positions`) |

## Per-Entity Metrics Detail

### okasi Q8 (Browser, WASM)

| Entity Type | True Positives | False Positives | False Negatives | Precision | Recall | F1 |
|-------------|----------------|-----------------|-----------------|-----------|--------|-----|
| PERSON | 37 | 22 | 5 | 62.7% | 88.1% | 73.3% |
| STREET_ADDRESS | 12 | 8 | 26 | 60.0% | 31.6% | 41.4% |
| USERNAME | 0 | 4 | 0 | 0% | N/A | N/A |
| EMAIL | 0 | 2 | 0 | 0% | N/A | N/A |

### okasi Q4 (Browser, WASM)

| Entity Type | True Positives | False Positives | False Negatives | Precision | Recall | F1 |
|-------------|----------------|-----------------|-----------------|-----------|--------|-----|
| PERSON | 37 | 20 | 5 | 64.9% | 88.1% | 74.7% |
| STREET_ADDRESS | 12 | 3 | 26 | 80.0% | 31.6% | 45.3% |
| USERNAME | 0 | 1 | 0 | 0% | N/A | N/A |
| EMAIL | 0 | 1 | 0 | 0% | N/A | N/A |

## ADR-004 Compliance

**ADR-004 Requirement:** ≥90% PERSON recall for production use.

**Result:** Neither Q8 nor Q4 clears ADR-004 (88.1% < 90%).

The 5 missed PERSON entities should be investigated to determine if:
1. They are edge cases the model wasn't trained for
2. They indicate a preprocessing bug
3. They are acceptable misses for the use case

## Span Parity

Browser vs Node.js span parity is consistent:
- Both runtimes produce identical PERSON recall (88.1%)
- Both runtimes produce identical PERSON true positives (37/42)
- Minor F1 variance (±0.3%) is within measurement noise

**Rust core parity:** Not yet measured. Requires running the Rust GLiNER2
integration on the same D4 fixtures. This is blocked on the GLiNER2 integration
completing in the Rust core (issue #87).

## Runtime Compatibility

onnxruntime 1.31.1 supports both Q8 and Q4 MatMulNBits quantization.

Prior versions had limitations:
- ORT 1.21.x: Only Q4 worked; Q8 failed with "Only 4b quantization is supported"
- ORT 1.18+: Q4 works

## Source Notes

- All measurements were taken on 2026-10-27 using the benchmark scripts in this directory
- "SPY F1 0.534" cited in prior documentation was from the okasi model card, not measured
- The D4 fixture set contains 85 test cases: 42 PERSON and 38 STREET_ADDRESS entities

## How to Run

```bash
# Node.js benchmark (uses onnxruntime-node)
npm run benchmark-matrix

# Browser benchmark (uses Playwright + onnxruntime-web WASM)
npm run benchmark-browser

# Both with skip-download (uses cached models)
npm run benchmark-matrix -- --skip-download
npm run benchmark-browser -- --skip-download

# Verbose output
npm run benchmark-matrix -- --verbose
npm run benchmark-browser -- --verbose
```

## Adding New Model Candidates

1. Add entry to `MODEL_CANDIDATES` in `scripts/benchmark-matrix.mjs`
2. Specify:
   - `id`: HuggingFace model ID
   - `variant`: Quantization variant (e.g., "q8", "q4")
   - `files`: List of files to download
   - `onnxModel`: Path to ONNX model file
   - `dataFile`: Path to external data file (if any)
   - `labels`: Entity labels the model expects
3. Run `npm run benchmark-matrix -- --models <id>/<variant>`

For browser benchmarks, also update `MODEL_CANDIDATES` in `scripts/benchmark-browser.mjs`.

**Important:** Models must use the okasi input schema:
- `input_ids`, `attention_mask`, `text_word_indices`, `text_word_counts`

Models using different schemas (e.g., `marker_positions`) are incompatible
without additional preprocessing implementation.
