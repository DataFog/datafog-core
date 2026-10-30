# I5 combined resource assessment — synthetic, not a device SLA

Measured Core source commit `ef15b5979e88161838ea2ce8e1e62a0e8d209754`; macOS arm64, 48 GiB physical RAM, Rust1.94.1 native harness. Existing Core example generated canonical near-capacity fixtures with Rust1.88.0. Source bytes were stable during all five fresh processes. Only ignored Core target files were added. [native-observations.json](evidence/fogvault-budget-2026-10-30/native-observations.json) retains source, binary, harness, dependency-lock and fixture hashes; each profile retains raw OS `/usr/bin/time -l` output and phase `ps` RSS samples. The retained [probe](evidence/fogvault-budget-2026-10-30/probe.rs), standalone manifest/lock and [fixture metadata](evidence/fogvault-budget-2026-10-30/fixture-metadata.json) preserve the measurement construction. Regenerate `near-ordinary.cbor` with the existing `workspace_conversation_capacity` example at the recorded source commit; the generated fixture and compiled binaries are deliberately omitted. The32-plan baseline requires that recorded pre-admission source.

## Actual combined native results

| Profile | Candidate bytes | Rows after save | Whole-process peak RSS MiB |
|---|---:|---:|---:|
| mappings-1 | 190 | 2 | 68.41 |
| mappings-1000 | 60,744 | 2 | 70.61 |
| mappings-5000 | 308,744 | 2 | 77.72 |
| history-1 | 7,369,632 | 99,744 | 197.64 |
| history-32 | 7,369,632 | 99,744 | 865.39 |

Each process creates a registered SQLite/control vault using the actual native Owner, prepares review(s), confirms a save, locks and drops its owner, authenticates/reopens and verifies exact restored text. History profiles first authenticate/decode the existing generated 99,742-operation/49,870-context graph (7,369,524 bytes), stage Core recovery, import into fresh keys, then prepare a new synthetic email review. The saved graph has99,744 operations and preserves49,870 confirmed handoffs. The32-plan case holds32 competing reviews against one head, commits one, drops the others and reopens. It does not issue32 independent committed operations.

Peak RSS includes Rust Core, FogVault KDF/crypto/SQLite, native runtime and allocator caches throughout all phases; process starts are isolated. It excludes Node/Python conversion copies and browser/Worker/renderer processes. Phase `ps` samples are points, not phase peaks. Memory left resident after lock/drop is not proof that secret buffers remain live: allocators/OS caches can retain freed pages. OS peak is one observation per profile; no percentile, device floor, latency guarantee, power-loss qualification or production-host claim follows. One pending review measured197.64MiB vs865.39MiB for32 on this graph. A16MiB encoded-payload cap is not a memory cap.

The existing example `workspace_conversation_capacity` separately measured exact requested allocations for this graph:32 conversation plans retain814,801,088 additional requested bytes; dropping them returns that requested-byte delta to0. Counting-allocator requested bytes exclude allocator overhead. This cross-check supports clone pressure, not a whole-process budget.

## Browser evidence and limits

Existing actual joint browser report `fogvault-v1/target/fogvault-integration/browser-boundary-fixes.json` passed13 installed Core+FogVault Worker/IndexedDB cases. Mapping1/100/1000 bundles were190/5,995/60,744 bytes; save samples0.90/3.30/9.40ms and unlock samples123.8/128.8/119.7ms. These are old single-run advisory observations, not fresh latency gates. Its `performance.memory` values were all10,000,000 and exclude Worker/WASM/allocator/foreign copies; they cannot support any process-memory claim.

Committed `docs/proposals/evidence/conversation-runtime-qualification.json` reports actual Core WASM near99,742 rows:32 pending plans reserve WASM linear memory43,712,512→802,684,928 bytes, which remains reserved after discard. This is Core-only evidence; it omits the combined FogVault Worker/Argon2, renderer JS, IndexedDB and process overhead. No new browser process-memory measurement was made. Browser admission and lost-reply/late-save-after-lock behavior already pass actual installed tests; this is lifecycle correctness, not a cancellation deadline.

## Admission before this policy change

