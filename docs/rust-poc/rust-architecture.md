# Rust `scan` Architecture

Bindings call one Rust core. Label-specific detectors produce byte-range candidates; the shared framework converts them into the public entity contract.

```text
                              +-----------------------------+
                              |       LABEL DETECTORS       |
                              |                             |
                              |  +-----------------------+  |
                              |  | Pattern detectors     |  |
                              |  | EMAIL · PHONE         |  |
                              |  +-----------------------+  |
                              |                             |
                              |  +-----------------------+  |
                              |  | Structured detectors  |  |
                              |  | SSN · CARD · DATE ·   |  |
                              |  | ZIP                   |  |
                              |  +-----------------------+  |
                              |                             |
                              |  +-----------------------+  |
                              |  | IP parser             |  |
                              |  | IPv4 · IPv6           |  |
                              |  +-----------------------+  |
                              +--------------+--------------+
                                             |
                                             v
+-------------------+      +-------------------+      +------------------------+
| Bindings          | ---> | scan(text)        | ---> | Shared framework       |
| Python · TS · WASM|      | Rust core entry   |      | offsets · sort · dedup |
+-------------------+      +-------------------+      +-----------+------------+
                                                                    |
                                                                    v
                                                         +--------------------+
                                                         | Entities           |
                                                         | Vec<Entity>        |
                                                         +--------------------+
```

```text
INTERNAL: Detectors return UTF-8 byte ranges.
PUBLIC:   Entities expose Unicode code-point offsets.
```

## Planned Repository Layout

```text
README.md
docs/
  rust-poc/
    rust-trd.md
    rust-design.md
    python-baseline.md
    rust-architecture.html
    rust-architecture.md
    adr/
  python-binding/       # future binding docs
fixtures/
  development.jsonl
  final.jsonl
crates/
  core/                 # Rust scan core
bindings/
  python/               # future
  typescript/           # future
  wasm/                 # future
```

## Shared Framework Responsibilities

Label detectors own candidate discovery and label-specific validation. The shared framework owns byte-range collection, conversion to public Unicode code-point offsets, deterministic sorting, exact deduplication, and preservation of non-identical overlaps. Matchers and lookup data are initialized once; scanning itself does not copy input text or perform I/O.

## Performance Model

- Structured labels use direct byte-based parsers; IP candidates use Rust's standard IP parser.
- Fixed labels use an internal enum; regex matchers and lookup data are reused.
- ASCII input returns byte offsets directly; non-ASCII input is converted to Unicode code-point offsets.
- One `scan` call is single-threaded. Trigger-driven scanning and batch parallelism are deferred until benchmarks justify them.
