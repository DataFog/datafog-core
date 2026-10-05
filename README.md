# DataFog Core

Fast structured PII detection, implemented in Rust and exposed for Rust, Python, Node.js, and browsers.

It detects `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`. Version 0.4.0 also detects `JWT` tokens by default; see the [JWT reference](docs/reference/jwt.mdx). With an explicit German locale, it also detects `DE_IBAN`, `DE_VAT_ID`, `DE_TAX_ID`, `DE_SOCIAL_SECURITY_NUMBER`, `DE_POSTAL_CODE`, `DE_PASSPORT_NUMBER`, and `DE_RESIDENCE_PERMIT_NUMBER`. Complete PEM private-key blocks are also detected as `PRIVATE_KEY` by default; see the [private-key reference](docs/reference/private-keys.mdx). Context-labeled `US_ROUTING_NUMBER` detection is also available; see [the detector rules](docs/reference/us-routing-number.mdx). Context-labeled `NPI` detection is also available; see [the detector rules](docs/reference/npi.mdx). Every binding returns the same finding information:

```text
entity type, matched text, byte range, code-point range,
optional confidence, detector name, optional detector version
```

Structured JSON scanning additionally discovers `PERSON` from documented name-field
aliases or explicit JSON Pointer mappings. It scans every string value with the
existing detectors and returns field paths plus string-local findings. No model
or dictionary download is needed. See [person-field discovery](docs/guides/person-discovery.mdx)
for `scan_structured` / `scanStructured` and structured transformation APIs.

Both ranges use zero-based, end-exclusive offsets. The byte range addresses the
UTF-8 input; the code-point range addresses Unicode scalar values. Rule-based
detectors currently report no confidence score. Node.js and browser WASM also
return an explicitly named UTF-16 code-unit range that can be passed directly
to JavaScript `String.prototype.slice`.

The transformation strategies are `redact`, `mask`, `remove`, `pseudonymize`,
and `tokenize` in Rust, Python, and Node.js.
Redaction uses an unnumbered `[ENTITY_TYPE]` placeholder, masking supports full
or leading/trailing reveal modes, and removal deletes only the exact finding
span. Pseudonymization uses provider-resolved 256-bit keys and deterministic
HMAC-SHA-256 tokens. Tokenization uses an application-supplied asynchronous
provider to issue opaque `DFTOKENv1(...)` envelopes and restore them under an
exact request-level scope. Both provider-backed strategies are deliberately
unsupported in browser WASM.
`transform` requires explicit findings; `scan_and_transform` (or
`scanAndTransform` in JavaScript) is the explicit scan-then-transform
convenience. Results include the transformed text and an ordered record for
every applied replacement, including its source metadata and output byte and
code-point ranges. Node.js and browser WASM additionally return source and
output UTF-16 ranges. Transformation records never include the original
matched text.

Transformation calls require an envelope with a default strategy. It can also
select entity types, override the strategy per entity, and exempt exact or
full-match regex values:

```js
{
  default: { strategy: "redact" },
  entities: ["EMAIL", "PHONE"],
  overrides: {
    PHONE: { strategy: "mask", reveal: { direction: "last", count: 4 } },
  },
  allow: {
    exact: { EMAIL: ["support@example.com"] },
    regex: { EMAIL: [{ pattern: ".+@example\\.org" }] },
  },
}
```

`scan_and_transform` uses `{ scan?: { locale?: string, detect_uuid?: boolean, format?: "text" | "env" | "sql" }, transform: ... }` so
detection settings remain separate from transformation policy.
For `.env` assignments or standard SQL quoted strings, select `format: "env"`
or `format: "sql"` to preserve email boundaries. See the
[email source-context policy](docs/guides/email-boundaries.mdx) for supported
syntax, raw escaped spans, and limitations. Default text scanning is unchanged.

## Packages

