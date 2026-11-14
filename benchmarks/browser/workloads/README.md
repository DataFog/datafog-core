# Synthetic workloads

`generate.py` emits deterministic JSON inputs for a single scenario. It does not run a backend or claim the scenario was admitted. Each generated fixture carries a SHA-256 digest over canonical UTF-8 JSON for the entire payload, excluding only the digest field itself. Fixture inputs use UTF-8 byte offsets, matching Core's `WorkspaceFinding` contract. Random public operation IDs are intentionally left to the browser harness, which must use the browser CSPRNG; IDs are not fixture content.

Generate the complete catalog:

```sh
python3 benchmarks/browser/workloads/generate.py --catalog --output target/browser-benchmark/scenarios.json
```

Generate one fixture for the runner:

```sh
python3 benchmarks/browser/workloads/generate.py \
  --mode storage_workspace --seed 1 --target 100 --history fresh \
  --reuse mostly_new --shape unicode \
  --output target/browser-benchmark/fixture.json
```

The catalog covers both measurement modes, all four mapping targets, fresh and long-lived history, repeated-existing and mostly-new values, and all five input shapes. Long-lived history is an executable recipe in `expected.history.recipe`, not an assertion that the history has already been created. The harness must execute and validate every step, record any failure, and include setup's full-bundle persistence costs in its setup accounting. Each phase's findings/source/batch are bounded separately according to Core limits. The generator records the expected active mapping count at the beginning and end of the timed trial.

The payload includes exact expected source texts for restore checks. The harness must compare restored bytes exactly before and after full browser restart; a fixture generated or a restore attempted is not itself a correctness pass.

Run generator tests with:

```sh
python3 -m unittest discover -s benchmarks/browser/workloads -p 'test_*.py'
```
