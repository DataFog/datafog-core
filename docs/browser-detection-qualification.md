# Browser detection qualification preparation

This is preparatory evidence for [G2 / #151](https://github.com/DataFog/datafog-core/issues/151),
targeting `0.5.0-beta.3`. **G2 has not passed.** The feature dependencies #141, #144, #145 and #146 have landed. The current
run exercises encrypted approval/save, committed-value recall, restart and both
backup credentials. An agreed pilot corpus and review of the stated launch bar
still remain.

The baseline runs the current `@datafog/wasm` candidate npm tarball installed into
a consumer, bundled into an actual MV3 Chromium extension, with the pinned English
GLiNER2.5 Small ONNX export. It invokes the real `ModelSession.scan(text)` with
its default rule/model routing. It is not a native GLiNER2 benchmark or a smoke
claim that the model catches everything. No paid provider or real user PII is used.

## Corpus and scoring contract

[`qualification-v1.json`](../fixtures/browser-detection/qualification-v1.json)
has 64 synthetic pasted-text records: 28 PERSON, 13 full ADDRESS and 11 structured
identifier gold spans, plus hard negatives. It covers Unicode/combining prefixes,
repeated values, names in code, compound/inverted names, international address
shapes in English contexts, words/substrings, marker text and the six #85
contraction scenarios. It is a development set informed by known examples, not a
held-out or statistically representative pilot. The independent raw detection measurement does not treat manual review or recall
as detection successes. A separate workflow now explicitly approves these
synthetic gold values and tests recall, reopen and backup recovery.

Gold spans use exact source bytes. Complete annotations are supplied for the
explicit `scoredTypes`; other returned entity types remain visible in observations
but are not scored against an incompletely annotated corpus. All predictions must
have valid, source-matching UTF-8, codepoint and UTF-16 ranges before scoring.

- **Exact:** same entity type and both byte boundaries. A partial name is a miss
  plus a false positive, even if it overlaps the intended value.
- **Overlap (lenient):** same type and positive byte overlap, using maximum
  one-to-one matching. One large prediction cannot recall multiple gold values;
  duplicate predictions remain false positives. This is a boundary diagnostic,
  not proof that the complete PII span was protected.
- **ADDRESS:** the browser's `address` prompt returns canonical full-span ADDRESS.
  Gold includes the written city/postcode/country when part of the delivery
  address. A separate street-line gold view includes the number/street/unit only.
  The street diagnostic compares untrimmed ADDRESS predictions with street-line
  spans; it does not change shipping output into STREET_ADDRESS. Its exact score
  must not be presented as the shipping address recall.

The fixed session uses threshold **0.5**, one runtime thread and the existing
mapping: person→PERSON, address→ADDRESS, email→EMAIL, phone number→PHONE,
identification number→IDENTIFICATION_NUMBER, url→URL and username→USERNAME.
No prompts, thresholds, labels or boundaries were tuned to the measured results.
The metadata/inputs were pinned before the committed-head baseline run.

## Historical initial result

Chromium 151.0.7922.34 on macOS arm64 / Apple M5 Pro, model revision
`5e2e3f51adfb0eeb7c1f83464400b4d498d41659`, ONNX Runtime Web 1.31.1,
`@huggingface/tokenizers` 0.2.0; the candidate package version is `0.5.0-beta.1`.
Model/tokenizer/runtime/WASM hashes, package hash, prompt, source commit, input
hashes and browser identity are pinned in the receipt. This is the present
baseline, not a released `beta.2` or `beta.3` result.

| Type | Exact TP / gold | Exact precision | Exact recall | Overlap recall |
| --- | ---: | ---: | ---: | ---: |
| PERSON | 26 / 28 | 78.8% | 92.9% | 100% |
| ADDRESS, full span | 8 / 13 | 34.8% | 61.5% | 69.2% |
| EMAIL | 3 / 3 | 100% | 100% | 100% |
| PHONE | 2 / 2 | 100% | 100% | 100% |
| CREDIT_CARD | 2 / 2 | 100% | 100% | 100% |
| SSN | 1 / 1 | 100% | 100% | 100% |
| US_ROUTING_NUMBER | 1 / 1 | 100% | 100% | 100% |
| DE_IBAN | 0 / 1 | undefined | 0% | 0% |
| ACCOUNT_NUMBER | 0 / 1 | undefined | 0% | 0% |

The street-line diagnostic is 0/13 exact and 9/13 overlap. The model supplies
full-address spans, so this does not mean that it found zero streets.
The small structured-identifier samples are insufficient to establish a no-miss
release bar. DE_IBAN is absent under this default English routing/configuration;
generic ACCOUNT_NUMBER is absent from the default Small prompt mapping. Native
catalog/custom-label support for account numbers does not qualify this default
browser workflow. A routing number is not the customer's account number.

The two exact PERSON misses are `Müller, Hans` (two separate predictions) and
`WONG, DAVID MING-HO` (only the given-name part). ADDRESS misses include Infinite
Loop, multiline Downing Street, Friedrichstraße, the long Martin Luther King
street and a truncated Cedar Road/Oakville address. False positives include
`Person` in a class declaration, `patricia` inside an account handle, `Wall Street`
as a person, repeated Jane Doe values/emails/code fields as addresses, and
`Cedar Labs` as an address. The raw receipt retains every finding and detector
source/score; none are dropped to improve these figures.

The six #85-shaped cases do not produce PERSON `I` with these browser defaults;
`Morgan` is found with either apostrophe. The native issue uses a different
model and custom descriptions. This observation neither closes #85 nor verifies
that custom configuration in Small.

## PR #115 review

[`pr115-review.json`](../fixtures/browser-detection/pr115-review.json) pins the
reviewed head/blob/content hash: 85 records, 106 entities, all codepoint offsets
valid at that head. Several STREET_ADDRESS labels include city/postcode/country,
inconsistent with the stated street-line convention. USERNAME-versus-PERSON and
family-name negatives also require policy review. We reused selected scenario
shapes, with separately pinned complete gold for this baseline; no native scores,
threshold tuning, evaluator implementation or baseline was imported. We did not
merge #115.

## Reproduce and inspect

After doctor-first setup with the repository's pinned tools:

```sh
npm ci --prefix bindings/wasm
npm ci --prefix examples/workspace-extension
cargo dfv build
cargo dfv doctor
node --test scripts/test-browser-detection-scoring.mjs
DATAFOG_BROWSER_MODEL_DIR=/absolute/path/to/verified/model-cache \
  node scripts/qualify-browser-detection.mjs
```

The model directory must contain the three files from `bindings/wasm/models/model.mjs`.
The existing build verifies every size/SHA-256 before packing. If not cached, use
the documented pinned provisioning workflow; no model is implicitly downloaded
by this runner. The corpus and adapter configuration are fixed in the fixture.

Each run writes `target/browser-detection/<timestamp>/receipt.json` (complete
findings, sources, scores and all misses/false positives), plus compact
`summary.json` and `observations.jsonl`. The committed receipt is
[`browser-detection-small-preparation.json`](../baselines/browser-detection-small-preparation.json)
and its paired [per-record observations](../baselines/browser-detection-small-preparation.jsonl).
The scorer tests recompute its exact/overlap totals and verify the corpus/scorer
hashes and observation digest. The other hashes describe the recorded historical
package/runtime run; changing a future dependency does not rewrite that history.

The runner fails on malformed spans, inference/setup errors or external requests.
Accuracy remains reported evidence, not a new blocking community-model CI policy.
The independently reviewed launch pilot gate must still meet its own agreed bar.

## Current merged-feature result

The new current receipt preserves the exact same 64 records, gold bytes, scorer,
Small prompt/threshold, model export and source attribution. It runs a locally
packed current-main candidate, whose package version is `0.5.0-beta.2`, rather
than substituting native GLiNER2 scores. The exact source, package and runtime
identities are retained. This is not a claim that the published beta.2 package
contains later beta.3 APIs.

Name and full ADDRESS scores remain 26/28 and 8/13 exact, respectively. Email
(3/3), phone (2/2), cards (2/2), SSN (1/1) and routing number (1/1) have no misses
on these small synthetic samples. The explicitly labelled 12-digit account
example now passes (1/1) through `datafog-core/account-number`; this improvement
belongs to the merged rule, not model accuracy. Its historical `account-number-gap`
record ID/provenance stays pinned for comparison. It does not test unlabelled
account numbers. The German IBAN example is still missed under English defaults.
Every original name/address miss and false positive remains visible.

The historical street diagnostic remains 0/13 exact and 9/13 overlap against
untrimmed full ADDRESS predictions. There are no direct STREET_ADDRESS findings
in these 64 records. Their address positives use `Delivery address:` or unlabelled
prose, rather than the approved rule's exact `street address:`, `shipping address:`
or `billing address:` labels. This corpus therefore does not qualify all of that
new rule's supported formats; its separately committed rule acceptance fixtures
retain that responsibility. No prediction is trimmed or relabelled to inflate
shipping ADDRESS accuracy.

## Separate committed-value workflow

The installed package is bundled into an actual MV3 extension with the existing
IndexedDB, vault and stronger-backup helpers. Actual scanner findings for the
fixed `person-01` record go through combined review, explicit rejection, a real
aborted save and explicit successful retry before activation and output. Pending
mappings are not recalled and failed saves release no text.

Separately, the script explicitly approves the 52 annotated gold occurrences in
47 positive records as **manual fixture selections**. This tests values a person
has approved, including model misses; it does not count them as automatic
identification. All 52 are recalled with exact original source ranges and recall
metadata in each phase: saved state, browser-process restart without a model,
password backup import and restart, and recovery-code import and restart. All
47 pseudonymized records restore unchanged in every phase. Queries/restoration
preserve committed bytes; case/whitespace matching works and substring expansion
stays blocked. Both imports keep stronger profile 2, use fresh workspace encryption
keys/nonces and a fresh saved head, and preserve permanent aliases.

Receipts contain no password, recovery code or backup ciphertext. These temporary
synthetic credentials stay in the runner's memory. This is a synthetic host seam,
not Josh's production extension or a production security approval.

## Concrete review still required for G2

Recommend treating this fixed development set as regression evidence, then agreeing
on a separate held-out pilot before a launch go/no-go. The measured 92.9% name
recall exceeds 90% on this set, but the sample is small and already informed by
known failures. Structured types have only one to three examples each.

The account-number bar also needs explicit scope: approved bounded labelled
formats now have deterministic coverage; unlabelled/differently formatted account
numbers and international financial identifiers are not thereby qualified. The
remaining DE_IBAN miss makes that distinction concrete. Either agree that bounded
scope for the pilot or file/qualify the broader capability. Do not reinterpret a
manual approval or later recall as automatic first-pass detection, lower the bar,
or tune solely to these examples. **G2 remains open.**
