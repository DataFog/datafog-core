# GLiNER2 Browser Spike

**Status:** Spike/Investigation  
**Purpose:** Evaluate GLiNER2 in-browser inference via ONNX Runtime Web

This spike investigates whether GLiNER2 can run in the browser with acceptable
speed and accuracy, and what WASM support would require for production.

## Quick Start

```bash
cd spike/gliner2-browser
npm install
npm run download-model  # Downloads ~526 MB Q8 model
npx playwright install chromium
npm test                # Run automated tests
```

## Summary of Findings

### 1. Runtime Path: onnxruntime-web (WASM) is viable

**Finding:** ONNX Runtime Web 1.31.1 with the WASM backend successfully runs
GLiNER2 Q8 and Q4 inference in headless Chromium with real accuracy metrics.

| Option | Status | Notes |
|--------|--------|-------|
| **onnxruntime-web (WASM)** | ✅ Works | Full inference working with transformers.js tokenizer |
| **onnxruntime-web (WebGPU)** | Not tested | Chrome/Edge only, likely faster |
| **Rust ONNX compiled to WASM** | Not viable | ORT is too large to compile to WASM |

**Measured (headless Chromium, Linux x86_64 VM, ORT 1.31.1):**
- Cold load (model + session): 3.3-3.5s
- Warm inference latency: ~1000ms per 1KB text
- Peak memory: ~600 MB

**Recommendation:** Use onnxruntime-web 1.31.1+ with WASM as primary backend.

### 2. Model Size and Download Cost

**Source:** `okasi/gliner2-privacy-filter-pii-multi-onnx` (HuggingFace)

| File | Size | Purpose |
|------|------|---------|
| `model_q8.onnx` | 1.3 MB | ONNX graph definition |
| `model_q8.onnx.data` | 509 MB | Quantized weights |
| `tokenizer.json` | 16 MB | SentencePiece vocabulary |
| Other config | <1 MB | Labels, metadata |
| **Total** | **~526 MB** | |

**Measured accuracy on D4 fixtures (85 test cases, PERSON + STREET_ADDRESS):**

| Variant | Size | PERSON Recall | PERSON F1 | Overall F1 | ADR-004 |
|---------|------|---------------|-----------|------------|---------|
| Q8 (okasi) | 525 MB | 88.1% | 73.3% | 59.4% | ✗ |
| Q4 (okasi) | 473 MB | 88.1% | 74.7% | 63.6% | ✗ |

**Finding:** Q4 slightly outperforms Q8 on these fixtures (better precision).
Neither clears ADR-004's 90% PERSON recall threshold, but both achieve 88.1%.

### 3. Latency

**Measured (full inference, D4 fixtures):**

| Runtime | Cold Load | Warm Latency (1KB) | Memory |
|---------|-----------|-------------------|--------|
| Node.js (ORT 1.31.1, CPU) | 750-880 ms | 120-330 ms | 150-170 MB |
| Browser (ORT 1.31.1, WASM) | 3.3-3.5 s | ~1000 ms | ~600 MB |

**Browser/Node ratio:** Browser is ~3.5x slower for cold load and ~8x slower
for inference due to WASM overhead.

### 4. Parity

**Browser vs Node.js:** Verified on D4 fixtures.

| Metric | Node.js | Browser | Status |
|--------|---------|---------|--------|
| PERSON Recall | 88.1% | 88.1% | ✅ Match |
| PERSON F1 | 73.3% | 73.3% | ✅ Match |
| STREET_ADDRESS F1 | 42.1% | 41.4% | ~Match (±0.7%) |