| Runtime | Distribution | Import | Status |
| --- | --- | --- | --- |
| Rust | [`datafog-core`](https://crates.io/crates/datafog-core) | `datafog_core` | Published |
| Python | [`datafog-core`](https://pypi.org/project/datafog-core/) | `datafog_core` | Published |
| Node.js | `@datafog/node` | `@datafog/node` | Published |
| Browser/WASM | `@datafog/wasm` | `@datafog/wasm` | Published |

## Migrating from DataFog Python

DataFog Core is a separate distribution and canonical API, not a drop-in
replacement for the established `datafog` Python package.

| DataFog Python 4.8.x | DataFog Core 0.3.x |
| --- | --- |
| `pip install datafog` | `pip install datafog-core` |
| `from datafog.engine import ...` | `from datafog_core import ...` |
| `scan(...).entities` | `scan(...)` returns `list[Finding]` |
| `scan_and_redact(...)` | `scan_and_transform(...)` |
| `result.redacted_text` | `result.text` |
| `Entity.type`, `.text`, `.start`, `.end` | `Finding.entity_type`, `.matched_text`, `.byte_range`, `.codepoint_range` |

Do not mechanically rename legacy `token` to Core `tokenize`: Core tokenization
is provider-backed and reversible. For non-reversible output, use `redact`,
`mask`, or `remove`; use keyed `pseudonymize` when stable linkage is required.

See the dedicated [DataFog Python migration
guide](docs/guides/migrating-from-datafog-python.mdx) for the API mapping,
strategy differences, entity names, range semantics, and migration checklist.

## Quick start

### Rust

```bash
cargo add datafog-core
```

```rust
use datafog_core::{
    scan, scan_and_transform, ScanAndTransformConfig, TransformationConfig,
    TransformationStrategy,
};

let findings = scan("Email jane@example.com");
assert_eq!(findings[0].entity_type, "EMAIL");
assert_eq!(findings[0].matched_text, "jane@example.com");
assert_eq!(findings[0].byte_range.start, 6);

let result = scan_and_transform(
    "Email jane@example.com",
    &ScanAndTransformConfig::new(TransformationConfig::new(
        TransformationStrategy::Redact,
    )),
).unwrap();
assert_eq!(result.text, "Email [EMAIL]");
```

### Python

```bash
python -m pip install datafog-core
```

```python
import asyncio

from datafog_core import PrivacyManager, scan, scan_and_transform

findings = scan("Email jane@example.com")
print(findings[0].entity_type)       # EMAIL
print(findings[0].matched_text)      # jane@example.com
print(findings[0].byte_range.start)  # 6

result = scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "redact"}}},
)
assert result.text == "Email [EMAIL]"

masked = scan_and_transform(
    "Email jane@example.com",
    {
        "transform": {
            "default": {
                "strategy": "mask",
                "reveal": {"direction": "last", "count": 4},
            }
        }
    },
)
assert masked.text == "Email ************.com"

class KeyProvider:
    async def resolve_key(self, key_ref, key_version):
        return {"key": load_32_byte_key(key_ref, key_version), "resolved_version": "7"}

async def pseudonymize():
    return await PrivacyManager(KeyProvider()).scan_and_transform(
        "Email jane@example.com",
        {
            "transform": {
                "default": {"strategy": "pseudonymize", "key_ref": "customers/email"}
            }
        },
    )

pseudonymized = asyncio.run(pseudonymize())

# A token provider implements tokenize_batch(scope, items) and
# restore_batch(scope, items). It owns storage or reversible cryptography,
# authorization, lifecycle, and audit.
token_manager = PrivacyManager(None, token_provider=TokenProvider())
tokenized = asyncio.run(token_manager.scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "tokenize", "token_ref": "customers/default"}}},
    {"scope": "tenant-a"},
))
restored = asyncio.run(token_manager.restore(tokenized.text, {"scope": "tenant-a"}))
```

### Node.js

Install the native Node.js package:

```bash
npm install @datafog/node
```

```js
import { PrivacyManager, scan, scanAndTransform } from "@datafog/node";

console.log(scan("Email jane@example.com"));
console.log(
  scanAndTransform("Email jane@example.com", {
    transform: { default: { strategy: "redact" } },
  }).text,
);

const manager = new PrivacyManager({
  async resolveKey({ keyRef, keyVersion }) {
    return { key: await load32ByteKey(keyRef, keyVersion), resolvedVersion: "7" };
  },
});
const pseudonymized = await manager.scanAndTransform("Email jane@example.com", {
  transform: {
    default: { strategy: "pseudonymize", key_ref: "customers/email" },
  },
});

const tokenManager = new PrivacyManager({ tokenProvider });
const tokenized = await tokenManager.scanAndTransform(
  "Email jane@example.com",
  { transform: { default: { strategy: "tokenize", token_ref: "customers/default" } } },
  { scope: "tenant-a" },
);
const restored = await tokenManager.restore(tokenized.text, { scope: "tenant-a" });
```

The release includes prebuilt binaries for macOS (Intel and Apple Silicon), Linux (x64 and ARM64), and Windows x64.

### Browser / WASM

Install the browser/WASM package:

```bash
npm install @datafog/wasm
```

```js
import { init, scan, scanAndTransform } from "@datafog/wasm";

await init();
console.log(scan("Email jane@example.com"));
console.log(
  scanAndTransform("Email jane@example.com", {
    transform: { default: { strategy: "redact" } },
  }).text,
);
```

## Development

```bash
cargo test --workspace
```

To exercise an installed binding package locally:

```bash
npm ci --prefix bindings/node
npm run test:package --prefix bindings/node

rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
npm ci --prefix bindings/wasm
npx --prefix bindings/wasm playwright install chromium
npm run test:package --prefix bindings/wasm
```

## Repository layout

```text
crates/core/        Rust scanning library
bindings/python/    Python extension
bindings/node/      Node.js native binding
bindings/wasm/      Browser/WASM binding
fixtures/           Shared conformance fixtures
```

## License

[MIT](LICENSE)

## German structured identifiers

Pass `{"locale":"de"}` to text or structured scans. Trimmed, ASCII
case-insensitive `de`, `de-DE`, and `de_DE` activate all seven German detectors;
omitted locale and recognized `en-US`/`fr` aliases keep base detection only.
Version 0.4.0 rejects unsupported explicit locales.

```python
from datafog_core import scan_and_transform

result = scan_and_transform("IBAN DE44 5001 0517 5407 3249 31", {
    "scan": {"locale": "de"},
    "transform": {"default": {"strategy": "redact"}, "entities": ["DE_IBAN"]},
})
assert result.text == "IBAN [DE_IBAN]"
```

These are format/context detectors, not official identifier validators. IBAN
checksums and account existence are not checked. Digits are ASCII; permitted
internal separators are space, tab, NBSP and narrow NBSP at specified group
boundaries, never newlines. Returned text and offsets preserve the source.
Passport and residence-permit patterns are legacy heuristics with limited
coverage. See the [German entity reference](docs/reference/german-entities.mdx)
and [migration differences](docs/guides/migrating-from-datafog-python.mdx).
The higher-level Python adapter and its dependency update were merged in
[PR #179](https://github.com/DataFog/datafog-python/pull/179). Its package release is separate from Core.

## UUID identifiers

Canonical UUID detection is opt-in: pass `{"detect_uuid":true}` to text or
structured scans, independently of locale. It emits `UUID` findings for versions
1–8 with the IETF variant and original casing/ranges. UUID syntax does not imply
sensitivity. See the [UUID reference](docs/reference/uuid.mdx) for boundaries,
excluded sentinel forms and transformation examples.

**0.4.1 is published** for Rust, Python, Node.js, and WASM. The higher-level Python adapter is integrated in [merged PR #179](https://github.com/DataFog/datafog-python/pull/179); its package release is separate. See the [release notes](docs/releases/0-4-1.mdx), [runtime capabilities](docs/reference/capabilities.mdx), and [0.4.x compatibility policy](docs/reference/compatibility.mdx).

**Core 0.4.1** adds three default detectors: provider-prefixed [`API_KEY`](docs/reference/api-keys.mdx), context-required [`BEARER_TOKEN`](docs/reference/bearer-token.mdx), and whole PostgreSQL [`CREDENTIAL_URI`](docs/reference/credential-uri.mdx). The registry reports 23 supported entities and 14 defaults with capability contract 1. See the [release notes](docs/releases/0-4-1.mdx) for exact scopes, preserved legacy Python overlap behavior, and the passed local exact-wheel Python 4.9 integration gate. Python's default backend is unchanged.
