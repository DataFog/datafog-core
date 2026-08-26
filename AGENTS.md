# DataFog Core repository rules

## Correctness

- Detection changes require positive and negative tests that prove the changed behavior.
- Do not silently discard fallible results.
- Avoid panic paths for user-controlled input.
- Preserve documented entity offset semantics across the Rust core and all bindings.

## Structure

- Add files only for distinct logical components.
- Do not introduce abstractions without a demonstrated need in the current codebase.
- Keep bindings thin: PII detection semantics belong in `crates/core`.
- Do not duplicate detector logic in Python, Node, or WASM bindings.
- Do not perform unrelated refactoring while implementing a scoped change.

## Code

- Comments document public behavior or explain non-obvious reasoning; do not narrate syntax.
- Prefer descriptive names over abbreviations.
- New third-party dependencies require a concrete need that cannot be met reasonably with the standard library or existing dependencies.

## Changes

- Keep each pull request focused on one behavior or maintenance goal.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` before merging Rust changes.
