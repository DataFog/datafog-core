# SDK Surfaces

DataFog Core provides consistent APIs across Rust, Python, Node.js, and Browser/WASM. Detection semantics live in `crates/core`; bindings are thin wrappers that expose the same functionality.

## Sub-features

### Text APIs

| Function | Purpose | All SDKs |
|----------|---------|----------|
| `scan` | Detect PII in text | Yes |
| `transform` | Transform explicit findings | Yes |
| `scan_and_transform` | Detect and transform in one call | Yes |
| `capabilities` | Query supported entities and config | Yes |

### Structured APIs

| Function | Purpose | All SDKs |
|----------|---------|----------|
| `discover_fields` | Find name fields in JSON | Yes |
| `scan_structured` | Detect PII in JSON data | Yes |
| `transform_structured` | Transform structured findings | Yes |
| `scan_and_transform_structured` | Detect and transform JSON | Yes |

### Provider APIs

| Function | Purpose | WASM |
|----------|---------|------|
| `PrivacyManager` | Async provider-backed transforms | No |
| `required_key_selectors` | Query keys needed for transform | No |
| `required_tokenization_items` | Query tokens needed | No |
| `transform_with_resolved_keys` | Transform with pre-resolved keys | No |
| `transform_with_provider_results` | Transform with pre-resolved providers | No |
| `restore` | Reverse tokenization | No |

WASM does not support provider-backed strategies (`pseudonymize`, `tokenize`) or `restore()`. All three throw `unsupported_strategy`. The functions exist in the API for type parity but are not usable.

### Model APIs (Rust, Python, Node only)

| Function | Purpose | Notes |
|----------|---------|-------|
| `ModelSession.open` | Open a model session | Config with bundle, limits |
| `session.scan` | Scan with rules + model | Routes by entity type |
| `session.scan_model` | Raw model output only | No rule detectors |
| `session.close` | Close and cleanup | Safe to repeat |

## How to get to it (user POV)

### Rust

```rust
use datafog_core::{
    scan, scan_with_config, scan_and_transform,
    transform, capabilities, ScanConfig,
    ScanAndTransformConfig, TransformationConfig,
    TransformationStrategy, Finding, TextRange,
};
use datafog_core::structured::{
    scan as scan_structured, discover_fields,
    scan_and_transform as scan_and_transform_structured,
};
#[cfg(feature = "models")]
use datafog_core::models::{ModelSession, ModelSessionConfig};

// Text scanning
let findings: Vec<Finding> = scan("Email jane@example.com");
let config = ScanConfig::new().with_entities(vec!["EMAIL".into()]).unwrap();
let selected = scan_with_config("Email jane@example.com", &config);

// Text transformation
let result = scan_and_transform(
    "Email jane@example.com",
    &ScanAndTransformConfig::new(TransformationConfig::new(
        TransformationStrategy::Redact,
    )),
)?;

// Structured scanning
let data: serde_json::Value = serde_json::json!({"email": "jane@example.com"});
let structured = scan_structured(&data, &Default::default())?;

// Capabilities
let caps = capabilities();

// Models (with feature = "models")
#[cfg(feature = "models")]
{
    let session = ModelSession::open(config)?;
    let result = session.scan("Contact John at john@example.com", &Default::default())?;
    session.close()?;
}
```

### Python

```python
from datafog_core import (
    scan, transform, scan_and_transform, capabilities,
    scan_structured, discover_fields, scan_and_transform_structured,
    Finding, TextRange, PrivacyManager
)

# Text scanning
findings = scan("Email jane@example.com")
findings = scan("Email jane@example.com", {"entities": ["EMAIL"]})

# Text transformation
result = scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "redact"}}}
)
print(result.text)
print(result.transformations)

# Structured scanning
data = {"email": "jane@example.com"}
result = scan_structured(data)
fields = discover_fields(data)

# Capabilities
caps = capabilities()
print(caps["supported_entities"])

# Provider-backed (async)
manager = PrivacyManager(provider=key_provider)
result = await manager.transform(text, findings, config, context)

# Models
from datafog_core import ModelSession
with ModelSession.open(config) as session:
    result = session.scan("Contact John at john@example.com")
```

### Node.js

