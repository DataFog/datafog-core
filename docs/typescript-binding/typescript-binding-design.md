# TypeScript Binding Design

## Node binding layout

```text
bindings/
  node/
    Cargo.toml
    package.json
    src/
      lib.rs
    index.js
    index.d.ts
```

- `Cargo.toml` defines the `datafog-node` Rust binding crate. It depends on `datafog-core` and `napi-rs`.
- `src/lib.rs` is the Rust-to-Node boundary.
- `package.json` defines the `@datafog/node` npm package.
- `index.js` loads the compiled native addon.
- `index.d.ts` defines the TypeScript public surface.

The dependency direction is `@datafog/node` → `datafog-node` → `datafog-core`. The core has no Node dependency.

Use `napi-rs` for the native addon. The POC publishes a macOS-arm64 artifact only.

The Node package is ESM-only for the POC:

```ts
import { scan } from "@datafog/node";
```

CommonJS support is deferred. A CommonJS consumer can use dynamic `import()` if needed.

## Node API and entity conversion

```ts
export type Label =
  | "EMAIL"
  | "PHONE"
  | "SSN"
  | "CREDIT_CARD"
  | "IP_ADDRESS"
  | "DATE"
  | "ZIP_CODE";

export type Entity = {
  readonly label: Label;
  readonly text: string;
  readonly start: number;
  readonly end: number;
};

export function scan(text: string): Entity[];
```

`scan` is synchronous. Each core entity maps directly to a plain JavaScript object: strings become JavaScript strings and offsets become JavaScript numbers. The binding preserves core ordering, duplicates, overlaps, and Unicode code-point offsets without re-detection, sorting, or normalization.

`readonly` is a TypeScript-only constraint; the binding does not freeze output objects at runtime.

`napi-rs` rejects non-string input with `TypeError`. The binding checks Rust offsets before converting them to JavaScript-compatible numeric offsets. Unexpected Rust failures become JavaScript `Error` values rather than terminating Node.

## Node package build and delivery

The Node build produces a release-mode macOS-arm64 native addon (`.node`). The npm package contains the compiled addon, its ESM loader, generated TypeScript declarations, and `package.json`.

Use `napi-rs` generated TypeScript declarations rather than maintaining declarations by hand.

The POC supports and tests Node 24 on macOS arm64. It does not publish to npm. Delivery validation uses `npm pack`, followed by installation of the generated tarball into a clean Node project.

The macOS-arm64 package must fail clearly on unsupported OS or CPU combinations. Cross-platform artifacts and npm publication are deferred.

## Node installed-package test

The installed-package test uses no JavaScript test framework:

1. Build the addon and create an npm tarball with `npm pack`.
2. Create a temporary clean Node project and install the tarball.
3. Run a checked-in ESM test script from that project.
4. Load the development and final fixtures; for every record, compare `scan(record.text)` exactly with `record.entities`.
5. Verify that non-string input raises `TypeError`.
6. Run a small `tsc --noEmit` smoke test that imports `scan` and `Entity`.

The runtime test imports only `@datafog/node`, never the native addon or Rust crate directly. Node 24 GitHub Actions runs this installed-package test on macOS arm64.

## WASM binding layout

```text
bindings/
  wasm/
    Cargo.toml
    package.json
    src/
      lib.rs
    index.js
    index.d.ts
    dist/                 # generated; not committed
```

- `Cargo.toml` defines the `datafog-wasm` binding crate. It depends on `datafog-core` and `wasm-bindgen`.
- `src/lib.rs` is the Rust-to-WASM boundary, compiled for `wasm32`.
- `package.json` defines the `@datafog/wasm` npm package.
- `index.js` is the browser-facing ESM wrapper.
- `index.d.ts` declares the shared `Label`, `Entity`, `init`, and `scan` surface.
- `dist/` contains generated JavaScript glue and the compiled `.wasm` binary.

The dependency direction is `@datafog/wasm` → `datafog-wasm` → `datafog-core`. The core has no WASM or browser dependency.

Use `wasm-bindgen` directly rather than `wasm-pack`, so the package has an explicit `@datafog/wasm` wrapper and metadata. The generated artifact directory is build output, not source. The POC maintains the small WASM declaration file by hand rather than creating a shared types package.

## WASM API and initialization

```ts
import { init, scan } from "@datafog/wasm";

await init();
const entities = scan("Email jane@example.com");
```

`init()` takes no required arguments and resolves the package's own `.wasm` asset relative to its ESM module. It is asynchronous, idempotent, and shared by concurrent callers. Once initialization succeeds, `scan` is synchronous.

Calling `scan` before `await init()` raises `Error`. Non-string input raises `TypeError`. Initialization failure rejects the `init()` promise; unexpected binding failures become JavaScript `Error` values.

`scan` returns the same plain JavaScript entity objects as Node. The WASM Rust boundary serializes core entities directly to JavaScript values. The wrapper only manages initialization and the public API; it does not re-detect, sort, normalize, or change offsets.

## WASM package build and delivery

The build pipeline is:

```text
Rust release build for wasm32-unknown-unknown
  → wasm-bindgen web output
  → ESM wrapper and declarations
  → npm tarball
```

Compile only for `wasm32-unknown-unknown`; WASI and Node WASM targets are out of scope. Use `wasm-bindgen --target web` to generate browser-oriented ESM glue and the `.wasm` binary. Build Rust in release mode; separate WASM-size optimization is deferred.

The generated `dist/` directory remains ignored by Git but is included in the npm tarball through the package `files` list. `@datafog/wasm` is ESM-only and intended for browser bundlers or import-map consumers, not standalone script-tag use. Its wrapper resolves the binary with `new URL(..., import.meta.url)`.

Use `npm pack` and a clean installation to validate delivery. npm publication is out of scope. Pin the `wasm-bindgen` CLI to the Rust crate's version so generated glue remains compatible.

## WASM browser test

The WASM test uses a real browser and a local HTTP server; `file://` loading is not sufficient for browser modules and WASM assets.

1. Build the package and create its npm tarball.
2. Create a temporary clean browser-consumer project, install the tarball, and copy both fixtures into it.
3. Serve the project over local HTTP.
4. Use Playwright with Chromium to open the consumer page.
5. Verify `scan` before initialization raises `Error`; call `await Promise.all([init(), init()])`; then verify non-string input raises `TypeError`.
6. Run both fixtures through `scan` and compare every entity array exactly with the fixture entities.
7. Run a small `tsc --noEmit` import/type smoke test for the handwritten declarations.

GitHub Actions runs this test with Node 24 and Chromium on Linux. The WASM package is not tied to the macOS-arm64 target used by the native Node addon.

## Shared conformance

The checked-in JSONL fixtures are the shared expected behavior. Each installed package runs both fixtures and compares the entire returned entity array exactly: `label`, `text`, `start`, `end`, and array order. This covers no-match, multi-match, Unicode-offset, duplicate, and overlap behavior represented in the fixtures.

Both bindings test the shared `TypeError` behavior for non-string input; WASM also tests scan-before-initialization. Binding tests import only their installed public packages, never Rust source directly.

The binding jobs rerun when the core, fixtures, or relevant binding files change. They do not run another Python-baseline comparison: the fixtures are the agreed output and the core comparison already establishes the Python baseline relationship.

Binding performance is outside conformance. It may be measured later as runtime-specific binding overhead, not as a direct Python-versus-Rust comparison.
