# Models

DataFog Core supports local model inference via ModelSession. Two backends are available: the DataFog native runtime and GLiNER2 via Python. Model sessions run on Rust, Python, and Node.js; WASM is rule-based only.

## Engine tiers

Model engines are categorized by ownership and CI behavior:

| Tier | Engine | CI on F1 regression | Owner |
|------|--------|---------------------|-------|
| **first_party** | datafog | > 1pt fails PR, ≤ 1pt warns | DataFog |
| **community** | gliner2 | warn-only, never fails PRs | Requires named owner in CODEOWNERS |

Contract checks block for all tiers: pinned revision verified by doctor (sha256 from each bundle's own manifest), engine loads behind its feature/config, spans have valid offsets matching across SDKs. Integration code (e.g. GLiNER2 entity-count gating in `crates/core`) is DataFog's regardless of tier and regression tests for it block.

## Sub-features

### Backends

| Backend | Model | Labels | Notes |
|---------|-------|--------|-------|
| DataFog native | DataFog/pii-en-65m | FIRST_NAME, LAST_NAME, EMAIL, PHONE, STREET_ADDRESS, CUSTOMER_ID, ACCOUNT_NUMBER | Prebuilt runtime, calibrated confidence |
| GLiNER2 | Caller-defined | Caller-defined labels with thresholds | Python runtime, raw scores |

### DataFog backend

The DataFog backend uses a prebuilt native executable with ONNX runtime. It produces calibrated confidence scores.

Model identity:
- Model: `DataFog/pii-en-65m`
- Model revision: `02cc6ca86a11dcb2b328861687770a22a7ea1b76`
- Runtime revision: `7cadd41e21f59b2a149e31378d073cf6ca057531`
- Adapter version: `datafog-native-v1`

### GLiNER2 backend

The GLiNER2 backend runs span extraction via Python. Labels and thresholds are caller-defined.

Configuration fields:
- `python`: path to Python interpreter with GLiNER2 dependencies
- `labels`: array of `{label, threshold, description?}`
- `intra_threads`: CPU thread count (default: 1)
- `chunk_size`: text window size (default: 384)
- `chunk_overlap`: window overlap (default: 64)

### Session lifecycle

1. **Open**: validate bundle, verify manifest digest, start worker
2. **Scan**: route entities to rules or model, combine findings
3. **Close**: stop worker, cleanup resources

Sessions are single-threaded. Concurrent scans return `model_busy`.

## How to get to it (user POV)

### Python

```python
import json
from datafog_core import ModelSession

# DataFog backend
config = {
    "bundle": "/opt/datafog/pii-bundle",
    "manifest_sha256": "<digest from provisioner>",
    "limits": {
        "max_input_bytes": 65536,
        "max_response_bytes": 1048576,
        "max_findings": 1024,
        "timeout_ms": 30000,
        "cleanup_timeout_ms": 1000
    }
}

with ModelSession.open(config) as session:
    # Default routing: rules + model-only labels
    result = session.scan("Contact John Smith at john@example.com")
    print(result.model)      # DataFog/pii-en-65m
    print(result.findings)   # Combined findings
    print(result.evidence)   # Source attribution

    # Select specific entities
    result = session.scan(
        "Contact John Smith at john@example.com",
        {"entities": ["EMAIL", "FIRST_NAME", "LAST_NAME"]}
    )

    # Raw model output only
    result = session.scan_model("Contact John Smith")

# GLiNER2 backend
gliner2_config = {
    "bundle": "/opt/datafog/gliner2-bundle",
    "manifest_sha256": "<digest>",
    "limits": {...},
    "gliner2": {
        "python": "/opt/datafog/gliner2-env/bin/python",
        "labels": [
            {"label": "street_address", "threshold": 0.5},
            {"label": "organization", "threshold": 0.6}
        ]
    }
}
```

### Node.js

```javascript
import { readFileSync } from "node:fs";
import { ModelSession } from "@datafog/node";

const config = JSON.parse(readFileSync("session.json"));
const session = await ModelSession.open(config);

try {
    const result = await session.scan("Contact John Smith at john@example.com");
    console.log(result.model, result.findings, result.evidence);

    // Routed scan
    const routed = await session.scan("...", {
        config: { entities: ["EMAIL", "STREET_ADDRESS"] }
    });

    // Raw model output
    const modelOnly = await session.scanModel("Contact John Smith");
} finally {
    await session.close();
}
```

### Rust

```rust
use datafog_core::models::{ModelSession, ModelSessionConfig, Cancellation};

let config: ModelSessionConfig = serde_json::from_str(&config_json)?;
let session = ModelSession::open(config)?;

// Default scan
let result = session.scan("Contact John Smith", &Cancellation::default())?;

// Routed scan
let routing = session.routed_config(&serde_json::json!({
    "entities": ["EMAIL", "FIRST_NAME"]
}))?;
let result = session.scan_routed("...", &routing, &Cancellation::default())?;

// Raw model
let raw = session.scan_model("...", &Cancellation::default())?;

session.close()?;
```

### Provisioning

DataFog model:

```bash
hf download DataFog/pii-en-65m \
  runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-<platform>.tar.gz \
  --revision 7cadd41e21f59b2a149e31378d073cf6ca057531 --local-dir /tmp/datafog-assets

python scripts/provision-datafog-model.py \
  --archive /tmp/datafog-assets/runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-<platform>.tar.gz \
  --destination /opt/datafog/pii-bundle
```

GLiNER2 model:

```bash
python3.12 -m venv /opt/datafog/gliner2-env
/opt/datafog/gliner2-env/bin/python -m pip install 'datafog-core[gliner2]'

hf download fastino/gliner2-privacy-filter-PII-multi \
  config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
  --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 --local-dir /tmp/gliner2-download

python scripts/provision-gliner2-model.py \
  --source /tmp/gliner2-download --destination /opt/datafog/gliner2-bundle
```

## Driving it with cargo dfv

```bash
cargo dfv doctor              # Check model_weights status
cargo dfv doctor --json       # Machine-readable, shows model file hashes
cargo dfv check --models      # Include model-based detectors
cargo dfv eval --models       # Run model evaluation suite
cargo dfv eval --model datafog   # Evaluate specific engine
cargo dfv eval --model gliner2   # Evaluate specific engine
```

## Gotchas

### Known email tokenization issue

The DataFog prebuilt runtime (`datafog-pii` from HF `DataFog/pii-en-65m` runtimes/v0.2.0) splits some emails incorrectly. For example:

Input: `r0000.specimen@rustpoc.test`

Model output:
- `r` at [31,32) — isolated single character
- `.specimen@rustpoc.test` at [36,58) — rest of email

The baseline stays provisional until this tokenization issue is resolved. Rule-based EMAIL detection handles this case correctly.

### Manifest verification

The `manifest_sha256` in session config must match the provisioner output. Doctor hashes files listed in `models/manifest.json` using sha256 but does not verify the model revision. If the manifest is missing, doctor warns but does not fail.

### Session concurrency

A ModelSession is single-threaded. Concurrent scan calls return `model_busy` error. Use separate sessions for parallel work.

### Cancellation

Scans support cancellation via `Cancellation` (Rust) or `ModelCancellation` (Python/Node). Pass a cancellation handle and call `cancel()` from another thread.

### Resource limits

All limits are required and caller-provided:
- `max_input_bytes`: maximum text length
- `max_response_bytes`: maximum serialized output
- `max_findings`: maximum finding count
- `timeout_ms`: inference deadline
- `cleanup_timeout_ms`: shutdown deadline

There are no library defaults. The values in docs are test budgets.

### Entity routing

`session.scan()` routes entities:
1. Rule detectors handle all supported entities (EMAIL, PHONE, etc.)
2. Model handles labels rules don't produce (FIRST_NAME, LAST_NAME, STREET_ADDRESS)
3. Both contribute to combined output

Selecting only rule-supported entities (e.g., `{"entities": ["EMAIL"]}`) skips model inference entirely.

### Platform support

Models run on:
- Linux x86_64 (GNU)
- Linux ARM64 (GNU)
- macOS ARM64 (14+)
- macOS Intel (15+)
- Windows x86_64 (MSVC)

WASM does not support models.

### GLiNER2 label mapping

GLiNER2 labels map to uppercase output types:
- `phone_number` → `PHONE` (special case)
- `street_address` → `STREET_ADDRESS`
- `organization` → `ORGANIZATION`

Empty, whitespace-padded, or duplicate labels fail validation.

### Score interpretation

DataFog provides calibrated `confidence` in [0.0, 1.0]. GLiNER2 provides raw `raw_score` values; findings have no `confidence` field. Evidence records preserve both scales and their source detector.
