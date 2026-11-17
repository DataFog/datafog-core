<p align="center">
  <a href="https://datafog.ai"><img src="docs/favicon.svg" width="72" height="72" alt="datafog"></a>
</p>

<h1 align="center">datafog core</h1>

<p align="center"><strong>Find and transform sensitive data in your application.</strong></p>

<p align="center">
  <a href="https://pypi.org/project/datafog-core/"><img src="https://img.shields.io/pypi/v/datafog-core?style=flat-square&amp;color=C0362C" alt="PyPI version"></a>
  <a href="https://www.npmjs.com/package/@datafog/node"><img src="https://img.shields.io/npm/v/%40datafog%2Fnode?style=flat-square&amp;color=C0362C" alt="npm version"></a>
  <a href="https://crates.io/crates/datafog-core"><img src="https://img.shields.io/crates/v/datafog-core?style=flat-square&amp;color=C0362C" alt="crates.io version"></a>
  <a href="https://github.com/DataFog/datafog-core/actions/workflows/rust-quality.yml"><img src="https://github.com/DataFog/datafog-core/actions/workflows/rust-quality.yml/badge.svg?branch=main" alt="Rust CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-C0362C?style=flat-square" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#quickstart">Quickstart</a> ·
  <a href="#detection-coverage">Detection</a> ·
  <a href="#transformations">Transformations</a> ·
  <a href="#local-models-beta">Local models</a> ·
  <a href="#documentation-and-development">Documentation</a>
</p>

datafog core is a Rust library with Python, Node.js, and browser WebAssembly bindings. Scan text or JSON for personal data and credentials, then redact, mask, remove, pseudonymize, or tokenize matched values. Findings use a consistent format across the bindings.

Rule-based scanning runs locally without model weights or downloads. Optional local model sessions add datafog and GLiNER2 support in Rust, Python, and Node.js.

