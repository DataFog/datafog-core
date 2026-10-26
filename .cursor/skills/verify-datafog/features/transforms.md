# Transforms

DataFog Core transforms detected PII using five strategies. Transformations produce audit records with source/output ranges and replacement metadata, without the original matched text.

## Sub-features

### Strategies

| Strategy | Output | Provider | WASM Support |
|----------|--------|----------|--------------|
| `redact` | `[EMAIL]` | None | Yes |
| `mask` | `j***@***.com` | None | Yes |
| `remove` | (deleted span) | None | Yes |
| `pseudonymize` | Keyed HMAC-SHA-256 pseudonym | Key provider | No |
| `tokenize` | Opaque reversible token | Token provider | No |

### Redact

Replaces the finding with its entity-type placeholder. No numbering.

```json
{"transform": {"default": {"strategy": "redact"}}}
```

Input: `Email jane@example.com`
Output: `Email [EMAIL]`

### Mask

Replaces characters with a mask character, optionally revealing first or last characters.

```json
{
  "transform": {
    "default": {
      "strategy": "mask",
      "mask_char": "*",
      "reveal_first": 1,
      "reveal_last": 0
    }
  }
}
```

Input: `Email jane@example.com`
Output: `Email j***************`

### Remove

Deletes the matched span entirely.

```json
{"transform": {"default": {"strategy": "remove"}}}
```

Input: `Email jane@example.com`
Output: `Email `

### Pseudonymize

Creates a deterministic keyed pseudonym using HMAC-SHA-256. Requires a key provider.

```json
{
  "transform": {
    "default": {
      "strategy": "pseudonymize",
      "key_ref": "pii-key-2024",
      "key_version": "v1"
    }
  }
}
```

The same input and key produce the same pseudonym. Different keys produce different pseudonyms.

### Tokenize

Issues an opaque token that can be restored with the same scope. Requires a token provider.

```json
{
  "transform": {
    "default": {
      "strategy": "tokenize",
      "token_ref": "email-tokens"
    }
  }
}
```

Tokens are reversible via `restore()` with the same PrivacyContext scope.

## How to get to it (user POV)

### Python

```python
from datafog_core import scan_and_transform, transform, PrivacyManager

# Simple redaction
result = scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "redact"}}}
)
print(result.text)  # Email [EMAIL]

# Transform explicit findings
findings = scan("Email jane@example.com")
result = transform(
    "Email jane@example.com",
    findings,
    {"default": {"strategy": "mask", "mask_char": "*"}}
)

# Provider-backed strategies (async)
manager = PrivacyManager(provider=key_provider, token_provider=token_provider)
result = await manager.scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "pseudonymize", "key_ref": "k1"}}},
    context={"scope": "tenant-123"}
)
```

### Node.js

```javascript
import { scanAndTransform, transform } from "@datafog/node";

const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } }
});
console.log(result.text);  // Email [EMAIL]
```

### Rust

```rust
use datafog_core::{
    scan_and_transform, ScanAndTransformConfig,
    TransformationConfig, TransformationStrategy,
};

let result = scan_and_transform(
    "Email jane@example.com",
    &ScanAndTransformConfig::new(TransformationConfig::new(
        TransformationStrategy::Redact,
    )),
)?;
println!("{}", result.text);  // Email [EMAIL]
```

### Browser/WASM

```javascript
import { init, scanAndTransform } from "@datafog/wasm";

await init();
const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } }
});
console.log(result.text);  // Email [EMAIL]
```

## Driving it with cargo dfv

```bash
cargo dfv check           # Runs transformation tests
cargo dfv check --json    # Machine-readable output
cargo dfv parity          # Verifies output range consistency
```

Transformation tests verify:
- Output text matches expected result
- Source and output ranges are valid
- Audit records capture strategy and replacement

## Gotchas

### Per-entity overrides

The `overrides` field applies strategies by entity type:

```json
{
  "transform": {
    "default": {"strategy": "redact"},
    "overrides": {
      "EMAIL": {"strategy": "mask", "mask_char": "*"},
      "SSN": {"strategy": "remove"}
    }
  }
}
```

### Allowlists

The `allowlist` field preserves specific values:

```json
{
  "transform": {
    "default": {"strategy": "redact"},
    "allowlist": ["support@example.com"]
  }
}
```

Allowlisted findings are skipped during transformation.

### Provider requirements

- `pseudonymize` requires a key provider that implements `resolve_key(key_ref, key_version)`
- `tokenize` requires a token provider that implements `tokenize_batch()` and `restore_batch()`
- Both require a `PrivacyContext` with a non-empty scope

### WASM limitations

Browser/WASM bindings do not support `pseudonymize` or `tokenize` because they require provider callbacks that cannot run in the browser sandbox.

### Structured transforms

Structured data transforms work path-by-path. Each path's text is transformed independently, and the output maintains JSON structure:

```python
from datafog_core import scan_and_transform_structured

data = {"email": "jane@example.com", "name": "Jane Doe"}
result = scan_and_transform_structured(
    data,
    {"transform": {"default": {"strategy": "redact"}}}
)
# result.data = {"email": "[EMAIL]", "name": "Jane Doe"}
```

### Range coordinate systems

Transformation records include:
- `source_byte_range`: UTF-8 byte offsets in input
- `source_codepoint_range`: Unicode codepoint offsets in input
- `output_byte_range`: UTF-8 byte offsets in output
- `output_codepoint_range`: Unicode codepoint offsets in output

Node.js also provides `source_utf16_range` and `output_utf16_range` for JavaScript string indexing.
