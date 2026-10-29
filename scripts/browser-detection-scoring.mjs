import assert from "node:assert/strict";

export function validateSpan(text, finding) {
  assert.equal(typeof finding.entityType, "string");
  const {start, end} = finding.byteRange;
  assert(Number.isSafeInteger(start) && Number.isSafeInteger(end) && start >= 0 && end > start && end <= Buffer.byteLength(text), "invalid byte range");
  const bytes = Buffer.from(text);
  const prefix = new TextDecoder("utf-8", {fatal: true}).decode(bytes.subarray(0, start));
  const match = new TextDecoder("utf-8", {fatal: true}).decode(bytes.subarray(start, end));
  assert.equal(match, finding.matchedText, "source mismatch");
  if (finding.utf16Range) assert.equal(text.slice(finding.utf16Range.start, finding.utf16Range.end), match, "UTF-16 mismatch");
  if (finding.codepointRange) assert.equal([...text].slice(finding.codepointRange.start, finding.codepointRange.end).join(""), match, "codepoint mismatch");
  return {start, end, codepointStart: [...prefix].length};
}

// Maximum one-to-one matching: one oversized prediction cannot recall two gold spans.
export function matchSpans(gold, predictions, mode) {
  assert(["exact", "overlap"].includes(mode));
  const eligible = (a, b) => a.entityType === b.entityType && (mode === "exact"
    ? a.byteRange.start === b.byteRange.start && a.byteRange.end === b.byteRange.end
    : a.byteRange.start < b.byteRange.end && b.byteRange.start < a.byteRange.end);
  const matchedGold = new Map();
  function assign(predictionIndex, visited) {
    for (let goldIndex = 0; goldIndex < gold.length; goldIndex++) {
      if (visited.has(goldIndex) || !eligible(gold[goldIndex], predictions[predictionIndex])) continue;
      visited.add(goldIndex);
      if (!matchedGold.has(goldIndex) || assign(matchedGold.get(goldIndex), visited)) {
        matchedGold.set(goldIndex, predictionIndex); return true;
      }
    }
    return false;
  }
  for (let index = 0; index < predictions.length; index++) assign(index, new Set());
  const matchedPredictions = new Set(matchedGold.values());
  return {tp: matchedGold.size, fp: predictions.length - matchedGold.size, fn: gold.length - matchedGold.size,
    misses: gold.filter((_, index) => !matchedGold.has(index)),
    falsePositives: predictions.filter((_, index) => !matchedPredictions.has(index))};
}

export function scoreCorpus(corpus, observations) {
  assert.equal(observations.length, corpus.records.length);
  const byId = new Map(observations.map(value => [value.id, value]));
  assert.equal(byId.size, observations.length);
  const results = {};
  for (const view of ["shipping", "streetLineDiagnostic"]) {
    const types = view === "shipping" ? corpus.scoredTypes : ["STREET_ADDRESS"];
    results[view] = {};
    for (const mode of ["exact", "overlap"]) {
      const totals = Object.fromEntries(types.map(type => [type, {tp: 0, fp: 0, fn: 0}]));
      const details = [];
      for (const record of corpus.records) {
        const observed = byId.get(record.id); assert(observed, `missing observation ${record.id}`);
        for (const value of [...record.gold, ...record.streetLineGold, ...observed.findings]) validateSpan(record.text, value);
        // Evaluate street diagnostics only where a separate street-line annotation exists.
        if (view === "streetLineDiagnostic" && record.streetLineGold.length === 0) continue;
        const gold = view === "shipping" ? record.gold : record.streetLineGold;
        const predictions = view === "shipping" ? observed.findings : observed.findings.filter(value => value.entityType === "ADDRESS").map(value => ({...value, entityType: "STREET_ADDRESS"}));
        for (const type of types) {
          const result = matchSpans(gold.filter(value => value.entityType === type), predictions.filter(value => value.entityType === type), mode);
          for (const key of ["tp", "fp", "fn"]) totals[type][key] += result[key];
          if (result.fn || result.fp) details.push({id: record.id, entityType: type, ...result});
        }
      }
      for (const total of Object.values(totals)) {
        total.precision = total.tp + total.fp ? total.tp / (total.tp + total.fp) : null;
        total.recall = total.tp + total.fn ? total.tp / (total.tp + total.fn) : null;
        const denominator = 2 * total.tp + total.fp + total.fn;
        total.f1 = denominator ? 2 * total.tp / denominator : null;
      }
      results[view][mode] = {totals, details};
    }
  }
  return results;
}