```javascript
import {
  scan, transform, scanAndTransform, discoverFields,
  scanStructured, scanAndTransformStructured, transformStructured,
  ModelSession, DataFogError
} from "@datafog/node";

// Text scanning
const findings = scan("Email jane@example.com");
const selected = scan("Email jane@example.com", { entities: ["EMAIL"] });

// Text transformation
const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } }
});
console.log(result.text);
console.log(result.transformations);

// Structured scanning (takes JsonDocument, not JSON string)
const data = { email: "jane@example.com" };
const structured = scanStructured(data);
const fields = discoverFields(data);
const transformed = scanAndTransformStructured(data, {
  transform: { default: { strategy: "redact" } }
});

// Models
const session = await ModelSession.open(config);
try {
  const result = await session.scan("Contact John at john@example.com");
} finally {
  await session.close();
}

// Error handling
try {
  transform("...", [], { default: { strategy: "pseudonymize", key_ref: "k" } });
} catch (e) {
  if (e instanceof DataFogError) {
    console.log(e.code, e.reason);
  }
}
```

### Browser/WASM

```javascript
import {
  init, scan, transform, scanAndTransform, restore,
  discoverFields, scanStructured, transformStructured, scanAndTransformStructured,
  DataFogError
} from "@datafog/wasm";

await init();  // Required before any other call

// Text scanning
const findings = scan("Email jane@example.com");

// Text transformation
const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } }
});

// Structured scanning (takes JsonDocument, not JSON string)
const data = { email: "jane@example.com" };
const structured = scanStructured(data);
const transformed = scanAndTransformStructured(data, {
  transform: { default: { strategy: "redact" } }
});
```

## Driving it with cargo dfv

```bash
cargo dfv build               # Build all bindings
cargo dfv build --sdk python  # Build Python only
cargo dfv build --sdk node    # Build Node only
cargo dfv build --sdk wasm    # Build WASM only
cargo dfv check               # Test all SDKs
cargo dfv parity              # Cross-SDK range comparison
```

The check command runs:
- Rust: `cargo test --workspace --all-features`
- Python: `bindings/python/tests/test_installed.py`
- Node: `npm run test:package` in `bindings/node/`
- WASM: `npm run test:package` in `bindings/wasm/`

## Gotchas

### WASM initialization

Browser/WASM requires `await init()` before any other function call. The init function loads the WebAssembly module.

### Range coordinate systems

| SDK | Byte | Codepoint | UTF-16 |
|-----|------|-----------|--------|
| Rust | Yes | Yes | No |
| Python | Yes | Yes | No |
| Node.js | Yes | Yes | Yes |
| WASM | Yes | Yes | Yes |

Node.js and WASM provide `utf16_range` for JavaScript string indexing.

### JSON input formats

- Rust: `serde_json::Value`
- Python: `dict` or `list` (JSON-compatible types only)
- Node.js: `JsonDocument` (array or object, not a JSON string)
- WASM: `JsonDocument` (array or object, not a JSON string)

Node.js and WASM structured APIs take JavaScript objects/arrays directly (`JsonDocument`), not serialized JSON strings.

### Error types

| SDK | Error Class | Notes |
|-----|-------------|-------|
| Rust | `PrivacyError` | Single error type with `code` discriminant |
| Python | `DataFogConfigurationError`, `DataFogFindingError`, `DataFogKeyProviderError`, `DataFogInternalError` | Separate exception classes |
| Node.js | `DataFogError` | Single class with `code`, `reason`, `path`, `findingIndex` |
| WASM | `DataFogError` | Single class with `code`, `reason`, `path`, `findingIndex` |

All errors include `code` and optional `reason`, `path`, and `findingIndex`/`finding_index`.

### Async vs sync

| SDK | Text APIs | Provider APIs | Model APIs |
|-----|-----------|---------------|------------|
| Rust | Sync | Async | Sync |
| Python | Sync | Async | Sync |
| Node.js | Sync | Sync | Async |
| WASM | Sync | N/A | Async |

Python `PrivacyManager` methods are async. Node.js and WASM `ModelSession.open`, `scan`, and `close` return Promises.

### Platform binaries

Python and Node.js packages include prebuilt binaries for:
- Linux x64 (GNU)
- Linux ARM64 (GNU)
- macOS Intel
- macOS Apple Silicon
- Windows x64 (MSVC)

WASM runs in any browser with WebAssembly support.

### Model feature gate

Rust model APIs require the `models` feature:

```toml
datafog-core = { version = "0.4", features = ["models"] }
```

Browser/WASM model support is in the optional `@datafog/wasm/models` entry point using GLiNER2.5 Small ONNX. See `docs/browser-model.md`.
