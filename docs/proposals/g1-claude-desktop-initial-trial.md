# Initial Claude Desktop marker pilot — proposal only

State: **unapproved, zero provider runs, no requests sent**. This draft is
for technical and independent review before Sid receives one trial-scope decision. The existing
15 offline Core tests establish the mechanism and do not authorize sending.

## Recommended single decision

Authorize **one Claude Desktop pilot: five fresh normal chats, the five fixed
prompt files once each, one response per task**, using the already-visible exact
selector **“Model: Fable 5.1 High”**, with no signup, upgrade, paid API usage or
additional spending. Use the existing subscription only; stop at account,
allowance, capability, tools or model-identity limits. This is initial diagnostic
trial scope, not G1 qualification. No send is authorized by this draft.

The access witness is [the access witness](evidence/g1-desktop-access-observation.json). CUA observed the running app as
Claude, bundle `com.anthropic.claudefordesktop`; its embedded UI path surfaces
`/Applications/Claude.app`. Its app version was not exposed. An authenticated
composer and the exact selector were visible, but account allowance, tool state
and underlying model ID/version were **not verified**. Treat “Fable 5.1 High” as an
opaque visible product label, not an inferred API model/version or provider backend.
The composer menu showed Research value 0, “Connectors 1” and “Auto.” This is not
proof that all tools are disabled or that a fresh chat inherits safe defaults.
No settings were changed. This execution prerequisite remains unresolved.
Chrome inventory exposed no ChatGPT or Gemini provider tab. The app named ChatGPT
with bundle `com.openai.codex` is Codex, not evidence of a ChatGPT trial surface.
Private chats, URLs, account details and credentials are excluded from the packet.

## Frozen inputs and five-attempt boundary

[the trial plan](evidence/g1-desktop-trial-plan.json) pins the existing #198 source e0c5b647, original manifest,
approved scorecard and exact five prompt hashes. Order: summarize, reply, table,
rewrite, followup. Every prompt contains all 50 issued synthetic markers; required
references per task are 50/30/50/20/20. The approved 170-reference initial scorecard
has no provider/model/repetition/aggregation approval attached.

After separate authorization, use one fresh non-project normal chat per prompt.
Check the displayed model label and High setting before every send. Stop the trial
if the label differs or is hidden; do not choose an alternative, infer a backend,
or change effort/model automatically. Check that tools, web search/connectors and
project instructions cannot run in the trial. If their absence cannot be verified
through the normal UI, stop for capability review. Do not alter the fixed prompt,
turn the existing private chat into a fixture, or upload local files.

Send only each `prompts/<task>.txt` content. Never send the gold bundle, synthetic
originals, IDs, manifest, local capture metadata or repository. Maximum **five Send
attempts total**, including any attempt whose submission acknowledgment is uncertain.
No edit, Retry/Regenerate, Continue, second submission, automatic resend or correction
message. A completed, refused, erroneous, truncated or missing response is that task's
single outcome, not a reason to replace the trial. If the send or identity becomes
uncertain, retain evidence and stop before the next task.

Bound observation to 180 seconds after Send per task. If generation is still active,
use the normal Stop control once, retain the partial result and mark it truncated.
If Stop or completion cannot be established, record incomplete/uncertain and stop
the pilot. No automatic continuation or retry. Retain refusal/error UI and the
unchanged partial or full response; never drop a failed task from the five-task list.

## Capture and missing identity treatment

The historical capture schema/validator accepts only `executionSurface: web|api`;
the available surface is desktop. This PR adds a separate offline collector,
`scripts.provider_markers.capture`, and `capture-with-desktop.schema.json`.
Web/API records delegate to the unchanged historical validator and retain exact
output. Desktop records retain their real surface, use the same closed fields and
null/unavailable accounting, and currently accept only Claude. This is local file
validation, without a provider client or dispatch. The original preparation module,
manifest and gold remain byte-exact. Root and independent review must close before
any execution question. App identity/version belongs in a local run witness;
do not invent extra metadata fields accepted by the closed capture schema.

For each later actual response, retain raw UTF-8 as supplied by the UI's copy/export
facility before any normalization, and hash those bytes. Record UTC observation,
provider `claude`, surface `desktop`, exact displayed model/setting, prompt ID/hash,
original manifest hash and an ordinal attempt ID in a separate run index. Preserve
failed attempts and raw response files. Capture metadata's `recordKind` must be
`provider_capture` for genuine runs; local simulations remain `protocol_test`.

Use `modelLabel: "Fable 5.1 High"` only if that exact combined label is still shown
for the fresh trial. `reportedModelId`, `modelVersion` and `providerRequestId` stay
null unless the UI actually exposes them; list each null field in
`unavailableMetadata`. Do the same for unexposed temperature/topP/seed/output limit,
input/output tokens, tokenizer and cost. High is a visible setting, not a fabricated
temperature or known model revision. Prompt byte counts are not token counts.
`capturedAt` and `responseTruncated` must be actual observations, never template
values. Keep credentials, account data, headers and private chats out of metadata.
If the UI cannot provide unchanged response text, retain the observable failure
and stop rather than retyping or pretending screenshots are raw model bytes.

## Results and scope exclusions

Run the approved local scorer per captured task. Keep required omissions,
repetitions and strict marker errors/ranges. Whole-response strict failure yields
zero successful-restoration credit for that task even if known references were
observed. Optional returned markers must restore. Retain raw failures and any
truncation; do not edit or discard them. Produce per-task facts only: no pooled
rate, model/provider pass, >=99% decision or aggregate conclusion.

The existing followup file is **one user message containing an inline fictional
prior-exchange description**. Its response tests that fixed prompt only. A genuine
multi-turn test requires an actual first send/response, then a later follow-up in
the same new conversation, plus reviewed turn contents and a larger request budget.
That sequence is absent and remains separately unapproved; do not silently turn
this five-attempt pilot into it.

ChatGPT, Gemini, alternative models, repetitions, aggregation, real multi-turn
coverage and broader G1 acceptance need separate concrete scopes/access evidence.
This pilot cannot close #150 or freeze G1. Current state remains **0 provider runs,
null preservation rate, g1Pass false**. This draft and its collector are proposed
for root and independent technical review; no provider execution is authorized.

## Offline collector verification

Use the existing pinned native wheel and run the offline suites:

```sh
python bindings/python/tests/provider_marker_conformance.py
python bindings/python/tests/provider_desktop_capture_conformance.py
```

The second suite exercises historical web/API output parity, real desktop surface,
explicit unavailable identities, closed metadata rejection, valid public settings,
Core issue ranges, unchanged raw CRLF/UTF-8, truncation and invalid UTF-8. Both
are wired into installed Python smoke; neither sends a request. For later
**separately authorized and captured** local files:

```sh
python -m scripts.provider_markers.capture fixtures/provider-markers \
  --metadata target/g1/metadata.json --response target/g1/response.txt
```

The collector pins its own hash on desktop output. That is a local collector
witness, not a signer identity, provider model revision or successful-send proof.
Actual UI application version was not observed and remains unknown.
