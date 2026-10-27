# GLiNER2 Browser Benchmark Matrix

Comprehensive benchmark comparing GLiNER2 model variants for browser inference.

**Last updated:** 2026-10-27  
**Test environment:**
- Node.js: v22.23.4, onnxruntime-node 1.31.1
- Browser: Headless Chromium (Playwright), onnxruntime-web 1.31.1 (WASM backend)
- Native Python: gliner2 2.1.0, PyTorch 2.14.1
- Platform: Linux x86_64 VM, 4 CPUs
- Fixtures: 85 D4 test cases (42 PERSON, 38 STREET_ADDRESS entities)
- Scorer: Custom matching on label + start + end offsets (same rules for all runtimes)

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

### Native Python (gliner2 2.1.0, PyTorch, CPU)

| Model | Load Time | Inference | PERSON P | PERSON R | PERSON F1 | ADDR P | ADDR R | ADDR F1 | ADR-004 |
|-------|-----------|-----------|----------|----------|-----------|--------|--------|---------|---------|
| fastino PyTorch | 4.1s | 31ms | 60.7% | **88.1%** | **71.8%** | 57.9% | 28.9% | 38.6% | ✗ |

### ONNX Node.js (onnxruntime-node 1.31.1, CPU)

| Model | Size | Cold Load | Inference | PERSON P | PERSON R | PERSON F1 | ADDR P | ADDR R | ADDR F1 | ADR-004 |
|-------|------|-----------|-----------|----------|----------|-----------|--------|--------|---------|---------|
| okasi Q8 | 525 MB | 876 ms | 120 ms | 61.0% | **85.7%** | **71.3%** | 63.2% | 31.6% | 42.1% | ✗ |
| okasi Q4 | 473 MB | 746 ms | 333 ms | 64.9% | **88.1%** | **74.8%** | 70.6% | 31.6% | 43.6% | ✗ |

### Browser WASM (onnxruntime-web 1.31.1, headless Chromium)

| Model | Size | Cold Load | Inference* | PERSON P | PERSON R | PERSON F1 | ADDR P | ADDR R | ADDR F1 | ADR-004 |
|-------|------|-----------|------------|----------|----------|-----------|--------|--------|---------|---------|
| okasi Q8 | 525 MB | 3.5s | 530ms | 62.7% | **88.1%** | **73.3%** | 60.0% | 31.6% | 41.4% | ✗ |
| okasi Q4 | 473 MB | 3.3s | ~530ms | 64.9% | **88.1%** | **74.7%** | 80.0% | 31.6% | 45.3% | ✗ |

*Inference with multi-threading (SharedArrayBuffer + Cross-Origin Isolation). Single-threaded: ~1850ms.

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

## Threshold Sweep

### PERSON Recall by Threshold

| Threshold | Native PyTorch | ONNX Q8 | ONNX Q4 |
|-----------|----------------|---------|---------|
| 0.3 | 88.1% (37/42) | 85.7% (36/42) | — |
| 0.4 | 88.1% (37/42) | 85.7% (36/42) | — |
| 0.5 | 88.1% (37/42) | 85.7% (36/42) | 88.1% (37/42) |

Lowering threshold does NOT improve PERSON recall for either native or ONNX. The 5-6 missed entities are consistently below threshold at all tested values.

### STREET_ADDRESS Recall by Threshold

| Threshold | Native PyTorch | ONNX Q8 |
|-----------|----------------|---------|
| 0.3 | 28.9% (11/38) | 31.6% (12/38) |
| 0.4 | 28.9% (11/38) | 31.6% (12/38) |
| 0.5 | 28.9% (11/38) | 31.6% (12/38) |

Address detection is consistently poor at ~30% recall regardless of threshold.

## ADR-004 Compliance

**ADR-004 Requirement:** ≥90% PERSON recall for production use.

**Result:** No variant at any threshold clears ADR-004.
- Native PyTorch: 88.1% (best case)
- ONNX Q8: 85.7% (loses 1 TP to quantization)
- ONNX Q4: 88.1%

## Missed PERSON Entities (5)

These entities are consistently missed by native PyTorch at threshold=0.5:

| Fixture | Entity | Reason |
|---------|--------|--------|
| d4-person-0006 | "Müller, Hans" | Inverted format with non-ASCII |
| d4-person-0013 | "mcarthur.patricia" | Looks like username (dot-separated) |
| d4-person-0026 | "Gordon" | Single name only |
| d4-person-0035 | "WONG, DAVID MING-HO" | ALL CAPS with hyphens |
| d4-mixed-0002 | "Alexander Petrov-Müller" | Hyphenated with non-ASCII |

**ONNX Q8 loses one additional entity:** "Elena Vasquez" (d4-person-0012) — quantization artifact.

## Span Parity

### Native vs ONNX

| Metric | Native PyTorch | ONNX Q8 Node.js | ONNX Q8 Browser |
|--------|----------------|-----------------|-----------------|
| PERSON TP | 37 | 36 | 37 |
| PERSON FN | 5 | 6 | 5 |
| PERSON Recall | 88.1% | 85.7% | 88.1% |

**Key finding:** ONNX Q8 loses 1 PERSON true positive vs native (quantization artifact).
Browser and Q4 match native recall.

### Assessment

The 88.1% recall comes from the model itself, not the ONNX export or preprocessing.
The same 5 entities are missed by native PyTorch, and ONNX Q8 loses one additional entity
due to quantization.

**Rust core parity:** Not yet measured. The Rust core GLiNER2 integration (issue #87) was
fixed by PR #112 and is merged on main. Running the same D4 fixtures through the Rust path
would complete the parity assessment.

## Browser Latency Analysis

### Threading Impact

| Configuration | 1KB Inference | Notes |
|---------------|---------------|-------|
| Single-thread (no COI) | **1849 ms** | SharedArrayBuffer unavailable |
| Multi-thread (with COI) | **530 ms** | 4 threads, requires COOP/COEP headers |

**3.5x speedup** from enabling multi-threading via Cross-Origin Isolation.

### What Limits Latency

1. **Threading (primary bottleneck):** Without SharedArrayBuffer, WASM runs single-threaded.
   Enabling Cross-Origin Isolation headers enables multi-threaded WASM.

2. **SIMD:** Available in headless Chromium. Already used by onnxruntime-web.

3. **WebGPU:** Available in headless Chromium but not tested. onnxruntime-web supports
   WebGPU backend but requires separate configuration.

4. **Sequence length:** Latency scales with token count:
   - 41 tokens (26 chars): 282 ms
   - 55 tokens (88 chars): 279 ms
   - 173 tokens (417 chars): 530 ms
   - ~500+ tokens (1KB): ~530 ms (with COI)

### Enabling Multi-Threading

Add these HTTP headers to enable SharedArrayBuffer:
```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

**Warning:** These headers prevent embedding third-party resources without CORS.
For Chrome extensions, use the `cross_origin_embedder_policy` manifest key.

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
