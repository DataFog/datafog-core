# Browser reversible pseudonymization benchmark

## Scope and pinned subjects

Compare the published DataFog Core 1.1.0 browser SDK and Core-owned persistence adapters with independent FogVault 0.1.0 and SQLite3 Multiple Ciphers. Exercise real packaged Manifest V3 extensions in retail Chrome where available. Never change production runtime behavior, adapter security settings, durability, or resource limits for measurement. Use deterministic synthetic data only; no real PII or provider calls. Record exact package versions, package/archive hashes, extension artifact hashes, Chrome and OS versions, hardware, configuration, and frozen harness commit in every receipt.

## Modes and workloads

1. **Storage/Workspace:** feed identical precomputed Core findings to each adapter. Keep detection outside timed intervals.
2. **Complete extension:** use the same bundled CNN model and inputs with each adapter; record model startup/inference separately from storage and workflow intervals.

Exercise fresh and long-lived workspaces, with substantial operation/conversation history; repeated existing values and mostly new values; short chat messages, statement-style documents, batches, Unicode and table-shaped responses. Target 10, 100, 1,000 and 10,000 active mappings where existing limits admit them. Capture actual serialized complete Workspace bundle size, encrypted artifact size, history size, and every admission failure. Both adapters persist complete Workspace bundles; measure that cost without assuming incremental SQLite updates.

## Shared scenario contract

Each trial receives a versioned JSON scenario: `scenarioVersion`, stable `scenarioId`, `mode`, `seed`, `mappingTarget`, `historyProfile`, `valueReuse`, `inputShape`, and deterministic fixture reference/hash. Backend adapters implement the same lifecycle operations and return structured results; harness code owns browser/profile control, clocks, restart, resource sampling, assertions, and receipts. Workload/report code owns fixture generation and analysis. A trial is eligible for latency comparison only after the common correctness gates pass.

At minimum, run the same end-to-end lifecycle on each backend: create vault → prepare review → explicitly approve → persist encrypted bundle and activate only after confirmed save → persist observed send → scoped restore → terminate and fully restart browser → unlock and restore. Measure separately: vault creation, password unlock, existing-workspace reopening/reconstruction, review preparation, approval-to-confirmed-save/activation and handoff-text availability, observed-send persistence, warm restore, post-restart restore, backup export, and backup recovery. Recovery must preserve mappings and authority; record its mechanism. Keep authenticated bundle migration distinct from encrypted archive backup and mark unsupported archive operations explicitly. Exclude human review and provider/network response time. Keep model inference outside the storage-only mode and separately attributed in complete-extension mode.

## Timing, repetitions, and execution

Use monotonic clocks and document exact start/end events for every interval. Preserve backend security settings. Alternate backend order across blocks, use isolated profiles, and never run competing trials concurrently on one machine. Pilot results are stored separately and never merged into final results. Freeze the harness commit after a pilot and correctness review; final receipts must reference that commit. Independent trials are fresh profile/workspace runs; operations within one profile are correlated and must not be presented as independent samples. Select repetition counts before final execution so each reported p50/p95 has sufficient independent trials; label percentiles unavailable when sample counts are insufficient. Do not drop slow, failed, unsupported, or admission-rejected trials: record them and explain exclusions from latency summaries.

## Correctness gates

Before performance inclusion, prove exact restoration and encrypted persistence across full browser restart; no text handoff before confirmed persistence and activation; wrong credentials, conflicting saves, lost replies, lock during save, stale reviews, and conversation scope; and recovery preserving mappings and authority. Record failure, unsupported behavior, and exclusion explicitly. Archive export/recovery capability is reported separately from the required recovery gate. A scenario failing a gate remains in correctness/failure reporting and is excluded from latency comparisons for both backends when equivalence cannot be established.

## Measurements and limitations

Report p50/p95 latency and failure rates by scenario/backend. Capture process memory, UI stalls, storage footprint and bytes written only where the browser/platform exposes reliable measurements; identify main-thread, Worker, WASM and browser-process sources separately. Label unavailable values and instrumentation overhead. Record complete serialized bundle sizes, history sizes, and admission failures. Do not infer a winner across unlike security settings or unsupported device/browser coverage. Start with available representative hardware and retail Chrome; list absent device/browser coverage and limit recommendations accordingly. Do not invent thresholds or claim statistically unsupported winners.

## Receipt schema

Each immutable JSONL trial receipt includes: schema and scenario versions; run and trial IDs; UTC timestamp; mode/backend; scenario inputs and fixture digest; repository and frozen harness commit; exact SDK/adapter/engine versions and artifact SHA-256 digests; extension package digest; browser/version/channel, OS, CPU, RAM and power state when known; backend security/configuration; profile/workspace lifecycle and randomized order block; named monotonic timing intervals and raw durations; correctness gate outcomes and failure details; mapping/history/input counts and serialized sizes; resource samples with process/thread attribution and method; instrumentation overhead/availability; admission result; and inclusion/exclusion reason. Retain raw receipts unchanged alongside generated summaries and charts.

## Reporting

The report must state methods, device/browser coverage, package identities, correctness exclusions, measurement limitations, per-scenario p50/p95 and failure rate, charts, and workload-specific recommendations supported by receipts. Performance optimization is out of scope. Demonstrated bottlenecks become separate follow-up issues after independent review.
