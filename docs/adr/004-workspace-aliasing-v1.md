# ADR 004: Workspace Aliasing V1 Decisions

- **Status:** Proposed
- **Date:** 2026-10-23

## Context

Workspace aliasing gives each sensitive value a persistent pseudonym, kept in an
encrypted local vault, so model output can be restored to real names. V1 puts real
bank statements through it: a month of PDFs is imported, the transaction table is
pseudonymized and sent to ChatGPT, and the answer is restored to real names
locally. The PRs that follow this one build the base on synthetic data: Core
aliasing, the CBOR snapshot, the WASM workspace API, an encrypted browser vault,
backups and a browser-restart test.

Issued markers become permanent at the pilot, when the first real statement is
saved. Anything the later stages need from a workspace (native app migration,
sync, more chat surfaces) has to be recorded from then on. This ADR settles the
v1 decisions and the snapshot contents that must exist before that point.
[workspace-v1-plan.md](../workspace-v1-plan.md) sequences the work.

## Decision

### Markers

Markers are `[[LABEL_CODE]]`, for example `[[PERSON_7KQ4ZM]]`.

- `LABEL` is the uppercase ASCII entity type or category, at most 32 bytes.
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
ChatGPT, Claude and Gemini, not only ChatGPT, so Stage 3 does not need a second
format.

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
display value. Undoing a merge (Stage 1) deletes the record.

### Types, rules and dispositions

| Type | V1 rule |
|---|---|
| `PERSON` | Always hidden |
| `ACCOUNT_NUMBER` (including masked forms) | Always hidden |
| `PHONE`, `EMAIL` in descriptions | Always hidden |
| `MERCHANT` | Kept by default; one click turns a merchant into a category |

A category uses the marker grammar with the category as the label, for example
`[[GROCERY_7KQ4ZM]]`, so it restores to the merchant's real name.

Per-value decisions are remembered in the workspace: keep, category, or not
sensitive (a scanner false positive). Type rules, dispositions and finding
sources (`scanner`, `recall`, `user`, plus a needs-review flag) live in Core.
Hosts only display them, so every future surface hides the same things.

### What leaves the device

Only the transaction table's rows are sent: date, description and amount.
Header address, footers and marketing text are dropped, not pseudonymized. Only
the description column is scanned; date and amount are passed through unchanged.
Because nothing outside parsed rows is sent, PDF intake may accept pages with
images and annotations once the table extractor gates output. It still blocks
encrypted, scanned, scripted and form PDFs.

### Detection for the pilot

Sources are statement rules (D1), workspace recall and user corrections. The
in-browser model (D2) is a spike and does not gate the pilot. Proposed D4 bar on
the synthetic statement set:

- No missed account numbers, labeled or masked.
- At least 95% of names in descriptions caught before review in a first month
  with no recall.
- Every previously approved value caught by recall in later months.

### Workspace snapshot

The snapshot keeps schema version 1 as a draft that each v1 task extends. It is
frozen at the pilot gate (task 2.2). Until then, development snapshots fail closed
rather than migrate. Target contents:

| Key | Contents | Lands with |
|---|---|---|
| 0 | Schema version | workspace PR |
| 1 | Workspace namespace | workspace PR |
| 2 | Head: revision, commit ID | workspace PR |
| 3 | Entries: type, display, code, issuing operation, counter | workspace PR |
| 4 | Per-label counters | workspace PR |
| 5 | Merges: from code, to code, operation | 1.3 |
| 6 | Policy: rule per type | 1.2 |
| 7 | Remembered decisions: type, display, disposition, operation | 1.2 / 6.5 |
| 8 | Operations: ID, kind, time | provenance |
| 9 | Conversations: surface, the site's own ID, created operation, issued codes | 6.6 |
| 10 | Content-free counts | 6.5 / 6.7 |

The backup carries the whole snapshot, so a complete snapshot is a complete
backup. Its round-trip test must cover every key before the freeze.

### Repositories and interfaces

- **Extension repo.** The v1 Chrome extension lives in its own repository.
  `examples/workspace-extension` stays in Core as a non-shipping conformance
  harness and gets no new product features.
- **WASM contract.** The WASM workspace API is the contract between the two
  repositories. It is versioned and frozen together with the snapshot.
- **Vault interface.** The extension puts IndexedDB and Web Crypto behind one
  interface: open, save-if-head-matches, lock, export, import. CI tests through
  it, and Stage 2 replaces it with a Keychain-backed module.
- **Workspaces per vault.** A vault holds a list of workspaces, even while the v1
  UI creates one.

### V1 cuts

Out of v1:

- remembered 7/30-day unlock
- OCR
- .xlsx input
- Python/Node
- sites other than ChatGPT
- undoing merges
- the event log

The real-name export is CSV only.

## Consequences

- Marker overhead drops from about 40 tokens to a handful. The tests above
  confirm this on each model.
- After a backup restore, a new code repeats a lost one with about a
  one-in-a-billion chance per pair, rather than never. Conversation records add a
  second check.
- Statement spellings that differ by case or spacing need no merge.
- The extension depends on a released or pinned `@datafog/wasm`, so Core changes
  reach it through the versioned contract rather than shared source.
