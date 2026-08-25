# Python Binding TRD

## Objective

Expose the Rust `scan` core as an installable Python package.

## Scope

- Python only.
- One `scan(text)` API.
- The same seven labels: `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`.
- The binding calls the Rust core; it does not contain detection logic.

## Public API

- Distribution name: `datafog-core-python`.
- Import name: `datafog_core`.
- `scan(text)` returns `list[Entity]`.
- `Entity` is an immutable value type with `label`, `text`, `start`, and `end` attributes.
- Preserve core ordering and return an empty collection when nothing is found.

The binding is intentionally not a drop-in replacement for the existing Python `ScanResult` API. A future `datafog` package integration can wrap this binding if compatibility is required.

## Offset contract

Return the core's zero-based Unicode code-point offsets unchanged.

## Error behavior

- Non-string input raises `TypeError`.
- The first binding accepts Python `str` only; `bytes` input is out of scope.
- Unexpected Rust failures raise `RuntimeError`.

## Packaging and installation

- Use PyO3 and maturin.
- Support CPython 3.10 through 3.14 on macOS arm64 for the POC.
- Produce an installable wheel.

## Acceptance criteria

- Build a wheel, create a clean virtual environment, and install the wheel with `pip`.
- Import `datafog_core` from the installed package.
- Run the 100- and 1,000-sentence fixtures through `datafog_core.scan`.
- Produce entities identical to the Rust core for `label`, `text`, `start`, and `end`.

## Out of scope

- Redaction, NER, smart mode, and Python-side fallback detection.
- Configuration expansion.
- Node and WASM bindings.
