# Fuzz targets

Three `cargo-fuzz` targets for the surfaces that parse untrusted bytes:

- `workspace_snapshot` — the CBOR workspace snapshot decoder, through the same
  entry point production uses. Every accepted snapshot must re-encode to the
  same bytes, decode again, and open as a workspace that exports them.
- `utf16_range` — UTF-8 byte range to UTF-16 offset conversion. Text is derived
  from arbitrary bytes via `String::from_utf8_lossy`, and ranges are derived from
  the text length so out-of-bounds and mid-character endpoints occur.
- `email_context` — two planted ASCII or internationalized addresses in
  mixed-script context, with arbitrary UTF-8 text before and after them. Every
  address must stay covered by EMAIL findings, and every returned finding must
  have faithful byte and UTF-16 ranges.

This crate is intentionally not a member of the root workspace: libfuzzer link
flags and the nightly-only sanitizer setup must not reach the published crates.

## Run

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked --version 0.13.2
cargo +nightly fuzz run workspace_snapshot
cargo +nightly fuzz run utf16_range
cargo dfv fuzz run email_context
```

A short bounded run, for example `cargo dfv fuzz run utf16_range -- -runs=20000`,
is enough for a smoke check. `cargo dfv fuzz list` lists the targets and
`cargo dfv fuzz build` format-checks and builds all three without fuzzing.

Crash artefacts land in `artifacts/` (git-ignored). Reproduce one with
`cargo +nightly fuzz run <target> artifacts/<target>/<file>`, and keep it in
`corpus/<target>/` once fixed so the case stays covered.

## Corpus

`corpus/<target>/` holds a handful of small curated seeds per target so a fresh
clone has something to bite on: snapshots from `Workspace::export_bundle`,
`<mode><start><end><text>` byte strings for `utf16_range`, and three context and
address selectors followed by arbitrary text for `email_context`. A run also grows the
corpus locally; commit only curated seeds and regression inputs, not the
generated output, so the tree stays reviewable.
