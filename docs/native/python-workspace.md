# Native Python workspace binding (X6)

Draft implementation against released `0.5.0-beta.2` plus committed Core streaming
contract #176 and the committed operation-history candidate #162. The Core
snapshot and workspace contract remain provisional; this binding does not freeze
beta.3 operation or conversation proposals.

| Core contract | Python surface | Evidence |
| --- | --- | --- |
| create/open/export/head/namespace | `Workspace` + immutable `WorkspaceHead` | Installed wheel lifecycle/reopen tests pass |
| prepare/prepare_batch | owned `WorkspacePreparedChange` | Installed unchanged-state, cancel and save tests pass |
| review/prepare_review | Core-owned combined review | Installed overlap/correction/error-index tests pass |
| plan preview/heads/format/policy preview/candidate bytes | cloned result views; exact `bytes` export | Installed ownership and Unicode/uint64 tests pass |
| activate_saved | explicit parent and saved complete heads | Installed stale/foreign/consumed/failed-activation tests pass |
| discard/close/drop | Core handle lifetimes; context managers | Installed close/drop/GC/32-slot/thread tests pass |
| restore/restore_report | Core strict restoration and content-free issues | Installed strict mixed-marker report test passes |
| recall | Core normalized exact recall | Installed Unicode/case fixture passes; shared X8 corpus pending |
| policy/prepare_policy_changes | Core protected rules and exact choices | Installed explicit-null/protected/cyclic-input tests pass |
| prepare_recovery | Core staged recovery | Installed restored aliases/settings/fresh-head test passes |
| operation records/counts/coverage and explicit outcomes | thin native methods matching committed Core #162 | Mandatory installed operation profile covers staged saves, unknown legacy history and owned content-free results; complete X8 operation qualification remains separate |
| conversations | Not exposed | Conversation #148 requires a committed Core contract and native/all-SDK follow-up |
| strict/progressive streaming | `buffered_restore` / `progressive_restore` with owned response handles | Installed split-encoding, late-failure, terminal-count, resource and lifetime tests pass |

The compiled PyO3 classes own Core handles directly. Preparing, querying and
exporting do not write a vault or activate state. The host must obtain approval,
compare-and-save the candidate durably, then pass the complete parent and saved
heads to `activate_saved` before handing off text. A candidate export is sensitive
plaintext snapshot bytes. Python copies, caller findings and returned strings are
caller-owned and cannot be promised cryptographic erasure by this binding.

Passwords, encryption keys, encrypted storage, request correlation and authorization
remain host responsibilities. The separate W6 host proof will cover actual encrypted
compare-and-save/concurrency; this binding alone is not a secure vault or LiteLLM
adapter. It makes no claims about unimplemented provenance or provider/SSE parsing.

Inputs own their data. Preparation checks count and aggregate material bounds before
cloning findings into a call; arbitrary recursive Python policy containers are never
traversed. Result dictionaries preserve the shared camelCase contract fields, with
precise Python integers for head revisions. `WorkspaceHead` exposes immutable
`revision` and `commit_id` properties; IDs and snapshot exports are exact `bytes`.

Each workspace allows 32 live pending changes. A valid public discard/drop releases a slot; a
changed-head activation invalidates and clears stale candidates immediately. A
no-op activation preserves sibling candidates with the same saved head. Closing or
dropping a workspace clears retained candidates and makes surviving handles fail
with `closed_workspace`. Context-manager exits discard/close without suppressing
exceptions. Public discard rejects consumed/closed plans; context and destructor
cleanup remain idempotent. Native lock scopes return Rust-only results before any
Python exception normalization or value conversion, so callbacks can reenter
without deadlocking. The GIL remains held during these bounded synchronous Core calls;
there are no native network, key-provider or persistence callbacks. Independent
threads can open the same saved bytes into separate workspaces.

The installed-wheel tests run through the existing Python contract-check entrypoint
on every supported CI Python version. Shared cross-SDK receipt qualification is
tracked separately in X8 #171. The new operation methods must enter that shared
profile before whole-X6/C1 qualification; the existing lifecycle/streaming corpus
does not establish operation or conversation parity.

## Response restoration

`Workspace.buffered_restore(encoding="utf8")` creates a strict response handle.
Its `feed(bytes)` / `feed_utf16(list[int])` methods return `None`; only `finalize()`
returns the existing atomic restore report. It preserves strict R2 EOF behavior
and never releases a transformed prefix when a later marker is invalid.

`Workspace.progressive_restore(...)` explicitly opts into irreversible delivery.
Each successful feed returns `{text, delivered, complete}` using the shared
camelCase fields (`restoredMarkers` within `delivered`). `complete` remains false
until successful finalization. Terminal `DataFogWorkspaceStreamError` extends
`DataFogWorkspaceError` with exact cumulative delivered counts and `complete=False`;
its inherited marker ranges retain original input coordinates. A failing call
releases none of that call, but earlier successful chunks cannot be retracted.
A host must not retry or present an incomplete response as complete.

