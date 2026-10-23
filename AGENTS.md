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
- From October 22, 2026, user-facing changes require a concise entry under `[Unreleased]` in `CHANGELOG.md`; earlier changes are not backfilled.
- Changes with no user-facing effect require both the `no-changelog` PR label and `Changelog exemption: <reason>` in the PR body. See [CONTRIBUTING.md](CONTRIBUTING.md).
- Check the whole PR against its target branch; do not duplicate changelog entries for each follow-up commit.

## Pull request writing

- Lead with what changes and why, using concrete behavior and direct language.
- Keep descriptions proportional to the change; a few short paragraphs usually suffice. Use headings and lists only when they help the reviewer.
- Explain design decisions needed to assess the change. Keep file inventories, implementation walkthroughs, and future roadmaps in linked documentation.
- Include relevant test results or measurements. Let CI carry routine checklists.
- State material limitations once and directly. Avoid hedging, repeated caveats, and speculative warnings.
- Describe the final change for a reviewer without conversation context.
