# DataFog Core

Fast structured PII detection, implemented in Rust and exposed for Rust, Python, Node.js, and browsers.

It detects `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`. Every binding returns the same finding information:

```text
entity type, matched text, byte range, code-point range,
optional confidence, detector name, optional detector version
```

Both ranges use zero-based, end-exclusive offsets. The byte range addresses the
UTF-8 input; the code-point range addresses Unicode scalar values. Rule-based
detectors currently report no confidence score.

## Packages

| Runtime | Distribution | Import | Status |
| --- | --- | --- | --- |
| Rust | [`datafog-core`](https://crates.io/crates/datafog-core) | `datafog_core` | Published |
| Python | [`datafog-core`](https://pypi.org/project/datafog-core/) | `datafog_core` | Published |
| Node.js | `@datafog/node` | `@datafog/node` | npm release pending |
| Browser/WASM | `@datafog/wasm` | `@datafog/wasm` | npm release pending |

## Quick start

### Rust

```bash
cargo add datafog-core
```

```rust
use datafog_core::scan;

let findings = scan("Email jane@example.com");
assert_eq!(findings[0].entity_type, "EMAIL");
assert_eq!(findings[0].matched_text, "jane@example.com");
assert_eq!(findings[0].byte_range.start, 6);
```

### Python

```bash
python -m pip install datafog-core
```

```python
from datafog_core import scan

findings = scan("Email jane@example.com")
print(findings[0].entity_type)       # EMAIL
print(findings[0].matched_text)      # jane@example.com
print(findings[0].byte_range.start)  # 6
```

### Node.js

`@datafog/node` will install as a native package once its npm release is published.

```js
import { scan } from "@datafog/node";

console.log(scan("Email jane@example.com"));
```

The release includes prebuilt binaries for macOS (Intel and Apple Silicon), Linux (x64 and ARM64), and Windows x64.

### Browser / WASM

`@datafog/wasm` will install from npm once its first release is published.

```js
import { init, scan } from "@datafog/wasm";

await init();
console.log(scan("Email jane@example.com"));
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
