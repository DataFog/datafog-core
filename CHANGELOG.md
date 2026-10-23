# Changelog

This changelog starts October 22, 2026, with PR #91. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Added

- Atomic workspace batches in Rust and WASM. Inputs share pseudonyms and one saved
  snapshot; a rejected input blocks the whole batch. The synthetic browser harness
  supports explicit input removal followed by a fresh preview and approval.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  A synthetic browser harness verifies approval, encrypted mapping storage, and
  reuse after browser restart. The workspace API and snapshot format are provisional.
