# Workspace Aliasing V1: Implementation Sequence

Decisions are in [ADR 004](adr/004-workspace-aliasing-v1.md). V1 is free-text
paste: pseudonymize pasted text, send it to ChatGPT, restore the answer. Work runs
in two repositories: `datafog-core` (Core, WASM, detection, harness), driven by
Sid, and the Chrome extension, driven by Josh.

## Foundation stack (merged)

Each PR targets the one before it:

| PR | Change |
|---|---|
| PR process | Changelog policy and PR writing guidance |
| Decisions | ADR 004 and this plan |
| Persistent workspace | Core aliasing with `[[LABEL_CODE]]` markers, CBOR snapshot, WASM API, encrypted vault harness, browser-restart test |
| Identity | Case/spacing-insensitive identity |
| Batches | Atomic multi-input prepare and approve |
| Follow-ups | Committed markers pass through new text |
| Restore report | All-or-nothing restore that lists blocking markers |
| Backup | Encrypted backup export and recovery |

The 0.5.0-beta.1 host/SDK boundary is documented in [the extension integration contract](extension-integration-0.5.0-beta.1.md).

The harness in `examples/workspace-extension` is synthetic-only conformance code.
Its vault, broker and backup flows are the starting point for the extension.
Harness CSV and PDF intake is parked on its own branch as a reference for the
file-upload investigation; it is not part of this stack.

## datafog-core: remaining work

Each item is one PR stacked on the previous one. Every item that adds a snapshot
key adds it to the decoder tests and the backup round-trip.

| # | PR | What the extension gets | Size |
|---|---|---|---|
| 0 | Contract draft | TypeScript declarations for every call through PR 5, so the extension builds against stubs now | S |
| 1 | Finding sources | Each finding tagged `scanner`, `recall` or `user`, plus `needs_review`; previews report each transformation's source | S |
| 2 | Workspace recall | Every committed value found again in new text, by normalized identity at word boundaries | M |
| 3 | Rules and remembered choices | Snapshot keys 6 and 7: a rule per type, and keep / not-sensitive choices per value; recall skips values remembered as keep or not sensitive | M |
| 4 | Combine sources | One finding list from scanner, recall and the person's corrections, using existing overlap resolution | S–M |
| 5 | Provenance and conversations | Snapshot keys 8–10: operation times (WASM calls take `{operationId, issuedAtMs}`), conversation records, content-free counts; restore can check whether a marker was sent into this chat | M |
| 6 | Freeze | Fixtures for every snapshot key, a version constant for the WASM contract, a changelog entry stating the frozen format; gated on the model marker tests | S |

If the file-upload investigation keeps CSV in V1, add:

- **CSV columns by header.** Read rows into records keyed by header; extend
  `discover_fields` aliases beyond `PERSON` to email, address, phone and account
  fields; return whole-column suggestions for approval; pseudonymize approved
  columns cell by cell through the workspace. Size M.

## Detection, in parallel

- **Browser model (D3).** 0.5.0-beta.1 uses the optional English GLiNER2.5 Small
  adapter exposed by `@datafog/wasm/models`. It runs pinned ONNX assets locally
  with no Python worker; importing the SDK does not activate it. Model/tokenizer
  files are about 297 MB and are provisioned separately. See
  [browser setup and limits](browser-model.md) and the
  [0.5.0-beta.1 extension contract](extension-integration-0.5.0-beta.1.md).

  Native GLiNER2 PII/general models and the DataFog native engine remain separate
  adapters. The old native GLiNER2 count-floor fix does not establish Small
  correctness. Qualify the actual browser export's spans and score semantics.
  The browser adapter has input bounds, single-flight inference and strict
  errors, but no inference cancellation/deadline, automatic chunking or worker
  isolation. Measure load, memory and UI stalls in the owning extension context.
  A local native helper remains a possible later fallback if browser resource
  costs are unacceptable. Community accuracy is warn-only; contract regressions
  block. Shared engine/Decima work is parallel, not a prerequisite for 0.5.0-beta.1.

