# DataFog Feature Map

This directory documents the features exposed by DataFog Core, organized by capability area. Each file covers one feature set with its sub-features, user-facing entry points, verification commands, and known gotchas.

## Index

| File | Feature | Scope |
|------|---------|-------|
| [entities.md](entities.md) | PII and credential detection | Text and structured scans |
| [transforms.md](transforms.md) | Privacy transformations | redact, mask, remove, pseudonymize, tokenize |
| [sdk-surfaces.md](sdk-surfaces.md) | SDK API surfaces | Rust, Python, Node, Browser/WASM |
| [models.md](models.md) | Model inference sessions | DataFog and GLiNER2 backends |

## Verification

Run the full verification suite with `cargo dfv`:

```bash
cargo dfv doctor   # Environment check
cargo dfv build    # Build all bindings
cargo dfv check    # Rule-based detector tests
cargo dfv parity   # Cross-SDK span/offset comparison
```

Evidence is written to `target/dfv/runs/<timestamp>/`.

## Adding Features

When the codebase gains a new distinct feature, add a file here with the standard sections:

1. **Sub-features**: enumerate the variants or modes
2. **How to get to it (user POV)**: API calls, configuration, imports
3. **Driving it with cargo dfv**: which commands exercise the feature
4. **Gotchas**: known issues, edge cases, open questions
