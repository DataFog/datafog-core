import {verifyWorkspaceReview} from "./workspace-review-conformance.mjs";
/** Shared installed-package workspace checks; all values are synthetic. */
export async function verifyWorkspace(api, counters) {
  const check = (condition, label) => { if (!condition) throw new Error(label); };
  const throws = (callback, code) => {
    try { callback(); } catch (error) { check(error.code === code, `workspace error category: ${code}`); check(error.message === code, "content-free workspace error"); return; }
    throw new Error(`missing workspace rejection: ${code}`);
  };
  const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
  const id = value => new Uint8Array(16).fill(value);
  const isMarker = (text, label) => new RegExp(`^\\[\\[${label}_[0-9A-HJKMNP-TV-Z]{6}\\]\\]$`).test(text ?? "");
  await api.init();
  const reviewMeasurements=verifyWorkspaceReview(api);
  const recallWorkspace = api.Workspace.create(id(21), id(22));
  const approvedValue = "İ Jane";
  const seed = recallWorkspace.prepare(approvedValue, [{entityType: "PERSON", matchedText: approvedValue, range: {unit: "utf8_bytes", start: 0, end: new TextEncoder().encode(approvedValue).length}}], id(23));
  check(recallWorkspace.recall(approvedValue).findings.length === 0, "recall never sees a pending candidate");
  const seedHeads = seed.heads();
  recallWorkspace.activateSaved(seed, seedHeads.expectedHead, seedHeads.candidateHead);
  const recallBundle = recallWorkspace.exportBundle(), recallHead = recallWorkspace.head();
  const recalledText = "👋 i\u0307\u00a0\tJANE";
  const recall = recallWorkspace.recall(recalledText);
  check(recall.complete === true && JSON.stringify(recall.head) === JSON.stringify(recallHead), "recall returns the unchanged committed head");
  check(recall.findings.length === 1 && recall.findings[0].finding.source === "recall" && recall.findings[0].finding.needsReview === true, "committed recall is labeled and requests review");
  const located = recall.findings[0];
  check(new TextDecoder().decode(new TextEncoder().encode(recalledText).slice(located.byteRange.start, located.byteRange.end)) === located.finding.matchedText && [...recalledText].slice(located.codepointRange.start, located.codepointRange.end).join("") === located.finding.matchedText && recalledText.slice(located.utf16Range.start, located.utf16Range.end) === located.finding.matchedText, "recalled Unicode ranges identify the original source in every unit");
  check(located.byteRange.start === 5 && located.codepointRange.start === 2 && located.utf16Range.start === 3, "recalled ranges include the original astral prefix");
  const followupPlan = recallWorkspace.prepare(recalledText, recall.findings.map(span => span.finding), id(24));
  check(recallWorkspace.restore(followupPlan.preview().text) === "👋 İ Jane", "recalled variants reuse the committed marker and first spelling");
  check(recallWorkspace.recall(followupPlan.preview().text).findings.length === 0, "committed markers are excluded from recall");
  const recallBatch = recallWorkspace.prepareBatch([{text: recalledText, findings: recall.findings.map(span => span.finding)}, {text: approvedValue, findings: recallWorkspace.recall(approvedValue).findings.map(span => span.finding)}], id(20));
  check(recallBatch.previews()[0].transformations[0].sourceByteRange.start === 5 && recallBatch.previews()[1].transformations[0].sourceByteRange.start === 0, "separate recall queries retain input-local ranges in batch preparation");
  throws(() => recallWorkspace.recall("👋 [[PERSON_BAD]]"), "invalid_marker");
  throws(() => recallWorkspace.recall("x".repeat(1024 * 1024 + 1)), "resource_limit");
  throws(() => recallWorkspace.recall("\ud800"), "invalid_finding");
  check(new Uint8Array(recallWorkspace.exportBundle()).every((value, index) => value === recallBundle[index]), "recall and unactivated follow-up preserve candidate bytes");
  const recalledReopen = api.Workspace.openBundle(recallBundle);
  check(recalledReopen.recall(recalledText).findings.length === 1, "recall survives bundle reopen");
  followupPlan.discard(); recallBatch.discard(); recalledReopen.close(); recallWorkspace.close();
  throws(() => recallWorkspace.recall(recalledText), "closed_workspace");
  const sources = api.Workspace.create(id(25), id(26));
  const sourceText = "👋 é Jane";
  const legacy = {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 9, end: 13}};
  const legacyPlan = sources.prepare(sourceText, [legacy], id(27));
  const legacySpan = legacyPlan.preview().transformations[0];
  check(legacySpan.source === "scanner" && legacySpan.needsReview === true, "omitted source metadata preserves conservative legacy defaults");
  const sourcePlan = sources.prepareBatch([
    {text: sourceText, findings: [{...legacy, source: "user", needsReview: false}]},
    {text: sourceText, findings: [{...legacy, source: "recall", needsReview: true}]},
  ], id(28));
  const sourcePreviews = sourcePlan.previews();
  check(sourcePreviews.map(p => p.transformations[0].source).join() === "user,recall", "batch previews retain input-local source metadata");
  check(sourcePreviews[0].transformations[0].needsReview === false && sourcePreviews[1].transformations[0].needsReview === true, "explicit review flags survive the Core binding");
  const sourceSpan = sourcePreviews[0].transformations[0];
  check(sourceSpan.sourceByteRange.start === 9 && sourceSpan.sourceCodepointRange.start === 5 && sourceSpan.sourceUtf16Range.start === 6, "review metadata preserves all Unicode range units");
  for (const metadata of [{source: "private-source"}, {source: null}, {source: 1}, {needsReview: null}, {needsReview: "false"}]) {
    throws(() => sources.prepare(sourceText, [{...legacy, ...metadata}], id(29)), "invalid_finding");
  }
  let sourceGetterRead = false;
  throws(() => sources.prepare(sourceText, [{...legacy, get source() { sourceGetterRead = true; return "user"; }}], id(29)), "invalid_finding");
  check(!sourceGetterRead, "source accessors are rejected without reading them");
  const preferred = sources.prepare(sourceText, [{...legacy, source: "scanner", confidence: 0.1}, {...legacy, source: "user", needsReview: false, confidence: 0.9}], id(29));
  check(preferred.preview().transformations[0].source === "user" && preferred.preview().transformations[0].needsReview === false, "metadata follows the selected duplicate rather than the first matching range");
  throws(() => sources.restore(sourcePreviews[0].text), "invalid_marker");
  sourcePlan.discard(); preferred.discard(); legacyPlan.discard(); sources.close();
  const blocked = api.Workspace.create(id(30), id(31));
  const blockedSaved = blocked.exportBundle();
  let sourceFailure;
  try { blocked.prepareBatch([{text: "Jane", findings: []}, {text: "👋 é [[my_note]] [[PERSON_ZZZZZZ]]", findings: []}], id(32)); }
  catch (error) { sourceFailure = error; }
  check(sourceFailure instanceof api.DataFogError && sourceFailure.code === "invalid_marker" && sourceFailure.inputIndex === 1, "preflight rejects before returning any batch plan");
  check(sourceFailure.markerIssues.map(issue => issue.kind).join() === "damaged,unknown", "source issue classifications survive installed WASM conversion");
  const firstIssue = sourceFailure.markerIssues[0];
  check(firstIssue.byteRange.start === 9 && firstIssue.codepointRange.start === 5 && firstIssue.utf16Range.start === 6, "source diagnostics preserve independent Unicode coordinate systems");
  check(!JSON.stringify(sourceFailure).includes("my_note") && equal(blocked.exportBundle(), blockedSaved), "source errors contain no input values and rejected preparation changes no mappings");
  blocked.close();
  const retained = api.Workspace.create(id(40), id(41));
  const pending = retained.prepare("Jane", [{entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}}], id(42));
  const noOp = retained.prepare("ordinary text", [], id(43));
  const noOpHeads = noOp.heads();
  retained.activateSaved(noOp, noOpHeads.expectedHead, noOpHeads.candidateHead);
  check(pending.preview().text.startsWith("[[PERSON_"), "no-op activation preserves valid current-head plans");
  const pendingHeads = pending.heads();
  throws(() => retained.activateSaved(pending, pendingHeads.expectedHead, {...pendingHeads.candidateHead, revision: "999"}), "workspace_conflict");
  check(pending.preview().text.startsWith("[[PERSON_"), "failed activation preserves valid plans");
  const live = [pending];
  for (let n = 0; n < 31; n++) live.push(retained.prepare("ordinary text", [], id(44 + n)));
  throws(() => retained.prepare("ordinary text", [], id(80)), "resource_limit");
  retained.activateSaved(pending, pendingHeads.expectedHead, pendingHeads.candidateHead);
  for (const stale of live) throws(() => stale.preview(), "inactive_plan");
  for (let n = 0; n < 40; n++) {
    const abandoned = retained.prepareBatch([{text: "ordinary text", findings: []}], id(81 + n * 2));
    const recovery = retained.prepareRecovery(id(82 + n * 2));
    const heads = recovery.heads();
    retained.activateSaved(recovery, heads.expectedHead, heads.candidateHead);
    throws(() => abandoned.previews(), "inactive_plan");
  }
  retained.close();
  const batch = api.Workspace.create(id(21), id(22));
  const name = { entityType: "PERSON", matchedText: "Jane", range: { unit: "utf8_bytes", start: 0, end: 4 } };
  const inputs = [{ text: "Jane | -10 USD", findings: [name] }, { text: "Jane | -20 USD", findings: [name] }];
  let grouped = batch.prepareBatch(inputs, id(23));
  const previews = grouped.previews();
  check(previews.length === 2 && previews[0].text.replace("-10", "-20") === previews[1].text, "batch shares aliases and preserves input order");
  throws(() => grouped.preview(), "invalid_batch");
  const batchSaved = grouped.exportCandidateBundle();
  check(grouped.heads().candidateHead.revision === "1", "one batch revision");
  batch.activateSaved(grouped, grouped.heads().expectedHead, grouped.heads().candidateHead);
  throws(() => grouped.previews(), "inactive_plan");
  const batchReopened = api.Workspace.openBundle(batchSaved);
  check(previews.every((p, i) => batchReopened.restore(p.text) === inputs[i].text), "batch restores each input after reopen");
  try { batch.prepareBatch([inputs[0], { text: "Jane", findings: [{ ...name, matchedText: "wrong" }] }], id(24)); throw new Error("missing rejection"); }
  catch (error) { check(error.code === "invalid_finding" && error.inputIndex === 1 && error.findingIndex === 0, "safe batch and finding indexes"); }
  throws(() => batch.prepareBatch([inputs[0], {text: "[[PERSON_ZZZZZZ]] literal", findings: []}], id(24)), "invalid_marker");
  check(equal(batch.exportBundle(), batchSaved), "rejected batch preserves complete saved bundle");
  grouped = batchReopened.prepareBatch(inputs, id(24));
  check(grouped.previews()[1].text === previews[1].text, "batch marker reuse");
  grouped.discard(); throws(() => grouped.previews(), "inactive_plan");
  throws(() => batch.prepareBatch([], id(24)), "invalid_batch");
  const exactly32 = batch.prepareBatch(Array(32).fill(inputs[0]), id(24));
  check(exactly32.previews().length === 32, "exactly 32 inputs accepted");
  exactly32.discard();
  throws(() => batch.prepareBatch(Array(33).fill(inputs[0]), id(25)), "resource_limit");
  const largeInput = {text: "x".repeat(512 * 1024 + 1), findings: []};
  throws(() => batch.prepareBatch([largeInput, largeInput], id(24)), "resource_limit");
  throws(() => batch.prepareBatch([inputs[0], {text: "Jane", findings: [{...name, range: {...name.range, unit: "utf16"}}]}], id(24)), "invalid_finding");
  let getterRead = false;
  throws(() => batch.prepareBatch([{get text() { getterRead = true; return "Jane"; }, findings: []}], id(24)), "invalid_batch");
  check(!getterRead, "batch transport rejects accessors without invoking them");
  batch.close(); batchReopened.close();
  const w = api.Workspace.create(id(1), id(2));
  const finding = { entityType: "CLIENT_NAME", matchedText: "Jane Smith", range: { unit: "utf8_bytes", start: 13, end: 23 } };
  const source = "2026-01-06 | Jane Smith | -40.00 USD";
  const before = w.exportBundle();
  let change = w.prepare(source, [finding], id(3));
  const preview = change.preview();
  check(equal(w.exportBundle(), before), "preparation preserves committed state");
  check(equal(change.exportCandidateBundle(), change.exportCandidateBundle()), "pending candidate exports are immutable");
  const marker = preview.text.match(/\[\[[A-Z][A-Z0-9_]*_[0-9A-HJKMNP-TV-Z]{6}\]\]/)?.[0];
  check(isMarker(marker, "CLIENT_NAME"), "short marker carries the exact caller-defined type");
  check(preview.text === source.replace(finding.matchedText, marker), "non-selected text preservation");
  throws(() => w.restore(marker), "invalid_marker");
  const saved = change.exportCandidateBundle();
  const heads = change.heads();
  w.activateSaved(change, heads.expectedHead, heads.candidateHead);
  throws(() => change.exportCandidateBundle(), "inactive_plan");
  throws(() => w.activateSaved(change, heads.expectedHead, heads.candidateHead), "inactive_plan");
  check(w.restore(marker) === finding.matchedText, "exact local restoration");
  throws(() => w.prepare(`Total for ${marker.slice(2, -2)}`, [], id(4)), "invalid_marker");
  check(w.restore(marker.toLowerCase()) === finding.matchedText, "restoration ignores marker case");
  throws(() => w.restore(`Total for ${marker.slice(2, -2)}`), "invalid_marker");
  const report = w.restoreReport(`👋 ${marker}, [[CLIENT_NAME_ZZZZZZ]], ${marker.slice(0, -3)}]] and ${marker.slice(2, -2)}`);
  check(report.text === null && report.restoredMarkers === 1 && report.issues.map(issue => issue.kind).join() === "unknown,damaged,unbracketed", "restore report lists every blocking span in order");
  check(report.issues[0].utf16Range.start === `👋 ${marker}, `.length, "restore report uses UTF-16 offsets for the host");
  const clean = w.restoreReport(`${marker} (ORDER_ABC123)`);
  check(clean.text === `${finding.matchedText} (ORDER_ABC123)` && clean.issues.length === 0, "restore report returns text only when complete");
  for (const bad of [marker.replace("CLIENT_NAME", "PERSON"), "[[CLIENT_NAME_ABC]]", "[[CLIENT_NAME_ZZZZZZ]]"]) {
    throws(() => w.restore(`${marker} ${bad}`), "invalid_marker");
  }
  const passthrough = w.prepare(`${marker}${marker}`, [], id(4));
  check(passthrough.preview().text === `${marker}${marker}` && passthrough.preview().transformations.length === 0, "committed markers pass through without new transformations");
  check(equal(passthrough.exportCandidateBundle(), saved), "marker-only follow-up preserves the saved snapshot");
  passthrough.discard();
  throws(() => w.prepare(source, [finding], id(3)), "invalid_operation_id");
  change = w.prepare(source, [finding], id(4));
  check(change.heads().expectedHead.revision === change.heads().candidateHead.revision, "no-change head");
  w.activateSaved(change, change.heads().expectedHead, change.heads().candidateHead);
  throws(() => change.preview(), "inactive_plan");
  const reopened = api.Workspace.openBundle(saved);
  change = reopened.prepare(source, [finding], id(5));
  check(change.preview().text === preview.text, "reopen preserves exact full marker");
  throws(() => w.activateSaved(change, change.heads().expectedHead, change.heads().candidateHead), "workspace_conflict");
  change.discard(); throws(() => change.preview(), "inactive_plan");
  const followup = api.Workspace.openBundle(saved);
  const question = `👋 é Compare ${marker} with John Smith, then ${marker}.`;
  const start = question.indexOf("John Smith");
  const freshFinding = {entityType: "CLIENT_NAME", matchedText: "John Smith", range: api.workspaceByteRangeFromUtf16(question, start, start + 10)};
  let answer = followup.prepare(question, [freshFinding], id(10));
  const answerPreview = answer.preview(), markers = answerPreview.text.match(/\[\[[A-Z][A-Z0-9_]*_[0-9A-HJKMNP-TV-Z]{6}\]\]/g);
  check(markers.length === 3 && markers[0] === marker && markers[2] === marker && isMarker(markers[1], "CLIENT_NAME") && markers[1] !== marker, "follow-up preserves old markers and allocates only the new exact value");
  check(answerPreview.text === question.replace("John Smith", markers[1]), "follow-up preserves all unselected text");
  check(answerPreview.transformations.length === 1 && answerPreview.transformations[0].sourceUtf16Range.start === start && answerPreview.transformations[0].sourceByteRange.start === freshFinding.range.start, "follow-up keeps input-local Unicode offsets");
  throws(() => followup.prepare(answerPreview.text, [], id(11)), "invalid_marker");
  const markerStart = question.indexOf(marker);
  throws(() => followup.prepare(question, [{entityType: "PERSON", matchedText: marker, range: api.workspaceByteRangeFromUtf16(question, markerStart, markerStart + marker.length)}], id(11)), "invalid_finding");
  const foreign = api.Workspace.create(id(9), id(2));
  const foreignPlan = foreign.prepare(source, [finding], id(3)), foreignMarker = foreignPlan.preview().text.match(/\[\[[A-Z][A-Z0-9_]*_[0-9A-HJKMNP-TV-Z]{6}\]\]/)[0];
  foreign.activateSaved(foreignPlan, foreignPlan.heads().expectedHead, foreignPlan.heads().candidateHead);
  for (const bad of [foreignMarker, marker.replace("CLIENT_NAME", "OTHER"), "[[CLIENT_NAME_ZZZZZZ]]"]) {
    try { followup.prepareBatch([{text: question, findings: [freshFinding]}, {text: `${marker} ${bad}`, findings: []}], id(11)); throw new Error("missing follow-up rejection"); }
    catch (error) { check(error.code === "invalid_marker" && error.inputIndex === 1 && error.findingIndex == null && error.message === "invalid_marker", "invalid follow-up markers reject the whole batch without source content"); }
    check(equal(followup.exportBundle(), saved), "rejected follow-up preserves committed mappings");
  }
  followup.activateSaved(answer, answer.heads().expectedHead, answer.heads().candidateHead);
  const followupSaved = followup.exportBundle(), followupReopened = api.Workspace.openBundle(followupSaved);
  check(followupReopened.restore(answerPreview.text) === question.replaceAll(marker, finding.matchedText), "follow-up markers restore exactly after reopen");
  answer = followupReopened.prepare(answerPreview.text, [], id(12));
  check(answer.preview().text === answerPreview.text && equal(answer.exportCandidateBundle(), followupSaved), "saved follow-up aliases remain unchanged on resubmission");
  answer.discard(); followup.close(); followupReopened.close(); foreign.close();
  const selection = api.workspaceByteRangeFromUtf16("👋 é Jane", 6, 10);
  check(selection.start === 9 && selection.end === 13 && selection.unit === "utf8_bytes", "Core UTF-16 conversion");
  throws(() => api.workspaceByteRangeFromUtf16("👋 Jane", 1, 7), "invalid_finding");
  throws(() => w.prepare("\uD800", [], id(8)), "invalid_finding");
  for (const entityType of ["", "bad:type", "_PERSON", " PERSON", "Person", "X".repeat(33)]) throws(() => w.prepare(source, [{...finding, entityType}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, matchedText: "wrong"}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, range: {...finding.range, unit: "utf16"}}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, confidence: -1}], id(6)), "invalid_finding");
  throws(() => w.prepare("x".repeat(1024 * 1024 + 1), [], id(6)), "resource_limit");
  throws(() => w.prepare("", Array(10001).fill(finding), id(6)), "resource_limit");
  const repeated = { ...finding, matchedText: "x".repeat(64 * 1024), range: { unit: "utf8_bytes", start: 0, end: 64 * 1024 } };
  throws(() => w.prepare("x".repeat(64 * 1024), Array(1024).fill(repeated), id(6)), "resource_limit");
  throws(() => w.prepare(source, [{ ...finding, confidence: { nested: Array(10000).fill(1) } }], id(6)), "invalid_finding");
  check(equal(w.exportBundle(), saved), "invalid operations preserve committed bundle");
  for (const malformed of [saved.slice(0, -1), new Uint8Array([...saved, 0]), new Uint8Array([0xbf, ...saved.slice(1)]), new Uint8Array([0xa7, 0x18, 0, ...saved.slice(2)])]) {
    throws(() => api.Workspace.openBundle(malformed), "invalid_snapshot");
  }
  let duplicate = saved.slice(); duplicate[3] = 0; throws(() => api.Workspace.openBundle(duplicate), "invalid_snapshot");
  for (const [name, fixture] of Object.entries(counters)) {
    const bytes = Uint8Array.from(fixture.bundleHex.match(/../g), pair => Number.parseInt(pair, 16));
    const high = api.Workspace.openBundle(bytes);
    check(high.head().revision === fixture.revision, "lossless exposed uint64 revision");
    check(equal(bytes, high.exportBundle()) && high.restore(fixture.marker) === "Jane", "CBOR uint64 fixture round trip");
    const unchanged = high.prepare(fixture.marker, [], id(6));
    check(unchanged.preview().text === fixture.marker && equal(unchanged.exportCandidateBundle(), bytes), "marker-only input does not advance even a maximum uint64 counter");
    unchanged.discard();
    if (name === "maximum") throws(() => high.prepareRecovery(id(7)), "resource_limit");
    else {
      const recovery = high.prepareRecovery(id(7));
      const {expectedHead, candidateHead} = recovery.heads();
      check(recovery.previews().length === 0 && candidateHead.revision === String(BigInt(fixture.revision) + 1n), "recovery has a fresh lossless head and no handoff preview");
      check(equal(high.exportBundle(), bytes), "preparing recovery leaves source unchanged");
      const recovered = api.Workspace.openBundle(recovery.exportCandidateBundle());
      check(recovered.restore(fixture.marker) === "Jane", "recovery preserves complete issued markers");
      high.activateSaved(recovery, expectedHead, candidateHead);
      throws(() => recovery.exportCandidateBundle(), "inactive_plan");
      throws(() => high.prepareRecovery(id(7)), "invalid_operation_id");
      recovered.close();
    }
    const fresh = { entityType: "PERSON", matchedText: "John", range: { unit: "utf8_bytes", start: 0, end: 4 } };
    if (name === "maximum") throws(() => high.prepare("John", [fresh], id(6)), "resource_limit");
    else {
      const p = high.prepare("John", [fresh], id(6));
      check(p.heads().candidateHead.revision === String(BigInt(fixture.revision) + 2n), "lossless uint64 increment after recovery");
      check(isMarker(p.preview().text, "PERSON") && p.preview().text !== fixture.marker, "a uint64 counter still issues a fresh short marker");
      p.discard();
    }
    high.close();
  }
  change = w.prepare("", [], id(7)); w.close();
  throws(() => change.preview(), "closed_workspace");
  reopened.close();
  return {reviewMeasurements};
}