Both factory-only handle types support `reset`, `close` and context managers.
Reset starts a new response at the same still-current saved head; it does not
retract prior output. Closing/dropping the parent or activating a changed head
immediately clears its retained child buffers and private lookups. Failed saves,
discarded candidates and same-head no-op activation preserve response handles.
Independent workspaces do not share stream state.

UTF-8 feeds accept Python `bytes`, without an extra Rust input copy. UTF-16 feeds
accept lists of exact integers in `0..65535`; booleans, floats and user numeric
callbacks are rejected and terminate the response through Core. Native UTF-16
copies use the same existing `zeroize` primitive as Core, including rejected
partial input, and are wiped before Python result/error callbacks. The native
copy is bounded to the Core byte budget plus two surrogate-covering units; a
longer valid tail cannot make that one Core feed succeed. Feeds are never split
into multiple Core calls because that would miscount text not yet delivered to
Python. Core owns cumulative byte limits, encoding and marker decisions. Python
allocation failures before feeding Core remain ordinary Python memory errors.

All successful values and typed Core failures are converted after native guards
drop. Installed regression tests reenter the same response from its exception
constructor with dynamic JSON decoding forbidden. Python caller-owned bytes,
lists and returned strings still cannot be promised cryptographic erasure.


## Recorded operation history (draft beta.3 dependency)

The native methods call the actual committed Core implementation from #162:
`create_with_operation`, `prepare_with_operation`,
`prepare_batch_with_operation`, `prepare_recovery_with_operation`,
`prepare_outcome`, `operations`, `counts` and `history_coverage`. No Python
history engine, clock, durable write, logging or automatic failure callback is
introduced. The [shared operation proposal](../proposals/workspace-operation-records.md)
owns the provisional format and remaining review questions.

Operation/outcome inputs are exact builtin dictionaries with camelCase fields,
matching policy input and result conventions:

```python
from datafog_core import Workspace, WorkspaceInput
from os import urandom
from time import time_ns

operation = {"id": urandom(16), "occurredAtMs": time_ns() // 1_000_000}
workspace = Workspace.create_with_operation(urandom(16), operation)
# Bootstrap creation is unsaved: the host must persist its exported bundle.
change = workspace.prepare_batch_with_operation(
    [WorkspaceInput("ordinary synthetic text", [])],
    {"id": urandom(16), "occurredAtMs": time_ns() // 1_000_000},
)
# Review/approve and durably compare-and-save the candidate before activation.
# Candidate export, previews and queries never change committed history.
change.discard()
workspace.close()
```

`id` is exactly 16 Python bytes; `occurredAtMs` is an exact integer in
`0..253402300799999`. Booleans, floats, coercion callbacks, extra fields and custom
mapping subclasses are rejected. The host supplies IDs and timestamps; timestamps
are assertions and may go backwards. Core's committed head orders mutations.

`operations()` returns copied dictionaries with `id: list[int]`, bounded `kind`,
`occurredAtMs: int | None`, `preparedInputs` and `transformedSpans`.
`counts()` returns the same exact numeric fields as WASM; totals describe retained
recorded rows. `history_coverage()` explicitly returns `recordedHistoryOnly: True`
and `preUpgradeHistory: "unknown"` for old history, or `"not_applicable"` when
the retained create row is present. Reads and result mutation never change saved
bytes. Unknown historical time stays `None`; it is never replaced with the current
clock. Metadata is private vault data, even though rows contain no source text.

Explicit preparation records a successful operation even if no mappings change;
ordinary legacy no-ops still preserve their bytes/head/version. Outcomes accept
only `copy_confirmed`, `user_cancelled`, `blocked_input` and `restore_blocked`.
Blocked outcomes require 1–10,000 issues; copy/cancel require zero. Issue details
and the count are validated but not retained. Each saved blocked outcome counts
one reported attempt, rather than its issue count. `confirmedHandoffs` stays zero:
copy confirmation grants no provider-delivery or handoff authority. Merely calling
restore, observing a failure, exporting/discarding a candidate or failing a save
never appends an outcome. All new changes use the existing bounded plan registry
and complete-head save/activation contract.

Actual v1/v2 read/re-export and legacy no-ops remain byte-preserving. An approved
real save may stage v3, with old history explicitly unknown and original aliases
and choices preserved. Explain that beta.1/beta.2 cannot reopen upgraded v3 state
before host approval. Core controls the approved provisional 100,000 retained-row
ceiling and final 256 recovery slots, along with the snapshot byte limit; the
binding does not evict history or promise indefinite recovery. New explicit
contexts report `operation_reused` for known reused IDs. The shared proposal's
legacy replay split and future conversation schema remain review work.

The mandatory installed operation tests exercise all eight methods without
feature-detection skips, timestamps/transport errors, atomic batch counts and
original error indices, read copies, staged no-ops, discarded/failed saves,
explicit outcomes, recovery, consumed/closed/32-slot lifetimes and v1/v2 upgrades.
The same public typed calls are checked by mypy and pyright, including negative
missing/extra fields and invalid outcome kinds. Native host/provenance integration,
shared operation parity and final v3 compatibility remain separate gates.