**Rust core parity:** Not yet measured. Requires running the Rust GLiNER2
integration on the same D4 fixtures (blocked on issue #87).

### 5. License and Redistribution

**GLiNER2 code:** Apache-2.0  
**GLiNER2 v2+ weights (NuNER-trained):** Apache-2.0  
**fastino/gliner2-privacy-filter-PII-multi:** Apache-2.0  
**okasi ONNX export:** Apache-2.0

**Requirements for redistribution:**
- Include Apache-2.0 license and copyright notices
- Include NOTICE file if present
- Mark any modifications

**Constraint:** The ~526 MB weights cannot be bundled in an npm package.
They must be hosted separately and fetched at runtime.

### 6. Production Integration Shape

**Recommendation:** The production integration should wait for the shared
model-engine interface (ROADMAP.md step 6). Key decisions:

**API surface in `bindings/wasm`:**

```typescript
interface ModelSession {
  static open(config: ModelSessionConfig): Promise<ModelSession>;
  scan(text: string, config?: ScanConfig): Promise<ModelScanResult>;
  scanModel(text: string): Promise<ModelScanResult>;
  close(): Promise<void>;
}

interface ModelSessionConfig {
  modelUrl: string;
  manifestSha256: string;
  gliner2: {
    labels: Array<{label: string; threshold: number}>;
  };
  limits: {
    maxInputBytes: number;
    maxFindings: number;
    timeoutMs: number;
  };
}
```

**Weight hosting:**

```
https://cdn.datafog.io/models/gliner2-pii-multi/v1/
├── manifest.json        # SHA-256 hashes of all files
├── model_q8.onnx
├── model_q8.onnx.data
├── tokenizer.json
└── ...
```

The WASM binding should:
1. Download and verify manifest SHA-256
2. Download each file and verify against manifest
3. Cache in IndexedDB for future loads
4. Load ONNX session with external data support

**Integration with `cargo dfv parity --live`:**

Add a `wasm-model` variant to the parity runner that:
1. Loads ModelSession in headless Chromium
2. Runs scanModel() on fixture texts
3. Compares spans to Python/Node model outputs

## Blocking Issues for Production

1. **Tokenizer implementation:** The SentencePiece tokenizer must produce
   exact token sequences. Options:
   - Port Python tokenization to JS
   - Use `@xenova/transformers` tokenizer
   - Use Transformers.js AutoTokenizer

2. **Count-floor fix (issue #87):** The browser runtime must implement the
   same floor that the Python worker applies. Either:
   - Patch the ONNX graph before export
   - Apply floor in JS post-processing
   - Verify okasi export already includes the fix

3. **WebGPU testing:** The spike only tested WASM. WebGPU would be faster
   but requires additional testing on real hardware.

## Files

```
spike/gliner2-browser/
├── package.json
├── scripts/
│   ├── download-model.mjs   # Fetch from HuggingFace
│   ├── serve.mjs            # Local dev server
│   └── run-tests.mjs        # Automated browser tests
├── src/
│   ├── tokenizer.mjs        # Minimal tokenizer (incomplete)
│   └── gliner2-session.mjs  # ONNX session wrapper
├── public/
│   ├── index.html           # Interactive demo
│   └── test-harness.html    # Test page
└── models/                  # Downloaded files (gitignored)
```

## Benchmark Matrix

See [docs/benchmark-matrix.md](docs/benchmark-matrix.md) for the full benchmark
matrix comparing GLiNER2 model variants.

### Model Variants Evaluated

| Model | Size | Runtime | PERSON F1 | Notes |
|-------|------|---------|-----------|-------|
| okasi Q8 | 525 MB | ORT 1.22+ | 73.3% | Working, measured |
| okasi Q4 | 473 MB | ORT 1.18+ | 74.7% | Working, measured |
| GLiNER2.5-mobile Q4F16 | 337 MB | — | — | ✗ Incompatible input schema |

**Note:** GLiNER2.5 models use a different input schema (`marker_positions`)
that is incompatible with our okasi-style preprocessor.

### Tokenization Status

**Working:** Transformers.js tokenization now matches the Python reference:

```bash
npm run verify-tokenization
# ✓ Token IDs MATCH exactly!
# ✓ Word indices MATCH exactly!
# ✓ Start mapping MATCH exactly!
# ✓ End mapping MATCH exactly!
```

The preprocessing uses the GLiNER2 schema format:
```
( [P] entities ( [E] name [E] address ... ) ) [SEP_TEXT] lowercased tokens...
```

### Runtime Compatibility

The okasi ONNX export uses **MatMulNBits** for weight quantization:

| Quantization | Min ORT Version | Node Status | Browser Status |
|--------------|-----------------|-------------|----------------|
| Q4 (4-bit) | 1.18 | ✅ Works | ✅ Works |
| Q8 (8-bit) | 1.22 | ✅ Works | ✅ Works |

**Resolved:** onnxruntime 1.31.1 supports both Q4 and Q8 MatMulNBits.
Prior versions (1.21.x) only supported Q4.

### Running the Benchmark

```bash
npm run benchmark-matrix                    # All models
npm run benchmark-matrix -- --skip-download # Use cached
npm run benchmark-matrix -- --verbose       # Debug output
```

## Recommendation

**GLiNER2 can run in the browser.** Full inference is working with measured
accuracy, and both Q8 and Q4 variants are viable for a Chrome extension.

**Neither variant clears ADR-004** (88.1% PERSON recall < 90% threshold).

**If proceeding despite ADR-004:**
- **Sweet Spot:** okasi Q4 (~473 MB) — slightly better F1, smaller download
- **Alternative:** okasi Q8 (~525 MB) — larger but same recall

The Q4 variant slightly outperforms Q8 on our test fixtures (74.7% vs 73.3%
PERSON F1), likely due to better regularization from quantization.

**Key constraints:**
- ~473-525 MB one-time download (cacheable)
- ~3.5s cold start in browser
- ~1s inference per 1KB text in browser
- Requires onnxruntime-web 1.31.1+
- Requires transformers.js for tokenization

**Production readiness:**
- Tokenizer: ✅ Done (transformers.js integration)
- Inference: ✅ Working (full accuracy measured)
- Parity (browser/Node): ✅ Verified
- ADR-004: ✗ Not met (88.1% < 90%)
- Rust core parity: ⏳ Blocked on issue #87

The production work should wait for the shared model-engine interface (step 6
in ROADMAP.md) so GLiNER2, DataFog, and future engines share the same WASM API.
