# ADR 004: Workspace Aliasing V1 Decisions

- **Status:** Proposed
- **Date:** 2026-10-26

## Context

Workspace aliasing gives each sensitive value a persistent pseudonym, kept in an
encrypted local vault, so model output can be restored to real names. In V1 a
person pastes free text into the DataFog Chrome extension. The extension
pseudonymizes it, the person sends it to ChatGPT, and the answer is restored to
real names on their machine. Follow-up questions keep the same pseudonyms.

The PRs that follow this one build that base on synthetic data: Core aliasing, the
CBOR snapshot, the WASM workspace API, follow-up text, restoration with an issue
report, an encrypted browser vault, backups and a browser-restart test.

Issued markers become permanent once real text is saved in a workspace. Anything
later stages need from a workspace (file intake, native app migration, sync, more
chat surfaces) has to be recorded from then on. This ADR settles the V1 decisions
and the snapshot contents that must exist before that point.
[workspace-v1-plan.md](../workspace-v1-plan.md) sequences the work.

## Decision

### Scope

Free-text paste is the V1 baseline: text in, pseudonymized text out, answer
restored. File upload is an open question that the extension investigation
answers:

- CSV is the most tractable format and the likely first file type.
- Excel is out, because of formulas and linked cells.
- PDFs split into machine-readable ones, which may follow, and ones that need
  OCR, which are out.
- An unsupported file type shows a clear "not supported" message. It is never
  partially processed.

### Markers

Markers are `[[LABEL_CODE]]`, for example `[[PERSON_7KQ4ZM]]`.

- `LABEL` is the uppercase ASCII entity type, at most 32 bytes.
- `CODE` is six Crockford base32 characters (`0-9A-Z` without `I L O U`),
  unique within the workspace. Codes are derived by HMAC-SHA256 from the
  host-random operation ID and re-drawn on collision.
- The workspace namespace and issuing operation ID stay in the vault and never
  go on the wire.
- Restoration ignores case and accepts Crockford's `I`/`L` → `1` and `O` → `0`
  substitutions in the code. It never ignores a wrong label.

Six characters, not four: a backup restore loses every code issued since that
backup, and new codes are drawn without knowing them. With about 1,000 codes per
type lost and 1,000 issued afterwards, four characters (about one million codes)
make a silent wrong restoration likely. Six characters (about one billion) make it
about 0.1%. The same space makes a code the model invents almost never resolve.
Putting the full workspace and operation IDs in each marker would give the same
protection at about 40 tokens per marker.

Before the snapshot freeze, the marker preservation and token tests run on
ChatGPT, Claude and Gemini, not only ChatGPT, so later surfaces do not need a
second format.

### Restoration

Restoration is all-or-nothing. `restore_report` returns text only when every
marker resolves. Otherwise it lists each blocking span as `unknown`, `damaged` or
`unbracketed`, with byte and UTF-16 ranges. A committed code whose brackets were
dropped blocks restoration rather than passing through as text.

### Identity

A value's identity is its entity type plus the value lowercased, trimmed and with
whitespace runs collapsed. The first spelling seen is stored and restored.
Punctuation and other differences go through merges.

### Merges

A merge is a record from one code to a canonical code. It never rewrites or
deletes an entry, because the merged code may already be in past chats. New
text uses the canonical code; the merged code keeps restoring, to the canonical
display value. Merges are not required for the V1 paste flow. When they ship,
this is their shape, and undoing a merge deletes the record.

### Types, rules and remembered choices

| Type | V1 rule |
|---|---|
| `PERSON` | Always hidden |
| `ADDRESS` (street address) | Always hidden |
| `EMAIL`, `PHONE` | Always hidden |
| `ACCOUNT_NUMBER`, `CREDIT_CARD`, `SSN`, `US_ROUTING_NUMBER` | Always hidden |
| Credentials (`API_KEY`, `BEARER_TOKEN`, `JWT`, `PRIVATE_KEY`, `CREDENTIAL_URI`) | Always hidden |

Per-value choices are remembered in the encrypted workspace: keep (send as is)
or not sensitive (a scanner false positive). Type rules, remembered choices and
finding sources (`scanner`, `recall`, `user`, plus a needs-review flag) live in
Core. Hosts only display them, so every surface hides the same things. Nothing
sensitive is kept in browser extension storage.

### What leaves the device

Only the text the person approves, with every finding replaced by its marker.
Nothing from the vault is sent. Where the person pastes is the extension's design
choice. Text pasted straight into ChatGPT's message box is already in ChatGPT's
page before DataFog replaces it, so a paste target inside the extension is the
safer default.

### CSV columns, if CSV is in V1