**Release:** coordinated [Core 1.6.0](https://github.com/DataFog/datafog-core/releases/tag/v1.6.0) for Python, Rust, Node.js, WASM and the Core-owned companion packages · [1.6 release notes](docs/releases/1-6-0.mdx) · [Python release history](https://pypi.org/project/datafog-core/#history) · [1.5 migration guide](docs/guides/migrating-to-core-1-5.mdx) · [Earlier cross-SDK 1.1.0 notes](docs/releases/1-1-0.mdx) · [Extension installation and testing](docs/releases/1-1-0-extension-testing.md).

Core 1.0 freezes Workspace contract 1 across Rust, Python, Node and browser WASM, with optional native/browser [FogVault integrations](docs/integrations/fogvault.md). Snapshot v3 writes and v1/v2/v3 reads keep their existing semantics. Exact published [0.5.0-beta.3](docs/releases/0-5-0-beta-3.mdx) interchange remains tested; FogVault is independently versioned 0.1.0.

## Quickstart

The coordinated Core release is 1.6.0 across PyPI, crates.io and npm. Applications should lock the exact version they have qualified. The `datafog` Python application and FogVault version independently of Core.

| SDK | Published install | Requirements |
| --- | --- | --- |
| [Python](https://pypi.org/project/datafog-core/) | `python -m pip install datafog-core==1.6.0` | Python 3.10+ |
| [Node.js](https://www.npmjs.com/package/@datafog/node) | `npm install --save-exact @datafog/node@1.6.0` | Node.js 24+ |
| [Rust](https://crates.io/crates/datafog-core) | `cargo add datafog-core@=1.6.0` | Rust 1.88+ |
| [Browser / WASM](https://www.npmjs.com/package/@datafog/wasm) | `npm install --save-exact @datafog/wasm@1.6.0` | Browser with WebAssembly support |

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

The Python package ships CPython 3.10+ ABI3 wheels for Linux x64 and ARM64
(`manylinux_2_28` and `musllinux_1_2`), Linux s390x and ppc64le
(`manylinux_2_28`), macOS Intel 10.12+ and Apple Silicon 11+, and Windows
x64/ARM64. The Node.js 1.6.0 package ships Linux x64/ARM64 GNU, macOS Intel/Apple
Silicon, and Windows x64 binaries.

PyPy, free-threaded CPython, older glibc/musl and other architectures fall back
to a source build. That route requires Rust 1.88+, Cargo, a native linker and
access to the Rust build dependencies; installation can take several minutes.
To require a prebuilt wheel, use `python -m pip install --only-binary=:all:
datafog-core`. See [installation requirements](docs/get-started/installation.mdx).

## Detection coverage

| Coverage | Entity types | Activation |
| --- | --- | --- |
| Personal and financial data | `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, `ZIP_CODE` | On by default |
| Credentials | [`API_KEY`](docs/reference/api-keys.mdx), [`BEARER_TOKEN`](docs/reference/bearer-token.mdx), [`JWT`](docs/reference/jwt.mdx), [`PRIVATE_KEY`](docs/reference/private-keys.mdx), [`CREDENTIAL_URI`](docs/reference/credential-uri.mdx) | On by default |
| Labeled street addresses | [`STREET_ADDRESS`](docs/reference/street-address.mdx) | On by default; bounded English street-line rule |
| Labeled accounts | [`ACCOUNT_NUMBER`](docs/reference/account-number.mdx) | On by default; bounded lexical rule |
| Labeled US identifiers | [`US_ROUTING_NUMBER`](docs/reference/us-routing-number.mdx), [`NPI`](docs/reference/npi.mdx) | On by default; requires a supported text label |
| German identifiers | IBAN, VAT ID, tax ID, social security number, postal code, passport number, residence permit number | Explicit [German locale](docs/reference/german-entities.mdx) |
| UUIDs | [`UUID`](docs/reference/uuid.mdx) | Explicit `detect_uuid` setting |
| Names in JSON fields | `PERSON` | [Structured scans](docs/guides/person-discovery.mdx), using name-field aliases or explicit JSON Pointer mappings |
| Names and address blocks in prose | `PERSON`, `ADDRESS` | `EmbeddedCnnDetector`: weights compiled into the extension, no download or runtime |
| Caller-defined labels | any | GLiNER2 [model sessions](#local-models-beta) |

Use [structured scans](docs/guides/person-discovery.mdx) to find names by JSON field, `EmbeddedCnnDetector` for recipient names and mailing-address blocks in prose, or [model sessions](#local-models-beta) for caller-defined labels.

Each finding includes its entity type, matched text, byte and code-point ranges, optional confidence, and detector name/version. Ranges are zero-based and end-exclusive. Node.js and WASM also provide UTF-16 ranges for JavaScript `slice`. Rule-based findings have no confidence score. [Findings and ranges →](docs/concepts/findings-and-ranges.mdx)

For scanning files from AI agents, use [datafog MCP](https://github.com/DataFog/datafog-mcp).

<details>
<summary><strong>Detection settings</strong></summary>

- **Select checks:** `scan(text, {"entities": ["EMAIL", "SSN"]})` in Python runs only the selected text detectors. Omit `entities` to use all enabled detectors. Selections must be nonempty, unique, exact entity names. Selecting a German type or UUID does not activate it; set `locale` or `detect_uuid` too.
- **Literal denylists:** `{"deny": [{"phrase": "Project Atlas"}]}` adds explicit sensitive phrases. Rules default to case-sensitive whole-phrase word boundaries; substring and Unicode case-insensitive options are available. Explicit deny matches win allowlists. [Matching and policy rules →](docs/reference/literal-denylists.mdx)
- **German locale:** `{"locale": "de"}` enables the seven German detectors. These use format/context rules; IBAN checksums and account existence are not verified. See the [German reference](docs/reference/german-entities.mdx) for detector rules.
- **ENV and SQL:** choose `format: "env"` or `format: "sql"` to handle email boundaries in assignments and quoted strings. [Source-context rules →](docs/guides/email-boundaries.mdx)
- **Phone numbers:** numbers without `+` use North American checks: ten digits, optionally preceded by `1`, with area codes starting with `2`–`9` and exchange codes starting with `1`–`9`. Numbers starting with `+` use a permissive 7–15 digit check, including shortened examples such as `+1-555-1002`. Country codes and assigned numbers are not validated; numeric identifiers can still match.
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

## Browser workspace and FogVault integration

The unreleased v1.1 browser extension flow adds a bundled, default-enabled
name/address CNN with a disable control, plus SQLite3 Multiple Ciphers as a
second encrypted Workspace backend. See the [candidate integration guide](docs/browser-workspace-v1-1.md).

Core 1.0 provides persistent `Workspace` pseudonyms, explicit approved saves,
strict/progressive restoration, operation history and conversation-scoped recall.
The optional browser adapter uses the maintained FogVault Worker/WASM owner and
IndexedDB storage. Native Rust, Node and Python owners use the same portable
vault core through the registered native adapter.

Install the browser integration:

```bash
npm install --save-exact @datafog/wasm@1.6.0 @datafog/fogvault-browser@0.1.0 @datafog/core-fogvault-browser@1.6.0
```

[Integration guide and supported profiles →](docs/integrations/fogvault.md)
· [Stable Workspace contract →](docs/contracts/core-v1.md)
· [Model assets and runtime setup →](docs/browser-model.md)
· [SDK release scope and separate extension acceptance →](docs/core-v1-release-scope.md)

## Local models (beta)

Core 1.0 includes optional local sessions for the datafog PII model and GLiNER2 in Rust, Python and Node.js. Model files are provisioned separately; model quality measurements remain separate from SDK correctness.

Install the published SDK version for your registry:

```bash
python -m pip install 'datafog-core==1.6.0'
npm install --save-exact @datafog/node@1.6.0
cargo add datafog-core@=1.6.0 --features models
```

For GLiNER2, install its optional dependencies in the Python environment used by the runner:

```bash
python -m pip install 'datafog-core[gliner2]==1.6.0'
```

### Embedded name and address detection

`EmbeddedCnnDetector` carries the pinned `statements-v1` CNN in the extension itself: no model download, no files, no subprocess and no inference runtime. It is the same detector the browser build embeds and returns the same `ModelScanResult` contract as `ModelSession`.

```python
from datafog_core import EmbeddedCnnDetector

detector = EmbeddedCnnDetector()          # authenticates the embedded weights once
result = detector.scan(text, {"entities": ["PERSON", "ADDRESS", "EMAIL"]})
```

Limits are 65,536 input bytes and 8,192 tokens per request and 1,024 findings. Inference is synchronous and in-process, so `ModelCancellation` cannot interrupt it; keep a detector per worker thread. `scan_model` returns model-only findings, and `EmbeddedCnnDetector.scan_disabled` validates the same selection with no inference. [Embedded CNN reference →](docs/reference/embedded-cnn.mdx)

GLiNER2 supports the PII checkpoint and compatible general-purpose span models with caller-defined labels. The GLiNER2 runner ships in the Python package and also serves Node.js and Rust callers. Download and verify model files separately, then use `ModelSession.open`, text `scan`, and `close`. Reuse sessions across calls. Both backends return consistent results and errors, with cancellation and cleanup support.

[datafog setup and session API →](docs/model-sessions.md) · [GLiNER2 setup →](docs/gliner2-model.md)

## Documentation and development

- [Roadmap](ROADMAP.md)
- [Dev environment setup](CONTRIBUTING.md#dev-environment-setup) — devcontainer or local install, `cargo dfv doctor`
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

[MIT](LICENSE). Built by [datafog](https://datafog.ai).
