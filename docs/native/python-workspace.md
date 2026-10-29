# Native Python workspace binding (X6)

Draft implementation against released `0.5.0-beta.2` plus committed Core streaming
contract #176. The Core snapshot and workspace contract remain provisional; this
binding does not freeze beta.3 operation or conversation proposals.

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
| operation records/counts/conversations | Not yet exposed by this native slice | Rust/WASM operation draft #147 and conversation #148 require native/all-SDK follow-up |
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
tracked separately in X8 #171 and has not yet passed for this new wrapper.

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
