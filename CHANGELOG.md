# Changelog

This changelog starts October 22, 2026. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Added

- Workspace values of one type that differ only in letter case or spacing share
  one marker, so `JOHN SMITH` and `John  Smith` are one person. Restoration
  returns the first spelling seen. Whitespace-only findings are rejected.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  Markers are short typed codes such as `[[PERSON_7KQ4ZM]]`, unique within the
  workspace. A synthetic browser harness verifies approval, encrypted mapping
  storage, and reuse after browser restart. The workspace API and snapshot format
  are provisional.