- **Additional free-text rules (D2).** Account numbers after a label, masked forms ("ending
  in 1234", `XXXX1234`), and a baseline street-address rule. Positive and negative
  tests per AGENTS.md. Size M.

- **Test set and scoring (G2).** Synthetic pasted text (emails, notes, message
  threads, forms) with planted values and a scoring script for the ADR 004 bar.

  **Scoring notes:**
  - The G2 detection bar (ADR 004) requires at least 90% of names caught. Measure
    on the shipping browser model with its documented prompt/threshold.
  - PR #115 proposes name/street-address fixtures and evaluation, but remains
    unmerged. Its native GLiNER2 measurements are not Small qualification.
  - Small's full `ADDRESS` spans differ from street-line `STREET_ADDRESS`
    annotations. Resolve that scoring contract before the pilot measurement.
  - Native and browser baselines must retain their engine/export identity and
    community-tier accuracy policy; model contract checks always block.

  Size M.

## Model marker tests

Run before the freeze, on ChatGPT, Claude and Gemini:

1. Use three synthetic pasted texts with 40–60 distinct markers across `PERSON`,
   `EMAIL`, `PHONE`, `ADDRESS` and `ACCOUNT_NUMBER`.
2. Ask five task types: summarize, draft a reply, extract into a table, rewrite,
   and a follow-up that reuses earlier markers.
3. Count markers kept verbatim, kept apart from case or Crockford substitutions,
   unbracketed, damaged and invented. `restore_report` classifies them.
4. Record tokens per marker with each provider's tokenizer.

Proposed pass: at least 99% of referenced markers restore without user action,
with no invented marker that resolves. If one model fails on a label shape, change
the grammar before the freeze, not after.

## Extension

The Chrome extension lives in its own repository and is owned by Josh. All items
below are extension-side work.

1. **Bootstrap.** Port the harness broker, vault, storage, backup and RPC code.
   Pin `@datafog/wasm` to a beta. Keep fault-injection hooks out of release builds.
2. **Vault interface.** Open, save-if-head-matches, lock, export, import, over a
   vault that holds a list of workspaces. CI tests through it.
3. **Paste surface.** Where the person pastes and reviews. A text area inside the
   extension keeps raw text out of ChatGPT's page; pasting into ChatGPT's box and
   replacing it in place is quicker but puts raw text in the page first.
4. **Review.** Findings highlighted by source; add or undo; a remember toggle
   for keep and not sensitive; approve. Needs Core PRs 1–4.
5. **Handoff.** Copy, or insert into ChatGPT's message box (the prototype's
   insertion code), and write a conversation record.
6. **Restore.** Paste the answer, restore all-or-nothing, highlight issue ranges
   from `restoreReport`.
7. **Follow-ups.** Later questions in the same workspace keep earlier markers.
8. **Settings guide and cleanup.** Temporary Chat, memory and training settings;
   clear temporary data on lock.
9. **File-upload report.** CSV, Excel and PDF findings, and a clear "not
   supported" message for anything V1 doesn't take.
10. **End-to-end CI.** Paste → detect → approve → save → restart → recall →
    restore, against the pinned WASM build.

## Prerelease scope

The [requirements spreadsheet](https://docs.google.com/spreadsheets/d/1Z5bwpzXiyncgg4nC5tPehSoO3mvg6nTGh8gxI4Q-3y0/edit)
is the source of truth for scope and numeric version assignments. This table
summarizes the current plan; TBD is not an assigned release number.

| Version | Sheet requirements | Extension use |
|---|---|---|
| `0.5.0-beta.1` | W1–W5, P1–P6, D1/D3, R1/R2, B1, X1/X3–X5 | Workspace/vault integration, review and approved handoff, strict restore, follow-ups, backup and optional browser Small |
| `0.5.0-beta.2` | D4/D5 | Finding-source review and workspace recall |
| TBD | D6–D8 | Type rules, remembered choices and combined findings |
| TBD | C1–C3, R3 | Operation/conversation provenance and conversation-aware restore |

Other unassigned requirements and pilot/scope decisions retain their sheet's
version or TBD value. Do not infer a release number from implementation order.

## Pilot gate

Real text goes into a workspace only when all of these hold:

- The model marker tests pass and the snapshot and WASM contract are frozen.
- G2 meets the ADR 004 detection bar.
- The backup round-trip covers every snapshot key.
- The independent shipping-host vault/backup encryption review (G4) is done.
  [B3 / #142](https://github.com/DataFog/datafog-core/issues/142) fixes new backup
  exports at Argon2id v1.3, 256 MiB, three passes and one lane, rewrapping the root
  without the old 64 MiB password wrapper. The KDF decision is recorded in
  [ADR 005](adr/005-backup-export-kdf.md); it does not complete the G4 review.

Before that, real-data testing uses a throwaway browser profile or a build that
saves no mappings. The critical path is the review screen (Core PRs 1–4) and
GLiNER2.5 Small in the browser, which decides how many names are caught before review.

## 0.5.0-beta.1 readiness and remaining pilot risks

The release checklist is [issue #132](https://github.com/DataFog/datafog-core/issues/132).
0.5.0-beta.1 is a developer integration package for synthetic Chrome extension tests;
later prereleases and the final pilot-format freeze are separate milestones.
Pin the exact
package version and recreate provisional development vaults on upgrades unless a
migration has been explicitly qualified.

1. **Extension execution.** The current adapter works in a browser page. The
   candidate tarball must also load under actual Manifest V3 CSP with matching
   local runtime assets, then prove model findings through approved encrypted
   workspace save, activation, restore and browser restart.
2. **Resource costs.** About 297 MB of model assets, first-load UI stalls and
   process memory need recorded measurements. A successful developer test does
   not establish low-memory or production PII qualification.
3. **Detection scoring.** Review #115 and measure Small with matching prompt,
   threshold and address-span definitions. Native GLiNER2 figures cannot be
   substituted for browser qualification.
4. **Pilot protection.** Finish recall/rules/provenance, provider marker tests,
   vault/backup encryption review and the intended snapshot-key round trips
   before declaring the pilot contract frozen.
