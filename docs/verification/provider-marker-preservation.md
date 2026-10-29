# G1 provider marker preservation: preparation and capture draft

Issue [#150](https://github.com/DataFog/datafog-core/issues/150) requires real
ChatGPT, Claude and Gemini results before launch qualification. This packet is
**offline preparation only**: zero provider runs, no preservation rate and no G1
pass. It neither contacts providers nor contains a scorer. The capture protocol,
provider/model selection, repetitions and denominator need maintainer review
before runs begin. No paid calls or provider credentials are needed to verify it.

## Synthetic fixture and sending boundary

`fixtures/provider-markers/manifest.json` pins the generation source commit,
Core/Python Git trees, Cargo.lock, generator, actual installed wheel and compiled
extension hashes. Core generates and commits 50 distinct aliases in one workspace
using fixed synthetic IDs: five each for PERSON, ADDRESS, EMAIL, PHONE,
IDENTIFICATION_NUMBER, URL, USERNAME, ACCOUNT_NUMBER, STREET_ADDRESS and SSN.
The first seven are Small's default labels; all ten are current protected types.
This selection is a proposed transport corpus, not evidence of detection accuracy
or complete model/provider/type coverage. Values explicitly say SYNTHETIC or use
reserved `.invalid` domains; the labelled findings are supplied directly to Core.

Only the **five files in `prompts/`** are provider input. Each includes all 50
issued markers, fictional case IDs and task instructions. Core audits each prompt
for exactly 50 restorable markers. The generator also rejects the synthetic
original values, namespace, operation IDs or exported bundle in those prompts.

`gold/workspace.json` contains synthetic originals, IDs and the committed workspace
bundle for local strict restoration. **Do not upload the gold directory, manifest,
local metadata or repository bundle to a provider.** No real PII or personal chat
history is required. Keep any subsequent provider trial in a fresh synthetic-only
conversation and record the actual model/surface shown; this draft has not chosen
models or authorized execution.

## Candidate map11 regeneration

The [offline generation comparison](evidence/provider-marker-v3-preparation.json)
records the freshly built and installed native wheel used for the current gold.
The private bundle changes from policy-v2/map8 (3,430 bytes) to candidate-v3/map11
(4,571 bytes). Fields 1–7, all private fixture IDs and all 50 original/marker
mappings remain exact; the version changes and fields 8/9/10 are added. The new
history records creation plus 50 legacy prepares with unknown times. Key9 contains
four empty arrays and `confirmedHandoffs` remains zero: generating this transport
corpus does not assert an observed external send or qualify conversation binding.

`markers.json`, all five prompt files, the task/reference expectations and the
preparation report are byte-for-byte unchanged. The manifest updates only the
gold hash and actual generation source/wheel/native witnesses. The proposed
170-reference denominator remains unapproved. Seven actual installed-wheel tests
pass against this packet; provider runs remain zero, preservation rate remains
null and G1 pass remains false. Final candidate integration and actual provider
execution/review are separate gates.

## Proposed reference and omission denominator

Each task has a fixed reference set, recorded as marker IDs in the manifest.
Case-to-field associations and the unedited marker tokens are the local reference;
there is no single gold natural-language response.

| Task | Required cases | Distinct required marker references | Permitted omitted references |
| --- | --- | ---: | ---: |
| Summarize | C01–C05 | 50 | 0 |
| Reply | C01, C03, C05 | 30 | 20 |
| Table | C01–C05 | 50 | 0 |
| Rewrite | C02, C04 | 20 | 30 |
| Follow-up | C01, C05 | 20 | 30 |

The draft denominator is **170 distinct required references per provider/model
repetition**. Required omissions remain failures rather than disappearing from the
denominator. A repeated occurrence cannot compensate for a different missing
required marker. References from the permitted-omission set do not enlarge that
170-slot denominator. They still must restore if returned, and damaged, unknown,
foreign, unissued or wrong-label marker candidates remain strict errors. Provider
omissions and malformed outputs must be retained for review, not silently dropped
as failed trials. Invented ordinary prose cannot be inferred to be a marker.

These are proposed counting rules, not implemented qualification arithmetic. The
issue's >=99% target needs a reviewed aggregation rule: pooling providers can hide
one failing provider, and the number of repetitions affects what 99% means. Review
per-provider/model reporting, repeated occurrences, retries, truncation, provider
errors and task fidelity before writing the scorer. The follow-up prompt contains
an **inline synthetic prior-exchange description in one user message**; it does
not prove preservation across a genuine multi-turn provider conversation. Decide
whether to replace or supplement it with a recorded multi-turn sequence.

## Reproducing the offline packet

Use repository pins and an actual installed wheel built from the checkout:

```sh
cargo dfv doctor
cargo dfv build --sdk python
# Install the selected target/wheels wheel into an isolated environment.
python -m scripts.provider_markers.preparation verify fixtures/provider-markers
python bindings/python/tests/provider_marker_conformance.py
```

The verifier compares the installed native extension with the selected wheel and
rejects dirty Core/Python runtime source. It reissues the deterministic markers
through Core, opens the stored bundle, restores every mapping, checks all file
hashes and exact prompt/reference sets, audits the sending boundary and enforces
zero-provider/no-pass preparation state. The generator hash must match the packet.
Generation and verification witnesses are separate: a subsequent OS/wheel/commit
can verify the same fixture without claiming it created the original packet.
The committed generation witness is one macOS ARM native build, not an all-platform
binary qualification claim.

To propose a changed packet, generate into a new explicit directory and review
the diff; `prepare` refuses to replace different existing artifacts:

```sh
python -m scripts.provider_markers.preparation prepare target/provider-marker-proposal
```

## Local response capture proposal

The machine-readable metadata format is
[`capture-metadata.schema.json`](../../scripts/provider_markers/capture-metadata.schema.json).
All fields are required. Record provider, actual displayed model label, available
model ID/version, web/API surface, UTC timestamp, prompt/manifest hashes,
truncation, sampling settings, tokenizer identity, observed token usage/cost and
provider request ID. If the surface does not expose an observation, use `null` and
list that exact field in `unavailableMetadata`; do not estimate it or claim an
unavailable model revision. Prompt byte/codepoint/UTF-16 sizes are measured in the
manifest and are **not token counts**. Usage/configuration values are recorded
observations, not a chosen sampling policy. No credential/header fields are allowed.

After separately authorized execution, save the unchanged UTF-8 response and local
metadata, then capture without any network access:

```sh
python -m scripts.provider_markers.preparation capture fixtures/provider-markers \
  --metadata target/g1/metadata.json --response target/g1/response.txt \
  > target/g1/captured.json
```

Capture preserves response bytes (including CRLF), hashes and sizes, original
public metadata, current Core witness and strict Core issue ranges. It does not
save restored originals. `recordKind: protocol_test` distinguishes synthetic local
validator tests from `provider_capture`; neither kind is automatically qualified.
A response with no markers may be ordinary valid text to Core while failing the
provider task. Therefore even a fully restorable or empty-marker response leaves
`qualification.g1Pass` false and `preservationRate` null pending human review and
a reviewed scorer. Keep raw provider failures, edits/retries and truncation visible;
never substitute these local tests for genuine provider captures.

The installed Python smoke suite exercises the offline packet, all 50 strict
restores, permitted case/Crockford tolerance, damaged/wrong-label/foreign/unissued
rejection, atomic no-partial restore, Unicode issue offsets, tampering checks,
metadata validation and byte-preserving local capture. These tests establish the
preparation mechanism only. Provider/model choices, actual executions, denominator
approval, token/cost observations and result review remain open under #150.
