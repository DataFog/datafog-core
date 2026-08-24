# Rust `scan` Contract

## Function

```rust
scan(text: &str) -> Vec<Entity>
```

## Entity

```rust
Entity {
  label: String,
  text: String,
  start: usize,
  end: usize,
}
```

## Labels

`EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, `ZIP_CODE`.

## Behavior

- Scan `text` for the seven supported PII types.
- Return one entity for each detected span.
- `text` equals the exact matched substring.
- `start` is a zero-based Unicode code-point offset.
- `end` is exclusive.
- Return entities in ascending `start` order.
- Return no duplicate spans.

## Implementation Approach

Deliberately open as long as the chosen approach:

- supports the seven scoped labels;
- returns contract-compliant entities;
- avoids NER and model dependencies;
- be evaluated against the fixed fixtures and Python baseline performance.

## Out of Scope

NER, engine selection, locales, allowlists, redaction, and all PII types outside the seven labels above.
