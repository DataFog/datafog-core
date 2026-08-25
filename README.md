# DataFog Core

Fast structured PII detection implemented in Rust, with Python, Node.js, and browser/WASM bindings.

## Supported entities

`EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`.

All bindings use the same Rust implementation and return the matched text with zero-based Unicode code-point offsets.

## Repository layout

```text
crates/core/        Rust scanning library
bindings/python/    Python extension: datafog-core / datafog_core
bindings/node/      Node package: @datafog/node
bindings/wasm/      Browser package: @datafog/wasm
fixtures/           Shared conformance fixtures
```

## Development

```bash
cargo test --workspace
```

### Python

```bash
python3 -m pip install maturin
maturin build --manifest-path bindings/python/Cargo.toml --release
python3 -m venv .venv
.venv/bin/python -m pip install target/wheels/datafog_core_python-*.whl
.venv/bin/python bindings/python/tests/test_installed.py
```

### Node.js

```bash
npm ci --prefix bindings/node
npm run test:package --prefix bindings/node
```

### Browser/WASM

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
npm ci --prefix bindings/wasm
npx --prefix bindings/wasm playwright install chromium
npm run test:package --prefix bindings/wasm
```

```js
import { init, scan } from "@datafog/wasm";

await init();
console.log(scan("Email jane@example.com"));
```

## Package status

The repository builds the following packages locally:

- `datafog-core` for Rust consumers
- `datafog-core` for Python consumers, imported as `datafog_core`
- `@datafog/node` for Node.js consumers
- `@datafog/wasm` for browser and bundler consumers

Publishing and cross-platform release automation are the next production steps.

## License

MIT