A column whose header names a sensitive field (for example `email`, `address`,
`phone`, `name`) is flagged as a whole for the person to approve. Core already
discovers name fields by key in structured JSON (`discover_fields`, ADR 002, for
`PERSON` only). CSV support reads rows into records keyed by header and extends
the aliases to the other types. Approved columns are pseudonymized cell by cell
through the same workspace, so the same value gets the same marker in pasted text
and in a file.

### Detection for V1

Rules catch emails, phones, card numbers, SSNs, routing numbers and credentials
well. Names and street addresses in free text are the gap: rules miss most of
them.

The in-browser model for V1 is GLiNER2 (`fastino/gliner2-privacy-filter-PII-multi`).
The DataFog PII model is on hold; it may return later through the planned
model-engine interface (ROADMAP.md, step 6). GLiNER2 is a community-tier model
engine: accuracy regressions are warn-only and do not block PRs.

The fallback path, when no model is available or when the model misses a value,
is workspace recall plus the person's own selection. Workspace recall catches
every value they approved before; the person's selection catches anything the
scanner and recall both missed.

**Detection bar (D4).** The synthetic pasted-text set, once it exists, should
measure:

- No missed emails, phone numbers, card numbers, SSNs or account numbers (rules).
- At least 90% of names caught before review, with no recall, using GLiNER2's
  `person` label at a 0.5 threshold.
- Street-address recall is an open measurement. The D4 fixtures have no labelled
  `STREET_ADDRESS` examples yet, so the 90% bar cannot be verified. Add fixtures
  and measure before the pilot gate.
- Every previously approved value caught by recall in later text.

### Workspace snapshot

The snapshot keeps schema version 1 as a draft that each V1 task extends. It is
frozen at the pilot gate. Until then, development snapshots fail closed rather
than migrate. Target contents:

| Key | Contents | Lands with |
|---|---|---|
| 0 | Schema version | Workspace PR |
| 1 | Workspace namespace | Workspace PR |
| 2 | Head: revision, commit ID | Workspace PR |
| 3 | Entries: type, display, code, issuing operation, counter | Workspace PR |
| 4 | Per-label counters | Workspace PR |
| 5 | Merges: from code, to code, operation | Merges (after V1) |
| 6 | Policy: rule per type | Rules and remembered choices |
| 7 | Remembered choices: type, display, disposition, operation | Rules and remembered choices |
| 8 | Operations: ID, kind, time | Provenance |
| 9 | Conversations: surface, the site's own ID, created operation, issued codes | Provenance |
| 10 | Content-free counts | Provenance |

Key 5 is reserved now so that adding merges after V1 needs no migration. The
backup carries the whole snapshot, so a complete snapshot is a complete backup.
Its round-trip test must cover every key before the freeze.

### Repositories and interfaces

- **Extension repo.** The V1 Chrome extension lives in its own repository.
  `examples/workspace-extension` stays in Core as a non-shipping conformance
  harness and gets no new product features.
- **WASM contract.** The WASM workspace API is the contract between the two
  repositories. It is versioned and frozen together with the snapshot.
- **Vault interface.** The extension puts IndexedDB and Web Crypto behind one
  interface: open, save-if-head-matches, lock, export, import. CI tests through
  it, and a later native app replaces it with a Keychain-backed module.
- **Workspaces per vault.** A vault holds a list of workspaces, even while the V1
  UI creates one.

### Real data

Real text goes into a workspace only after the vault encryption review. Before
that, testing with real data uses a throwaway browser profile, or a build that
saves no mappings.

### Out of V1

- the statements product (below)
- Excel, OCR and other file types, pending the file-upload investigation
- remembered 7/30-day unlock
- Python and Node workspace bindings
- sites other than ChatGPT
- merges and undoing them
- the event log

### Deferred: the statements product

Statements (PDF bank and credit-card statements → ChatGPT → a real-name
spreadsheet) are a longer-tail product, possibly a dedicated web product rather
than the extension. A three-month test of the extension on real usage will
measure the gap. These decisions are parked with it, not discarded:

- Only a statement's transaction rows are sent; headers and footers are dropped.
- Core takes statement rows (date, description, amount) and scans only the
  description.
- Merchants are kept by default, and one click turns one into a category alias
  such as `[[GROCERY_7KQ4ZM]]`.
- Statement rules: masked account numbers ("ending in 1234") and transfer payees
  ("ZELLE TO …").

Deterministic pseudonyms for the same vendor across uploads already exist: the
workspace gives a value the same marker every time.

## Consequences

- Marker overhead drops from about 40 tokens to a handful. The tests above
  confirm this on each model.
- After a backup restore, a new code repeats a lost one with about a
  one-in-a-billion chance per pair, rather than never. Conversation records add a
  second check.
- Spellings that differ by case or spacing need no merge.
- Name and address detection in prose depends on the model spike more than it
  did for statements.
- The extension depends on a released or pinned `@datafog/wasm`, so Core changes
  reach it through the versioned contract rather than shared source.
