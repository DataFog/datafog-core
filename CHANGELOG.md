# Changelog

This changelog starts October 22, 2026, with PR #91. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Added

- Encrypted workspace backup and restore in the synthetic browser workflow, using
  the vault password or an offline recovery code. Imports require local approval,
  reject occupied destinations, and preserve aliases under fresh workspace keys
  and a new Rust/WASM recovery commit head.
- Protected follow-up text in Rust and WASM. Committed workspace pseudonyms stay
  unchanged; invalid or uncommitted markers and overlapping findings block the
  operation. The synthetic browser workflow requires local preview and approval.
- Local PDF text intake in the synthetic browser workflow, with page review,
  shared CSV/XLSX aliases, and whole-batch rejection of scans, image-bearing,
  encrypted, malformed, or unsupported PDFs.
- Values-only XLSX intake in the synthetic browser workflow, with sheet/cell review,
  exact stored values and number formats, and whole-batch rejection of formulas.
- Local CSV intake in the synthetic browser workflow, with decoded-cell review,
  pseudonymized string tables, and whole-batch rejection on parsing or encoding errors.
- Atomic workspace batches in Rust and WASM. Inputs share pseudonyms and one saved
  snapshot; a rejected input blocks the whole batch. The synthetic browser harness
  supports explicit input removal followed by a fresh preview and approval.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  A synthetic browser harness verifies approval, encrypted mapping storage, and
  reuse after browser restart. The workspace API and snapshot format are provisional.
