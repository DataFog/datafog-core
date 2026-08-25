# DataFog Core

Fast structured PII detection, implemented in Rust and exposed for Rust, Python, Node.js, and browsers.

It detects `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`. Every binding returns the same entity shape:

```text
label, text, start, end
```

`start` and `end` are zero-based Unicode code-point offsets; `end` is exclusive.

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

let entities = scan("Email jane@example.com");
assert_eq!(entities[0].label, "EMAIL");
assert_eq!(entities[0].text, "jane@example.com");
```

### Python

```bash
python -m pip install datafog-core
```

```python
from datafog_core import scan

entities = scan("Email jane@example.com")
print(entities[0].label)  # EMAIL
print(entities[0].text)   # jane@example.com
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
