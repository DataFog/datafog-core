# Changelog

This changelog starts October 22, 2026. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Changed

- Prepare `0.5.0-beta.2` (Python `0.5.0b2`): sources, recall, exact saved choices,
  combined protected review, scoped detector rules and stronger backup passwords.
  Old vaults retain v1 bytes until a real approved saved upgrade; beta.1 cannot
  open upgraded policy-v2 state. Native workspace bindings remain beta.3 work.

- New browser workspace backups require password reauthentication and use
  Argon2id with 256 MiB and three passes, replacing the older password wrapper.
  Legacy backups remain importable; restored new backups retain the stronger
  profile for password unlock.

- Prepare `0.5.0-beta.1` (Python `0.5.0b1`) as a developer integration
  release: persistent workspace handoff/restore and optional English browser Small
  model sessions. Pin this beta exactly and recreate disposable development vaults
  on upgrades; snapshot compatibility remains provisional.

- Toolchain versions are now pinned in repository root files (`rust-toolchain.toml`, `.nvmrc`, `Cargo.lock`, `bindings/wasm/package.json`) as the single source of truth; the Dockerfile, devcontainer, and CI read these files instead of hardcoding versions
- `cargo dfv doctor` reads version requirements from pin files and fails loudly when a pin file is missing; it no longer uses hardcoded version constants

### Added

- Native Node exposes the shared workspace lifecycle, staged changes, policy,
  recall, combined review, strict buffered and opt-in progressive restoration
  without loading browser WASM. Streaming preserves Core error counts, encoding
  validation and parent/head lifecycle cleanup.
- Native Python workspace handles expose Core preparation, combined review,
  recall, protected choices, recovery and strict/opt-in progressive restoration
  with typed errors, exact delivery counts, explicit saved-state activation and
  bounded pending-plan lifetimes.
- Rust and browser WASM can restore UTF-8 or UTF-16 response chunks against a committed workspace
  head, with strict buffered finalization by default and a separate opt-in
  progressive adapter. Late marker/encoding errors stop progressive output with
  original ranges and delivered counts; unfinished or oversized possible markers
  stop incomplete, while the strict default retains existing R2 behavior.
- Detect bounded English street lines after explicit street/shipping/billing
  address labels across SDKs, retaining broader model address coverage.


- Detect explicitly account-labelled four-digit endings and bounded X/asterisk
  masks without inferring hidden digits or changing workspace literal identity.

- Labelled `ACCOUNT_NUMBER` detection for bounded contiguous numeric formats in
  Rust, Python, Node and WASM, preserving configured models’ broader account coverage.


- Workspace combined review merges scanner findings, committed-value recall and
  manual selections, preserving source metadata and original coordinates. Unresolved
  coverage/type conflicts block handoff with ranged review errors before a plan is
  created; legacy supplied-finding preparation receives the same protection.

- Core-owned workspace rules and exact remembered choices, including explicit
  protected-type false-positive corrections, with staged approval/save, recall
  enforcement and a warned, byte-preserving legacy-to-policy-v2 upgrade.

- Rust and WASM workspaces can recall committed values in new text, with source
  metadata and original byte/codepoint/UTF-16 ranges. Queries return all matches
  or an explicit limit failure and never change saved mappings or bypass approval.

- Workspace findings and preview transformations carry scanner/recall/user source
  and review metadata in Rust and WASM. Existing callers default to scanner and
  review required; these display flags never bypass approval or persistence.

- Transformations stop with `review_conflict` when overlap selection would expose
  part of another eligible finding. Resolve the conflict before retrying;
  providers are not called and no partial transformed result is returned.

- Encrypted workspace backup and restore in the synthetic browser workflow, using
  the vault password or an offline recovery code. Imports require local approval,
  reject occupied destinations, and preserve aliases under fresh workspace keys
  and a new Rust/WASM recovery commit head.
