# Python Binding Design

## Binding layout

```text
bindings/
  python/
    Cargo.toml
    pyproject.toml
    src/
      lib.rs
```

`bindings/python` is a member of the root Rust workspace.

- `Cargo.toml` defines the Rust binding crate and depends on `datafog-core` through a local path and on PyO3.
- `pyproject.toml` defines the `datafog-core-python` distribution and `datafog_core` import module for maturin.
- `src/lib.rs` contains the Python boundary only: the Python `Entity`, the Python `scan`, and the call into `datafog_core::scan`.

The dependency direction is `datafog-core-python` → `datafog-core`. The core has no Python or PyO3 dependency.

## PyO3 API and entity conversion

The module exports `Entity` and `scan`:

```python
from datafog_core import Entity, scan
```

`scan(text)` accepts Python `str`, calls `datafog_core::scan(text)`, and returns `list[Entity]`.

The Python `Entity` is an immutable value object with `label`, `text`, `start`, and `end` attributes. Two entities compare equal when all four fields are equal.

The wrapper maps core entities directly to Python entities:

```text
Rust label  → Python label
Rust text   → Python text
Rust start  → Python start
Rust end    → Python end
```

The conversion copies strings into Python-owned values. The wrapper preserves core ordering, offsets, duplicates, and overlap behavior; it does not sort, filter, deduplicate, or normalize results.

The binding returns only the entity list. It does not implement the existing Python package's `ScanResult` wrapper or engine metadata.

## Error mapping

PyO3 rejects non-`str` input as `TypeError`.

The wrapper catches unexpected Rust panics and raises Python `RuntimeError` rather than terminating the Python process.

## Packaging and wheel build

- Use a single macOS-arm64 ABI3 wheel built against Python 3.10. The wheel supports CPython 3.10 through 3.14.
- Maturin builds the wheel in release mode.

## Installed-package tests

```text
bindings/python/tests/test_installed.py
```

The acceptance test runs only after the wheel is installed into a clean Python environment. It loads the development and final fixtures, calls `datafog_core.scan`, converts each returned entity to the fixture schema, and compares `label`, `text`, `start`, and `end` exactly.
