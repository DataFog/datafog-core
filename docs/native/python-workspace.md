# Native Python workspace binding (X6)

Implementation in progress against the committed policy/combined-review launch
candidate. The Core snapshot and workspace contract remain provisional; this
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
| provenance/counts/conversations | Not implemented in current Core candidate | Await approved, committed #147/#148 contracts |
| streaming | Not implemented in current Core candidate | Await compatible committed #169 contract |

The compiled PyO3 classes own Core handles directly. Preparing, querying and
exporting do not write a vault or activate state. The host must obtain approval,
compare-and-save the candidate durably, then pass the complete parent and saved
heads to `activate_saved` before handing off text. A candidate export is sensitive
plaintext snapshot bytes. Python copies, caller findings and returned strings are
caller-owned and cannot be promised cryptographic erasure by this binding.

Passwords, encryption keys, encrypted storage, request correlation and authorization
remain host responsibilities. The separate W6 host proof will cover actual encrypted
compare-and-save/concurrency; this binding alone is not a secure vault or LiteLLM
adapter. It makes no claims about unimplemented provenance or streaming APIs.

Inputs own their data. Preparation checks count and aggregate material bounds before
cloning findings into a call; arbitrary recursive Python policy containers are never
traversed. Result dictionaries preserve the shared camelCase contract fields, with
precise Python integers for head revisions. `WorkspaceHead` exposes immutable
`revision` and `commit_id` properties; IDs and snapshot exports are exact `bytes`.

Each workspace allows 32 live pending changes. Discard/drop releases a slot; a
changed-head activation invalidates and clears stale candidates immediately. A
no-op activation preserves sibling candidates with the same saved head. Closing or
dropping a workspace clears retained candidates and makes surviving handles fail
with `closed_workspace`. Context-manager exits discard/close without suppressing
exceptions. The GIL remains held during these bounded synchronous Core calls;
there are no native network, key-provider or persistence callbacks. Independent
threads can open the same saved bytes into separate workspaces.

The installed-wheel tests run through the existing Python contract-check entrypoint
on every supported CI Python version. Shared cross-SDK receipt qualification is
tracked separately in X8 #171 and has not yet passed for this new wrapper.