- `restoreReport` in Rust and WASM restores model output all-or-nothing and lists
  every unknown, damaged or unbracketed marker with byte and UTF-16 ranges, so a
  host can show what blocked restoration. Restoration also rejects a committed
  marker whose brackets were dropped (`PERSON_7KQ4ZM`) instead of leaving the code
  in the restored text.
- Protected follow-up text in Rust and WASM. Committed workspace pseudonyms stay
  unchanged; unknown or uncommitted markers and overlapping findings block the
  operation. The synthetic browser workflow requires local preview and approval.
- Atomic workspace batches in Rust and WASM. Inputs share pseudonyms and one saved
  snapshot; a rejected input blocks the whole batch. The synthetic browser harness
  supports explicit input removal followed by a fresh preview and approval.
- Workspace values of one type that differ only in letter case or spacing share
  one marker, so `JOHN SMITH` and `John  Smith` are one person. Restoration
  returns the first spelling seen. Whitespace-only findings are rejected.
  **Migration note:** development snapshots with case-variant duplicate entries
  will fail to decode; discard them and start fresh.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  Markers are short typed codes such as `[[PERSON_7KQ4ZM]]`, unique within the
  workspace. A synthetic browser harness verifies approval, encrypted mapping
  storage, and reuse after browser restart. The workspace API and snapshot format
  are provisional.
- `.devcontainer/devcontainer.json` for VS Code, Cursor, and GitHub Codespaces dev environments
- Dev environment setup documentation in CONTRIBUTING.md covering devcontainer and local install paths, verification steps, and troubleshooting
- `scripts/pin.sh` helper to read pinned tool versions from repository files
- Optional browser Small model sessions return canonical findings for the scan/transform flow, with Core-owned entity routing and score evidence.
- Experimental browser entity extraction using pinned GLiNER2.5 Small ONNX assets, configurable labels, and explicit Unicode ranges (#116)
- Agent-friendly verification CLI (`cargo dfv`) with `doctor`, `build`, `check`, `parity`, `eval`, `bench`, and `smoke` commands for cross-SDK testing (#108, #109)
- Per-entity precision/recall/F1 evaluation with tiered accuracy gating: first-party engines fail on F1 drops >1pt, community engines warn only (#109)
- Provisional baselines for DataFog and GLiNER2 models; regressions against provisional baselines warn but do not block PRs (#109, #112)
- `ROADMAP.md` documenting current and planned work, engine maintenance policy, and process for proposing new engines (#110)
- Cursor Cloud Agent environment with Dockerfile, install script, and verification skill for `cargo dfv` workflows (#111)
- Documentation for GLiNER2 model integration in `docs/gliner2-model.md` (#112)

- Native model sessions accept an explicit DataFog or GLiNER2 engine selector, with shared validation across Rust, Python and Node and compatibility for existing configurations.
- Model sessions expose a Core-owned description of their configured backend, model identity, label mapping and raw-score scale across Rust, Python, Node and browser SDKs.

### Fixed

- Private snapshot encoding wipes replaced native buffers and failed serialized
  prefixes. Successful bundle exports retain their existing caller-owned bytes.

- Native SSN findings retain their type when a semantic PERSON finding covers
  exactly the same span, after entity selection and allowlists are applied.

- Live SDK parity now fails on missing bindings, failed scans, malformed responses, invalid offsets, or missing fixtures; reports identify the failed SDK and validate JavaScript UTF-16 ranges.

- WASM workspace activation drops retained plans whose parent head is stale,
  releasing their previews and capacity. No-op and failed activations preserve
  valid current-head plans; explicitly discard unused live plans as before.

- Workspace preparation rejects damaged, unknown and unbracketed committed source
  markers before approval, with content-free byte, codepoint and UTF-16 ranges.
  The browser harness highlights blocked input and supports editing and retry.

- Development containers build with the pinned tool versions and mount the repository at the configured workspace path.
- GLiNER2 entity-count gating now floors predictions at 1, preventing valid spans from being skipped when the count classifier selects 0 (#112)
