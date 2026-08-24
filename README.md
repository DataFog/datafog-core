# DataFog Rust POC

## Objective

Evaluate whether a Rust implementation of `scan(text, engine="regex")` should replace DataFog's existing fast-install Python core.

## Scope

PII fields: `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DOB`, and `ZIP`.

Exclude spaCy, GLiNER, `smart`, and all NER model download/loading time.

## Baseline

- Repository: `datafog/datafog-python`
- Version: `4.8.0a6`
- Commit: `75e414b2`
- Invocation: `scan(text, engine="regex")`
- Fields: `EMAIL`, `PHONE`, `SSN`, `CREDIT_CARD`, `IP_ADDRESS`, `DOB`, `ZIP`

### Canonical output labels

The evaluation vocabulary uses `DOB` and `ZIP`. Normalize baseline Python
output before scoring:

- `DATE` → `DOB`
- `ZIP_CODE` → `ZIP`

Compare canonical `(label, text, start, end)` tuples. Preserve raw baseline
output alongside normalized output.

## Measurements

- Precision, recall, and F1 overall and by PII field
- Output-difference rate: Rust `scan` vs pinned Python baseline
- Total runtime, p50/p95 latency, and sentences/second
- Startup time
- Peak memory use

## Evaluation Data

- 100 sentences for development/regression
- Frozen 1,000 sentences for final evaluation

## Out of Scope

- Production migration or other code changes
