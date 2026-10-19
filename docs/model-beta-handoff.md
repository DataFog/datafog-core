# Model beta handoff

Target: `0.4.5-beta.1` for Cargo/npm and `0.4.5b1` for Python. The demo application
is owned separately; it consumes the Core session API described in
[model sessions](model-sessions.md) and [GLiNER2 setup](gliner2-model.md).

## Consumer contract

Provision local assets explicitly, retain their licenses, and keep the expected
manifest digest independently. Open one `ModelSession`, warm it, reuse it, then
close it on shutdown. Choose `model_only` or `combined` explicitly. Feed ordinary
findings to the existing text/structured transforms. Python releases the GIL;
Node performs blocking work off the event loop.

One session accepts one request at a time with no queue. Use separate sessions
only when accounting for their additional memory. Input/output/finding budgets
and deadlines are caller configuration. `ModelCancellation` is one-shot. A
cancelled/expired queued request never starts inference; cancellation/failure
after inference begins retires the worker. Reopen after failure. Do not treat a
model failure as successful rule-only protection.

The demo agent owns application UI, request routing, CSV parsing and downstream
MCP integration. Core owns detection, canonical offsets, provenance, local worker
lifecycle, composition and transformation contracts. Structured scans process
decoded string leaves separately. Existing discovery exclusions do not exclude
model inference. GLiNER2 scores are uncalibrated evidence; DataFog confidence uses
the published calibration. Preserve first/last-name distinctions.

## Release checklist

- Pass the complete two-backend/five-native-platform qualification against the
  intended release source, plus all existing supported-language binding checks.
- Qualify packaged Rust source and installed Python/Node artifacts; retain the
  exact artifacts, source identity, dependency versions, checksums and reports.
- Finish review of ADR #57, initial integration #59, lifecycle #60 and GLiNER2 #61.
- Run the existing package-specific publication jobs from that qualified source.
  Each job rejects publication without complete qualification for its exact SHA.
- Publish the optional `datafog-gliner2-worker` Python distributions alongside
  Core. Configure its PyPI trusted publisher for this repository/workflow and
  `pypi` environment; the existing Core project's publisher does not automatically
  grant a new project publishing rights.
- Use npm's explicit `beta` tag for the root and every native platform package;
  preserve the stable `latest` tag. Create `v0.4.5-beta.1` as a GitHub prerelease.
- Verify fresh registry installation, model loading, scanning, transformation and
  cleanup before calling the release delivered. Record any partial publication
  and resume only missing artifacts; never replace already published versions.

## Measured evidence

Qualification reports include synthetic development and held-out denominators,
exact-span misses/false positives, model-only/combined p50/p95 and throughput,
cold loading/first inference, sampled parent+worker RSS and effective settings.
Broader overlapping address spans count as additional predictions under the
exact-span fixture metric; these reports are not population accuracy estimates.
No performance threshold or universal resource default is inferred from a single
host. The full native matrix remains mandatory; no platform is deferred.