- Raw Rust Core: callers own prepared changes; there is no global32-plan quota. The existing examples choose32 as a stress profile.
- Core-owned native Rust Owner: host-owned changes had no retained-plan cap.
- Optional Python owner: weak cleanup registry revoked retained children on close, but did not cap active retained plans or streams.
- Optional Node owner:32 active native plan handles and32 stream handles; finalizers release abandoned handles. This limits handle count, not combined bytes.
- Browser Core WASM:32 retained plan handles. Core-owned browser adapter adds lifecycle checks, but does not reduce this plan cap. Existing Core WASM stream admission differs; no global multi-owner memory cap is implied.
- Rust/Python/Node/Browser inputs still have Core's independent source1MiB,10,000findings,32inputs,20,000entries and16MiB bundle limits; native wrappers admit64 vault records and88-byte metadata. The vault payload failure tests prove no successful handoff and unchanged durable prior state after reopen. They do not prove low memory before rejection.

## Integrated-owner admission policy

Core-owned integrated owners now admit **one active prepared change per owner**. They reject a second preparation before cloning Core state. They release that slot on explicit discard, confirmed consume, drop/finalization and owner close; preserve tenant/thread/foreign-plan and failure semantics. The native Rust Owner enforces this once for Node/Python; the browser adapter enforces the matching policy. Raw Core semantics stay unchanged. Do not introduce a higher configurable default until its combined budget is measured. Existing caller-held previews/foreign byte copies remain host-owned.

Stream count and multi-owner/model concurrency also need explicit host policy: a buffered response can retain up to16MiB, so32 streams/owners can defeat a one-plan limit. Avoid claiming that a per-owner cap is global admission. Measure representative maximum buffered/progressive streams and either admit only one active response by default or document host-owned aggregate limits. Defer exact byte budgets until actual measurements support them.

## Concrete remaining I5 acceptance

1. Keep integrated-owner concurrency admission before clone/allocation and verify slot reuse after discard, success, failed preparation, close and abandoned-handle collection. This bounds one owner's prepared changes; it does not close the remaining items below.
2. Repeat combined lifecycle peak observations at near history capacity with admitted concurrency, plus source-limit batch and buffered/progressive response pressure, and installed Node/Python conversion costs. Record missing dimensions rather than assuming native Rust covers them.
3. For each advertised browser, collect actual combined application memory including Core WASM and FogVault Worker. Prefer an isolated browser process-tree observation and explicit accounting; sampled RSS sums double-count shared pages and are not physical-memory peaks. `performance.memory` alone is insufficient. Do not add an SLA threshold from this48GiB machine.
4. Verify lifecycle shutdown after submitted save and between bounded jobs at near-capacity, while keeping late durable confirmations from reactivating old owners. Native and Core WASM preparation are synchronous and have no in-call cancellation deadline. Shipping UI/host scheduling remains a separate decision.
5. Select actual host/device classes and a product admission budget from measured data; run the shipping extension/native hosts on them. The matrix below is a qualification plan, not an approved support floor.

| Target | Evidence now | Remaining before claiming resource support |
|---|---|---|
| Rust native macOS arm64 | New actual SQLite/KDF whole-process RSS profiles; existing lifecycle tests | Batch/stream/concurrency policy and repeat observations; host budget |
| Installed Node/Python native | Existing installed lifecycle/tenant/GC/interop tests | Actual foreign-copy combined peaks and declared aggregate concurrency |
| Linux/Windows native | CI lifecycle coverage, not new RSS evidence | OS-specific combined memory + actual host shutdown measurements |
| Chromium desktop | Actual installed joint13cases; small mapping timings; Core-only large graph linear memory | Combined Core+Worker+renderer memory and responsive shipping-host scheduling |
| Firefox/Edge desktop | Existing CI functional joint cases | Engine-specific combined memory; no extrapolation fromChromium |
| WebKit | Owner-deferred | Remains explicitly deferred, no support claim fromotherengines |
| Mobile browser/native | No shipping mobile host/device resource evidence in this pass | Actual embedding, storage/lifecycle, low-memory behavior and measured device classes |
| Native model+vault concurrently | Vault-only measurements exclude model workers | Separate #90/model fault/retirement evidence and combined aggregate memory |
