# Workspace Aliasing V1: Implementation Sequence

Decisions are in [ADR 004](adr/004-workspace-aliasing-v1.md). V1 is free-text
paste: pseudonymize pasted text, send it to ChatGPT, restore the answer. Work runs
in two repositories: `datafog-core` (Core, WASM, detection, harness), driven by
Sid, and the Chrome extension, driven by Josh.

## This stack

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

- **Browser model (D2).** On the V1 detection path now: names and street
  addresses in prose are what rules miss. Run the DataFog PII model in ONNX
  Runtime Web inside an extension worker; measure size, speed on a Mac, and
  agreement with the native model. Sid owns the model, the ONNX build and
  accuracy; Josh owns the worker wiring. The fallback, if the browser is too slow,
  is a small local helper reached through native messaging. Size M–L.
- **Free-text rules (D1).** Account numbers after a label, masked forms ("ending
  in 1234", `XXXX1234`), and a baseline street-address rule. Positive and negative
  tests per AGENTS.md. Size M.
- **Test set and scoring (D4).** Synthetic pasted text (emails, notes, message
  threads, forms) with planted values and a scoring script for the ADR 004 bar.
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

## Betas

| Beta | Ships after | Unblocks |
|---|---|---|
| A | This stack | Vault, handoff, restore, follow-ups, backup |
| B | Core PRs 1–2 | Review highlights and recall |
| C | Core PRs 3–4 | Remembered choices and a complete review screen |
| D | Core PR 5 | Conversation records |

## Pilot gate

Real text goes into a workspace only when all of these hold:

- The model marker tests pass and the snapshot and WASM contract are frozen.
- D4 meets the ADR 004 bar.
- The backup round-trip covers every snapshot key.
- The vault encryption review is done.

Before that, real-data testing uses a throwaway browser profile or a build that
saves no mappings. The critical path is the review screen (Core PRs 1–4) and the
browser model, which decides how many names are caught before review.
