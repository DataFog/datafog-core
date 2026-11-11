# Browser benchmark harness

The runner loads the exact published DataFog Core 1.1.0, Core-owned browser adapters 1.1.0 and FogVault 0.1.0 packages into a temporary, packaged Manifest V3 extension. It launches installed retail Google Chrome in a unique profile for each trial, reuses that same profile across an explicit Chrome process shutdown/relaunch, and blocks network access. The extension flow writes no provider traffic and accepts only synthetic fixtures.

The flow uses the versioned JSON scenario contract documented in [the benchmark specification](../../../docs/benchmarks/browser-reversible-pseudonymization.md). `storage_workspace` mode supplies the fixture's precomputed Core findings. `complete_extension` mode runs the packaged CNN in its own Worker and combines those scanner findings with the same deterministic fixture values as explicit review selections. This keeps mapping targets controlled while recording CNN startup and inference separately.

## Prerequisites

- Node.js 24 or later and npm.
- Python 3 for generating fixture JSON.
- retail Google Chrome installed at the platform default location, or its executable path supplied explicitly.
- network access for the first `npm pack` of the four pinned published packages. Later trials reuse their hashed archives from the run artifact cache.
- Unix `zip`, `tar` (npm's platform tooling), and `ps` for extension packaging and browser process RSS sampling.

The runner does not fall back to Playwright's bundled Chromium. Chrome 137 and later ignore command-line unpacked-extension load flags in branded builds, so the runner starts retail Chrome with `--enable-unsafe-extension-debugging` and installs the staged MV3 extension through browser CDP `Extensions.loadUnpacked`. If retail Chrome is unavailable, provide a supported installed Chrome binary with `--chrome-executable`.

## One trial

Generate a deterministic fixture, then run one pilot:

```sh
python3 benchmarks/browser/workloads/generate.py \
  --mode storage_workspace --seed 1 --target 10 --history fresh \
  --reuse mostly_new --shape unicode \
  --output target/browser-pseudonymization/fixture.json

node benchmarks/browser/harness/run.mjs \
  --backend sqlite \
  --scenario target/browser-pseudonymization/fixture.json \
  --run-kind pilot \
  --run-id local-pilot \
  --trial-id storage-unicode-sqlite-001
```

Use `--backend fogvault` for the matching FogVault trial. Each receipt records its profile ID and order block so paired trials can be matched. Avoid running two benchmark processes at once on the same machine.

## Sequential matrix

The workload generator can emit a scenario catalog. The matrix runner materializes one digest-checked fixture per scenario, runs each requested independent pair sequentially, and reverses backend order every repeat. It shares the exact package archives and packaged extension artifacts across trials, stores every JSON receipt, and records all failures in `matrix.jsonl`.

```sh
python3 benchmarks/browser/workloads/generate.py --catalog \
  --output target/browser-pseudonymization/catalog.json

node benchmarks/browser/harness/run-matrix.mjs \
  --catalog target/browser-pseudonymization/catalog.json \
  --trials 30 --run-kind pilot --run-id pilot-2026-11-11
```

Use enough independent fresh-profile pairs for any percentile you intend to report. Repeated steps inside one long-history profile are correlated and the analyzer must treat them as one trial. Store `pilot` and `final` runs in separate roots; never pool them. `final` receipts must reference a clean frozen source commit. The matrix runs a complete catalog, so use a filtered catalog file when executing a planned subset. Matrix output roots are immutable: choose a new `--run-id` for each invocation. The runner refuses to overwrite an existing root or truncate a previous matrix ledger.

## Timed lifecycle

The page uses `performance.now()` for each named interval. WASM initialization and model startup/inference are separate; fixture generation and Core finding conversion happen outside `trialPreparation`. A long-history trial creates its seed data, executes every history recipe step through the selected adapter, closes the owner, unlocks and reconstructs the populated workspace, and only then times its trial preparation and save. The handoff previews are read only after the adapter commit resolves and Core activates the saved plan. The flow then persists a synthetic observed-send, verifies a warm scoped restore, and tests an explicit backup/recovery path where that package exposes one.

The runner closes Chrome completely and relaunches it with the same temporary profile. The extension page then performs a fresh password unlock, reconstructs the saved Workspace and verifies exact scoped restoration and observed-send authority. `localStorage` contains only synthetic route/checkpoint data; workspace mappings remain in the adapter's encrypted storage. Successful trial profiles and staging directories are removed after the receipt is written. Failed trials retain `failure-workspace/` for diagnosis; receipts and matrix logs are always kept.

## Receipts and measurement limits

Receipts include the fixture digest, repository/harness commit, exact published package tarball hashes, loaded extension artifact hash, Chrome binary/version, OS/CPU/RAM, backend settings, trial order, named timing boundaries, correctness gates, resource estimates and admission/failure result. Receipts also include the complete set of observed Core candidate-bundle byte sizes. That is the serialized plaintext Workspace bundle passed to the adapter. The sealed on-disk ciphertext artifact size is explicitly unavailable because neither published adapter exposes it through its public API. Browser `StorageManager.estimate()` is an origin-level storage estimate, not per-file accounting. Browser process RSS samples include only the Chrome main process; renderer/utility process memory, Worker heap, WASM heap and reliable bytes-written counters are labeled unavailable. Main-thread stalls use the extension page's `PerformanceObserver` long-task entries.

FogVault 0.1.0 and its Core-owned adapter expose authenticated Workspace bundle recovery but no standalone encrypted backup export API. The runner reports that migration separately and marks backup archive export/import unavailable; it does not count bundle migration as backup qualification. SQLite's published adapter exposes encrypted backup export/import, which the flow verifies in a fresh vault while checking restored mappings and observed-send authority.

Per-trial gates directly cover exact restoration before/after full browser restart, confirmed-save-before-handoff ordering, wrong-password rejection, conversation scope and recovery. Conflict, lost-save-reply and lock-during-save gates are filled only from a package-hash-bound qualification receipt; unsupported gates exclude a trial from latency percentiles. Stale-review rejection is exercised by the timed Core flow. Analyze each immutable `runId` separately; the analyzer rejects input files that mix runs.

## Harness tests

```sh
node --test benchmarks/browser/harness/test/*.test.mjs
```


## Qualification and artifact identities

After a pilot has populated the run artifact cache, freeze and commit the harness, then run the installed-package adapter suites and retain their JSON receipts. Build one qualification receipt per backend with the exact package artifact manifest from that cache. These fault-suite runs may use a different browser engine from timing trials, which the receipts report:

```sh
node integrations/sqlite-browser/test/run-installed.mjs
node integrations/fogvault-browser/test/run-installed.mjs

node benchmarks/browser/harness/qualify.mjs \
  --backend sqlite \
  --suite-receipt <sqlite-suite-receipt.json> \
  --package-manifest <run-artifact-cache>/package-artifact-manifest.json \
  --output target/browser-pseudonymization/qualification/sqlite.json

node benchmarks/browser/harness/qualify.mjs \
  --backend fogvault \
  --suite-receipt <fogvault-suite-receipt.json> \
  --package-manifest <run-artifact-cache>/package-artifact-manifest.json \
  --output target/browser-pseudonymization/qualification/fogvault.json
```

Use the receipt paths actually emitted by each installed-package suite. The qualification command records the clean source commit, package archive hashes, and suite receipt hashes; the matrix runner forwards corresponding evidence with `--sqlite-qualification` and `--fogvault-qualification`. Each staging cache retains `resolved-dependencies.json`, with every installed dependency version and content digest. Preserve package/dependency manifests, suite receipts, and qualification receipts with the run artifacts.

Prior failed pilot receipts must be retained with their failure reason. Do not replace them when rerunning with a fresh trial ID and profile.
