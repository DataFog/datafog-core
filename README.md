<p align="center">
  <a href="https://datafog.ai"><img src="docs/favicon.svg" width="72" height="72" alt="DataFog"></a>
</p>

<h1 align="center">DataFog Core</h1>

<p align="center"><strong>Find and transform sensitive data in your application.</strong></p>

<p align="center">
  <a href="https://pypi.org/project/datafog-core/"><img src="https://img.shields.io/pypi/v/datafog-core?style=flat-square&amp;color=E8A84C" alt="PyPI version"></a>
  <a href="https://www.npmjs.com/package/@datafog/node"><img src="https://img.shields.io/npm/v/%40datafog%2Fnode?style=flat-square&amp;color=E8A84C" alt="npm version"></a>
  <a href="https://crates.io/crates/datafog-core"><img src="https://img.shields.io/crates/v/datafog-core?style=flat-square&amp;color=E8A84C" alt="crates.io version"></a>
  <a href="https://github.com/DataFog/datafog-core/actions/workflows/rust-quality.yml"><img src="https://github.com/DataFog/datafog-core/actions/workflows/rust-quality.yml/badge.svg?branch=main" alt="Rust CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-E8A84C?style=flat-square" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#quickstart">Quickstart</a> ·
  <a href="#detection-coverage">Detection</a> ·
  <a href="#transformations">Transformations</a> ·
  <a href="#local-models-beta">Local models</a> ·
  <a href="#documentation-and-development">Documentation</a>
</p>

DataFog Core is a Rust library with Python, Node.js, and browser WebAssembly bindings. Scan text or JSON for personal data and credentials, then redact, mask, remove, pseudonymize, or tokenize matched values. Findings use a consistent format across the bindings.

Rule-based scanning runs locally without model weights or downloads. Optional local model sessions add DataFog and GLiNER2 support in Rust, Python, and Node.js.

