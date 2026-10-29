# Committed-value recall — proposed 0.5.0-beta.2 contract

This is the implementation review for [D5 / #141](https://github.com/DataFog/datafog-core/issues/141),
built on the merged [D4 / #156](https://github.com/DataFog/datafog-core/pull/156).
**This API is unreleased and requires review.** The method is implemented in this
candidate's Rust/WASM exports; it is absent from the published `0.5.0-beta.1`
package. [#158](https://github.com/DataFog/datafog-core/pull/158) is a wider type-only
contract proposal, not approval of matching rules, limits or later policy.

## Query and result

`workspace.recall(text)` reads committed values and returns
`{head, complete: true, findings}`. Each item contains:

```ts
{
  finding: {entityType, matchedText, range: {unit: "utf8_bytes", start, end},
            source: "recall", needsReview: true},
  byteRange: {start, end},
  codepointRange: {start, end},
  utf16Range: {start, end}
}
```

Ranges identify the exact original input, zero-based and end-exclusive.
`matchedText` is the source slice, never the workspace's private first spelling.
There is no confidence score, detector authenticity claim or permission to
disclose. An empty complete result means no eligible committed value matched;
it does not establish that the text contains no PII. Limits fail the entire query
with `resource_limit`; there is no successful partial result.

Recall allocates no aliases or plans, changes no counters/head/candidate bytes,
and does not expose unactivated/discarded entries. The head is a by-value
observation, not a lock or reservation. Closing the workspace rejects later
queries with `closed_workspace`. Rust uses `Workspace::recall`, `RecallResult`
and `RecallFinding`; JS declarations match this candidate's actual exports.

```js
const recalled = workspace.recall(source);
const plan = workspace.prepare(source, recalled.findings.map(span => span.finding), randomOperationId());
const preview = plan.preview();
// Ask the person to approve this exact preview.
// Encrypt/export, atomically save against the complete expected head, confirm it,
// then activateSaved before copying/inserting preview.text.
```

The existing [beta.1 lifecycle](extension-integration-0.5.0-beta.1.md) still applies,
including fresh random 16-byte operation IDs, save failure/uncertainty, complete
head comparison, stale-plan reclamation, strict restore and the 32-live-plan
limit. Existing aliases restore to their first committed spelling. Run recall
separately for each batch input; feed each result into that input's findings.
`prepareBatch` still enforces aggregate source/finding bounds and input-local
coordinates. No `recallBatch` method is claimed.

## Proposed matching decisions

- Use the existing exact identity rule: Unicode character lowercase, Unicode
  whitespace runs collapsed to ASCII space, with surrounding whitespace trimmed.
  No NFC, accent stripping, full case-folding, fuzzy matching or punctuation
  normalization. `ß` does not match `ss`; `İ` maps to `i` plus combining dot.
- Return only complete original scalar boundaries. A stored `i` or combining dot
  does not match half of the lowercase expansion of `İ`. Combining accents remain
  significant. Internal source whitespace is included in the match; surrounding
  whitespace is excluded.
- If a key begins/ends in a word character, require a boundary at that original
  edge. Word characters are Unicode Alphabetic, Mark, Decimal_Number,
  Connector_Punctuation, U+200C and U+200D. Thus `Ann` does not match `Annette`,
  `_Ann`, `Ann中`, or the prefix before an adjacent combining mark/joiner.
  Punctuation within or at the edge of a stored value stays significant.
  This is category-based matching, not locale-specific word segmentation.
- Return all overlapping values and types, sorted by original byte start, end,
  then entity type. Do not apply source priority, confidence ranking or correction
  precedence. Feed candidates into preparation's existing selection; the stronger
  protected-coverage/combined-review rules belong to #145/#146 and need review.
  In particular, partially crossing candidates must not be treated as proof that
  legacy selection hides their full union. The combined-review launch flow needs
  #146's reviewed conflict/coverage handling before that guarantee can be made.
- Skip complete committed markers and never match across them. Use the same
  source preflight as preparation for unknown/damaged/unbracketed committed
  markers: `invalid_marker` with content-free byte/codepoint/UTF-16 issue ranges.

These choices are deliberately visible for maintainer/product review. They do not
resolve always-hidden versus not-sensitive policy, manual correction precedence
or remembered choices. This slice recalls all currently committed identities;
#145 must apply its reviewed eligibility rules later.

## Provisional bounds and implementation tradeoffs

| Bound | Candidate behavior |
|---|---|
| Original source | 1 MiB UTF-8, matching existing preparation |
| Stored entries | Existing 20,000-entry snapshot ceiling |
| Results | At most 10,000; exceeding it rejects, without truncation |
| Normalized source | 4 MiB across all marker-separated segments |
| Normalized keys | 16 MiB total per query |
| Prefix index | At most 262,144 nodes, including root |
| Matching work | 32 million charged units per query |
| Result transport | 16 MiB conservative budget including JSON escape/range overhead |

The temporary shared prefix index is built from committed normalized keys on
every query. Work charges key/normalized-source UTF-8 bytes, attempted trie
transitions, candidate visits and copied match bytes. Validation, regex category
tests, bounded range conversion and sorted output are additional bounded work;
this is not an instruction, memory-allocation or wall-clock quota. Source and
entry/result/node ceilings bound those other phases. No background cache or saved
index is added. Temporary matching state stays local to the call. Each private
index character has stable boxed storage, cleared in place before deallocation
on success and error exits. Map rebalancing moves only the owner, not that private
scalar allocation. This adds one small allocation per distinct trie edge; the
approved node/work/source limits are unchanged. Cleanup is best effort, not a
memory-forensics guarantee; caller-owned input and returned findings retain their
normal lifetimes.

The node cap is additional to #158's initial proposal: it limits index memory for
long unique keys. Consequently some otherwise valid large workspace snapshots
can explicitly exceed recall's index/key budget even on a short no-hit input.
Empty/plain whitespace queries and empty workspaces need no index. Repetitive
prefix inputs may hit the work budget despite being below the source limit.
**Review these usability/limit tradeoffs before release.** The measured browser
cases are observations, not proof of all 20,000-entry states, low-memory devices
or a latency guarantee. Adjusting limits requires new measurements, not a silent
partial-match fallback.

No snapshot schema/backup format changes occur here. Existing encrypted backups
retain the entries recall reads. Source/review metadata remains transient, as in
D4. This draft does not freeze the API or claim the full G2/B2 qualification gates.

## Verification evidence

Core tests cover committed-only visibility, discarded/pending values, unchanged
heads/bundles/plans, first-spelling alias reuse, overlapping values/types,
Unicode lowercase expansion, combining marks, whitespace/punctuation, all three
coordinate units, marker preflight and source/result/index/work limit failures.
Installed WASM coverage repeats transport/typing, batch-local ranges, lifecycle
and malformed-input cases.

The actual packed-SDK MV3 model proof commits GLiNER2.5 Small findings, restarts
the browser, recalls them without a live model, and goes through encrypted save
and approval before handoff. The backup suite additionally checks recall after
password/recovery-code import, fresh recovery keys and browser restart. Real
MV3 measurements retain three samples each for a 1,000-value dictionary, repeated
matches, maximum no-hit source, and explicit result/index/work exhaustion.
The proof report records exact source/tarball/model identities and samples under
`examples/workspace-extension/output/playwright/<run>/report.json`.

The [retained observation fixture](../fixtures/workspace/recall-browser-benchmark.json)
records three calls per case on Apple M5 Pro / 48 GiB with bundled Chromium
151.0.7922.34 after stable zeroizing index storage was added. Median observations
were 3.6 ms for 1,000 values/130,500-byte no-hit input,
2.1 ms for 500 matches, 13.2 ms for a 1 MiB no-hit source,
131.7 ms to reject the repetitive-prefix work case, and
7.6 ms to reject the index-node case. Calls block their calling
page while running; these observations are not a latency or low-memory guarantee.
The fixture pins the clean measured commit plus exact production-file and tarball
hashes. Subsequent candidate reports retain their own exact source identity.
