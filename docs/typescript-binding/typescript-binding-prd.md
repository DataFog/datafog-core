# TypeScript Binding PRD

## Objective

Expose `datafog-core` through a TypeScript-facing API that can be delivered first to Node and then to WASM consumers.

## Scope

- One public TypeScript contract shared by the Node and WASM packages.
- One `scan(text)` operation covering `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`.
- Node binding first; WASM binding second.
- The bindings call `datafog-core`; they contain no detection logic.

## Public contract

```ts
type Label =
  | "EMAIL"
  | "PHONE"
  | "SSN"
  | "CREDIT_CARD"
  | "IP_ADDRESS"
  | "DATE"
  | "ZIP_CODE";

type Entity = {
  label: Label;
  text: string;
  start: number;
  end: number;
};

scan(text: string): Entity[];
```

`start` and `end` are zero-based Unicode code-point offsets, with an exclusive `end`. Results preserve the core's ordering, duplicates, and overlap behavior. An input with no matches returns an empty array.

## Packages and runtimes

- Node package: `@datafog/node`.
- WASM package: `@datafog/wasm`.
- `datafog-core` remains an internal implementation crate.
- The Node POC supports macOS arm64 only.
- The WASM POC targets modern browsers only.

## WASM initialization and errors

WASM consumers explicitly initialize the package before scanning:

```ts
await init();
const entities = scan(text);
```

`scan` remains synchronous after initialization, matching the Node API.

- Non-string input raises `TypeError` at runtime.
- Calling WASM `scan` before initialization raises `Error`.
- An unexpected core failure raises `Error`.

## Node package delivery

The Node POC builds and tests a macOS-arm64 native artifact. Cross-platform native artifacts and publishing are deferred until the binding is proven.

## Validation

- Build and install the Node package; run the development and final fixtures through `scan`.
- Build and load the WASM package; run the same fixtures through `scan`.
- For each package, compare `label`, `text`, `start`, and `end` exactly with the fixture entities and Rust core output.

## Out of scope

- Detection logic outside `datafog-core`.
- Redaction, NER, smart mode, and configuration expansion.
- A combined Node/WASM package or shared TypeScript types package for the POC.