**Stable:** [0.4.4](https://github.com/DataFog/datafog-core/releases/tag/v0.4.4) · **Model beta:** [0.4.5-beta.4](https://github.com/DataFog/datafog-core/releases/tag/v0.4.5-beta.4)

## Quickstart

Choose the package for your application:

| SDK | Install stable | Requirements |
| --- | --- | --- |
| [Python](https://pypi.org/project/datafog-core/) | `python -m pip install datafog-core` | Python 3.10+ |
| [Node.js](https://www.npmjs.com/package/@datafog/node) | `npm install @datafog/node` | Node.js 24.x |
| [Rust](https://crates.io/crates/datafog-core) | `cargo add datafog-core` | Rust 1.88+ |
| [Browser / WASM](https://www.npmjs.com/package/@datafog/wasm) | `npm install @datafog/wasm` | Browser with WebAssembly support |

### Python

```python
from datafog_core import scan, scan_and_transform

findings = scan("Email jane@example.com")
print(findings[0].entity_type)   # EMAIL
print(findings[0].matched_text)  # jane@example.com

result = scan_and_transform(
    "Email jane@example.com",
    {"transform": {"default": {"strategy": "redact"}}},
)
print(result.text)  # Email [EMAIL]
```

<details>
<summary><strong>Node.js example</strong></summary>

```js
import { scan, scanAndTransform } from "@datafog/node";

const findings = scan("Email jane@example.com");
console.log(findings[0].entityType); // EMAIL
console.log(findings[0].matchedText); // jane@example.com

const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } },
});
console.log(result.text); // Email [EMAIL]
```

</details>

<details>
<summary><strong>Rust example</strong></summary>

```rust
use datafog_core::{
    scan, scan_and_transform, PrivacyError, ScanAndTransformConfig,
    TransformationConfig, TransformationStrategy,
};

fn main() -> Result<(), PrivacyError> {
    let findings = scan("Email jane@example.com");
    println!("{}", findings[0].entity_type); // EMAIL
    println!("{}", findings[0].matched_text); // jane@example.com

    let result = scan_and_transform(
        "Email jane@example.com",
        &ScanAndTransformConfig::new(TransformationConfig::new(
            TransformationStrategy::Redact,
        )),
    )?;
    println!("{}", result.text); // Email [EMAIL]
    Ok(())
}
```

</details>

<details>
<summary><strong>Browser / WASM example</strong></summary>

```js
import { init, scan, scanAndTransform } from "@datafog/wasm";

await init();
console.log(scan("Email jane@example.com"));

const result = scanAndTransform("Email jane@example.com", {
  transform: { default: { strategy: "redact" } },
});
console.log(result.text); // Email [EMAIL]
```

</details>

Python and Node.js packages include prebuilt native binaries for Linux x64 and ARM64 (GNU), macOS Intel and Apple Silicon, and Windows x64 (MSVC).

## Detection coverage

| Coverage | Entity types | Activation |
| --- | --- | --- |
| Personal and financial data | `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, `ZIP_CODE` | On by default |
| Credentials | [`API_KEY`](docs/reference/api-keys.mdx), [`BEARER_TOKEN`](docs/reference/bearer-token.mdx), [`JWT`](docs/reference/jwt.mdx), [`PRIVATE_KEY`](docs/reference/private-keys.mdx), [`CREDENTIAL_URI`](docs/reference/credential-uri.mdx) | On by default |
| Labeled US identifiers | [`US_ROUTING_NUMBER`](docs/reference/us-routing-number.mdx), [`NPI`](docs/reference/npi.mdx) | On by default; requires a supported text label |
| German identifiers | IBAN, VAT ID, tax ID, social security number, postal code, passport number, residence permit number | Explicit [German locale](docs/reference/german-entities.mdx) |
| UUIDs | [`UUID`](docs/reference/uuid.mdx) | Explicit `detect_uuid` setting |
| Names in JSON fields | `PERSON` | [Structured scans](docs/guides/person-discovery.mdx), using name-field aliases or explicit JSON Pointer mappings |

Use [structured scans](docs/guides/person-discovery.mdx) to find names by JSON field, or [local models](#local-models-beta) to detect names and street addresses in prose.

Each finding includes its entity type, matched text, byte and code-point ranges, optional confidence, and detector name/version. Ranges are zero-based and end-exclusive. Node.js and WASM also provide UTF-16 ranges for JavaScript `slice`. Rule-based findings have no confidence score. [Findings and ranges →](docs/concepts/findings-and-ranges.mdx)

For scanning files from AI agents, use [DataFog MCP](https://github.com/DataFog/datafog-mcp).

<details>
<summary><strong>Detection settings</strong></summary>

- **Select checks:** `scan(text, {"entities": ["EMAIL", "SSN"]})` in Python runs only the selected text detectors. Omit `entities` to use all enabled detectors. Selections must be nonempty, unique, exact entity names. Selecting a German type or UUID does not activate it; set `locale` or `detect_uuid` too.
- **German locale:** `{"locale": "de"}` enables the seven German detectors. These use format/context rules; IBAN checksums and account existence are not verified. See the [German reference](docs/reference/german-entities.mdx) for detector rules.
- **ENV and SQL:** choose `format: "env"` or `format: "sql"` to handle email boundaries in assignments and quoted strings. [Source-context rules →](docs/guides/email-boundaries.mdx)
- **Phone numbers:** numbers without `+` use North American checks: ten digits, optionally preceded by `1`, with area and exchange codes starting with `2`–`9`. Numbers starting with `+` use a permissive 7–15 digit check, including shortened examples such as `+1-555-1002`. Country codes and assigned numbers are not validated; numeric identifiers can still match.
- **Decimal numbers and CSV:** contiguous card digits after a digit and `.` or `,` are suppressed, so `3.4111111111111111` and `3,4111111111111111` do not produce card findings. Cards containing spaces or hyphens still undergo normal checks. Parse CSV/TSV into cells before scanning so delimiters and adjacent fields do not affect matching.

Use [`capabilities()`](docs/reference/capabilities.mdx) in Rust or Python to discover supported entities and their activation rules.

</details>

## Transformations

| Strategy | Result | Application provider | Browser / WASM |
| --- | --- | --- | --- |
| `redact` | Replace with a label such as `[EMAIL]` | None | Supported |
| `mask` | Replace characters, optionally revealing the first or last characters | None | Supported |
| `remove` | Delete the matched span | None | Supported |
| [`pseudonymize`](docs/guides/pseudonymization.mdx) | Create a stable keyed HMAC-SHA-256 pseudonym | Key provider | Unsupported |
| [`tokenize`](docs/guides/tokenization-and-restoration.mdx) | Issue an opaque token that can be restored under the same scope | Token provider | Unsupported |

Use `transform` with existing findings, or `scan_and_transform` / `scanAndTransform` to scan and transform together. Transformation records include source/output ranges and replacement metadata, without the original matched text.

Keep scan settings under `scan` and transformation policy under `transform`. A default strategy is required; entity selection, per-type overrides, and allowlists are optional. [Configuration guide →](docs/guides/configuration.mdx) · [Strategy details →](docs/concepts/privacy-transformations.mdx)

## Local models (beta)

[0.4.5-beta.4](https://github.com/DataFog/datafog-core/releases/tag/v0.4.5-beta.4) adds local sessions for the DataFog PII model and GLiNER2 in Rust, Python, and Node.js on all five native platforms.

Install the beta for your runtime:

```bash
python -m pip install 'datafog-core==0.4.5b4'
npm install @datafog/node@beta
cargo add datafog-core@0.4.5-beta.4 --features models
```

For GLiNER2, install its optional dependencies in the Python environment used by the runner:

```bash
python -m pip install 'datafog-core[gliner2]==0.4.5b4'
```

GLiNER2 supports the PII checkpoint and compatible general-purpose span models with caller-defined labels. The GLiNER2 runner ships in the Python package and also serves Node.js and Rust callers. Download and verify model files separately, then use `ModelSession.open`, text `scan`, and `close`. Reuse sessions across calls. Both backends return consistent results and errors, with cancellation and cleanup support.

[DataFog setup and session API →](docs/model-sessions.md) · [GLiNER2 setup →](docs/gliner2-model.md)

## Documentation and development

- [Roadmap](ROADMAP.md)
- [Installation and runtime requirements](docs/get-started/installation.mdx)
- [Python](docs/reference/python.mdx) · [Node.js](docs/reference/node.mdx) · [Rust](docs/reference/rust.mdx) · [Browser / WASM](docs/reference/browser-wasm.mdx) API references
- [Structured JSON scans and name-field discovery](docs/guides/person-discovery.mdx)
- [Configuration and allowlists](docs/guides/configuration.mdx)
- [Errors](docs/reference/errors.mdx) and [0.4.x compatibility](docs/reference/compatibility.mdx)
- [Build, test, and contribute](docs/development.mdx)
- [Release notes](https://github.com/DataFog/datafog-core/releases)

For existing `datafog` Python applications, follow the [migration guide](docs/guides/migrating-from-datafog-python.mdx) for changes to imports, results, and transformations.

For Rust development, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

See the [development guide](docs/development.mdx) for the repository layout and Python, Node.js, and WASM package checks.

## License

[MIT](LICENSE). Built by [DataFog](https://datafog.ai).
