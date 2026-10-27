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

**Finding:** ONNX Runtime Web with the WASM backend successfully loads and
initializes the GLiNER2 Q8 model in headless Chromium.

| Option | Status | Notes |
|--------|--------|-------|
| **onnxruntime-web (WASM)** | Works | Model loads in ~3.4s, inference requires proper tokenizer |
| **onnxruntime-web (WebGPU)** | Not tested | Chrome/Edge only, likely faster |
| **Rust ONNX compiled to WASM** | Not viable | ORT is too large to compile to WASM |

**Measured (headless Chromium, Linux x86_64 VM):**
- WASM runtime init: ~500ms
- Model loading (509 MB): ~3.4 seconds from localhost
- Total cold start: ~3.9 seconds

**Recommendation:** Use onnxruntime-web with WASM as primary backend. WebGPU can
be optional acceleration for Chrome/Edge users.

### 2. Model Size and Download Cost

**Source:** `okasi/gliner2-privacy-filter-pii-multi-onnx` (HuggingFace)

| File | Size | Purpose |
|------|------|---------|
| `model_q8.onnx` | 1.3 MB | ONNX graph definition |
| `model_q8.onnx.data` | 509 MB | Quantized weights |
| `tokenizer.json` | 16 MB | SentencePiece vocabulary |
| Other config | <1 MB | Labels, metadata |
| **Total** | **~526 MB** | |

**Comparison with other quantizations (from research):**

| Variant | Total Size | Accuracy (SPY F1) |
|---------|------------|-------------------|
| fp32 | ~1.23 GB | baseline (0.490) |
| fp16 | ~643 MB | ~same |
| Q8 (okasi) | ~526 MB | 0.534 (improved!) |
| Q4 (okasi) | ~458 MB | 0.516 |

**Finding:** Q8 is the best tradeoff. The ~526 MB download is acceptable for a
one-time cached load in a Chrome extension. The Q8 quantization actually
*improves* F1 on SPY benchmark (likely regularization effect).

### 3. Latency

**Measured (model loading only, inference incomplete):**

| Metric | Time | Notes |
|--------|------|-------|
| Cold start (model load) | 3350-3450ms | From localhost |
| Empty input inference | <1ms | Returns immediately |
| Non-empty inference | Not measured | Tokenizer issue |

**Why inference wasn't fully measured:**

The okasi ONNX export uses a simplified input interface:
- `input_ids`: tokenized text (int64)
- `attention_mask`: attention mask (int64)
- `text_word_indices`: start position of each word (int64)
- `text_word_counts`: number of words (int64)

This requires exact SentencePiece tokenization matching the model's training.
My minimal tokenizer implementation doesn't produce compatible token sequences,
resulting in shape/type errors during inference.

**Production path:** Use a proper SentencePiece tokenizer (e.g.,
`@xenova/transformers` or port the Python preprocessing).

### 4. Parity

**Blocked:** Parity testing requires working inference. The spike demonstrates:

1. Model loads successfully
2. WASM backend initializes correctly
3. External data files are handled properly
4. Empty inputs return empty results correctly

Full parity verification needs:
- Matching SentencePiece tokenizer
- Span decoding logic matching Python GLiNER2
- Count-floor fix for issue #87

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

## Recommendation

**GLiNER2 can run in the browser.** The model loads successfully via ONNX
Runtime Web, and the ~526 MB Q8 variant is acceptable for a Chrome extension.

**Key constraints:**
- ~526 MB one-time download (cacheable)
- ~3.5s cold start (model load)
- Requires proper SentencePiece tokenizer
- Single-threaded WASM unless cross-origin isolation is available

**Estimated production work:**
- Tokenizer: M (either port Python or integrate transformers.js)
- Session management: S
- Weight hosting/verification: S
- Count-floor verification/fix: S
- Parity testing: S
- WebGPU support: M (optional)

The production work should wait for the shared model-engine interface (step 6
in ROADMAP.md) so GLiNER2, DataFog, and future engines share the same WASM API.
