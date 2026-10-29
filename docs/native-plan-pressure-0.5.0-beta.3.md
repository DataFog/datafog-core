# Native prepared-plan ownership under load (0.5.0-beta.3)

The existing installed suites already prove single-handle 32/33 admission,
discard/GC reclamation, changed-head invalidation and no-op sibling preservation.
Python's encrypted host also exercises two processes over 16 pressure cycles.
This follow-up fills the paired-handle/worker gap in both native SDKs and records
memory scope explicitly; it does not choose an aggregate host admission budget.

`cargo dfv parity --workspace` runs this proof as a mandatory subproof of the
Python/Node native-server profile. It does not add these worker cycles to either
the four-SDK 37-scenario/421-step totals or the native CAS 9-scenario/28-checkpoint
counts. The separate `planPressure` receipt specifies actual fixture dimensions:

| Dimension | Actual fixture |
| --- | --- |
| Runtimes | Installed native Python and Node, separately |
| Independent processes per SDK | 2 |
| Separately opened handles per process | 2, reading the same encrypted workspace/head |
| Admission per handle | 32 accepted, 33rd returns `resource_limit` |
| Simultaneous plans at each barrier | 128 per SDK, not a global limit |
| Standard cycles | 16 per worker |
| Source per pending plan | 256 KiB fixed synthetic SHA-256 hex text |
| Retained stale/closed wrapper references | 1,568 per worker after 16 cycles |

Each worker has its own encrypted workspace row; its two handles reopen that same
row independently. Every cycle proves a discarded slot can be replaced, persists
one selected candidate through the real encrypted compare-and-save path, activates
it and refills all 32 slots after changed-head cleanup. Old wrappers remain held
and reject further access. The independently opened second handle deliberately
retains its stale local head: only its real CAS detects the competing saved head.
It must be closed/reopened; Core is not a cross-handle coordinator.

Both workers first hold all plans at a barrier. Durable writes are then serialized
by the test controller to separate admission/ownership evidence from a second
storage race test. Real races remain in the unchanged native-server profile.
All workers finish and are joined/reaped with exit code zero. The final retained
reference list is cleared and GC sampled; that is not an assertion that every
allocator returns resident pages to the OS.

## Exact allocation evidence versus observations

The existing example-only counting allocator has an additive `--plan-pressure`
mode. It prepares the same 256 KiB source in 32 actual Core plans and discards
their pending state while retaining the plan structs and their preallocated
container. On the arm64 qualification run, requested pending allocations were
about 8.4 MB and returned exactly to the measured baseline after discard. Saved
bundle bytes remain unchanged. This measures shared Core requested allocations;
it is neither a lower-level Core plan limit nor direct PyO3/N-API registry
instrumentation, and allocator overhead is excluded.

The installed processes separately report before/held/released RSS and Python
`tracemalloc` or JavaScript heap/external/array-buffer observations. Those language
heaps exclude substantial native allocations. RSS includes caches/high-water,
code/shared pages and fragmentation; sums are not unique physical memory. No RSS
threshold is used as a shipping budget or as proof of native `free`.

The first 16-cycle Node observation did not establish a resident-memory plateau,
so one bounded controlled extension used the same workload in fresh Node workers:
16 warmup plus 64 additional cycles, retaining 7,840 closed wrappers per worker,
then dropping the retained list and sampling after explicit GC. The retained clean
`39346db` diagnostic showed post-release RSS roughly 93→188 MB /94→195 MB over
80 cycles. The final 16 cycles rose about 1.1 MB /0.9 MB; dropping references
lowered JS used heap about 0.6 MB but did not lower RSS. That remains an
**unresolved residency trend**, not a leak diagnosis or a claim that the entire
native binding heap returns to baseline. The [lossless diagnostic receipt](verification/evidence/native-plan-pressure-node-diagnostic.jsonl)
retains every cycle, before/held/released sample and final reference-drop result.

The 80-cycle investigation is **not a required CI gate**. Normal parity requires
only the 16-cycle Python/Node slot/lifetime proof and exact Core-discard counter.
An explicit `cargo dfv parity --workspace --plan-memory-diagnostic` reproduces
fresh 16-warmup/64-additional Node cycles if more investigation is needed. It adds
no resident-memory acceptance budget. The ordinary receipt marks this diagnostic
as not executed; a dry run never claims it ran.

Passing qualifies per-handle slot/lifetime behavior, complete worker cleanup and
the exact shared-Core discard measurement. Direct native-registry allocation
accounting and resident-memory attribution remain outside this proof. The
small-history fixture also cannot stand in for #162's large-history plan costs.

## Ownership and unselected policy

The per-handle bound multiplies when an application opens more handles/workers.
128 observed plans are a fixture result, not a new Core/process/host-wide maximum.
Production applications still own concurrency, admission and memory budgets; this
change selects none. Keys, encryption, durable CAS and routing stay with the
existing trusted synthetic hosts. No C2 key9 format, send credits, chat identity,
provider handling, retention choice or shipping-host security approval is added.

The original single-handle suites remain intact in
`bindings/python/tests/workspace_conformance.py`,
`bindings/python/tests/workspace_operation_conformance.py`,
`scripts/native-node-operations-conformance.mjs` and
`scripts/native-node-workspace-gc.mjs`. The Python host's existing pressure test
also remains. The new raw receipts are `python-plan-pressure.json`,
`node-plan-pressure.json`, `node-plan-pressure-extended.json`, their exact fixtures
and `core-plan-allocation.json` beside the normal parity outputs. All values are
synthetic; source text and encryption keys are not copied into pressure receipts.
