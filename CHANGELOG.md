# Changelog

This changelog starts October 22, 2026. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Added

- Protected follow-up text in Rust and WASM. Committed workspace pseudonyms stay
  unchanged; unknown or uncommitted markers and overlapping findings block the
  operation. The synthetic browser workflow requires local preview and approval.
- Local PDF text intake in the synthetic browser workflow, with page review,
  aliases shared with CSV and typed text, and whole-batch rejection of scans,
  image-bearing, encrypted, malformed, or unsupported PDFs.
- Local CSV intake in the synthetic browser workflow, with decoded-cell review,
  pseudonymized string tables, and whole-batch rejection on parsing or encoding errors.
- Atomic workspace batches in Rust and WASM. Inputs share pseudonyms and one saved
  snapshot; a rejected input blocks the whole batch. The synthetic browser harness
  supports explicit input removal followed by a fresh preview and approval.
- Workspace values of one type that differ only in letter case or spacing share
  one marker, so `JOHN SMITH` and `John  Smith` are one person. Restoration
  returns the first spelling seen. Whitespace-only findings are rejected.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  Markers are short typed codes such as `[[PERSON_7KQ4ZM]]`, unique within the
  workspace. A synthetic browser harness verifies approval, encrypted mapping
  storage, and reuse after browser restart. The workspace API and snapshot format
  are provisional.
