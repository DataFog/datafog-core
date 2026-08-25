# DataFog Rust POC

## Objective

Evaluate whether a Rust implementation of `scan(text, engine="regex")` should replace DataFog's existing fast-install Python core.

## Scope

PII fields: `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, and `ZIP_CODE`.

Exclude spaCy, GLiNER, `smart`, and all NER model download/loading time.

## Baseline

- Repository: `datafog/datafog-python`
- Version: `4.8.0a6`
- Commit: `75e414b2`
- Invocation: `scan(text, engine="regex")`
- Fields: `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DATE`, `ZIP_CODE`

## Measurements

- Precision, recall, and F1 overall and by PII field
- Output-difference rate: Rust `scan` vs pinned Python baseline
- Total runtime, p50/p95 latency, and sentences/second
- Startup time
- Peak memory use

## Results

### Final 1,000-sentence comparison

![Final 1,000-sentence results](docs/images/1000-sentences.png)

### Throughput scaling

![Throughput scaling results](docs/images/scaling-view.png)

## Local tools

### Results Viewer

1. Run a comparison: `python3 scripts/compare.py fixtures/final.jsonl`.
2. Or run batch scaling: `python3 scripts/compare.py scale fixtures/development.jsonl fixtures/final.jsonl`.
3. Open `results-viewer.html` in a browser and select the timestamped JSON report from `results/`.

![Results Viewer](docs/images/results-viewer.png)

### Data Doctor

1. Open `data-doctor.html` in a browser and select a fixture JSONL file.
2. Review one sentence at a time; mark it correct, flag it, or add/change/remove labels.
3. Download the updated fixture, inspect its Git diff, then replace the source fixture intentionally.

![Data Doctor](docs/images/data-doctor.png)

#### Fixture JSONL schema

Data Doctor expects one JSON object per line. Each record requires `id`, `text`, and `entities`; `category` is optional.

```json
{"id":"case-001","text":"Email jane@example.com","entities":[{"label":"EMAIL","text":"jane@example.com","start":6,"end":22}]}
```

Each entity uses a supported label, the exact matched `text`, and zero-based Unicode code-point offsets with an exclusive `end`.

## Evaluation Data

- 100 sentences for development/regression
- Frozen 1,000 sentences for final evaluation

## Out of Scope

- Production migration or other code changes
