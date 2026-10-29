import assert from "node:assert/strict";
import {validateSpan} from "./browser-detection-scoring.mjs";

// Actual SDK results must describe one source span in every documented unit.
// Historical scoring stays unchanged; malformed transport is never scored.
export function validateSdkSpan(text, finding) {
  const {start, end, codepointStart} = validateSpan(text, finding);
  const prefix = Buffer.from(text).subarray(0, start).toString("utf8");
  for (const [name, expectedStart, expectedEnd] of [
    ["utf16Range", prefix.length, prefix.length + finding.matchedText.length],
    ["codepointRange", codepointStart, codepointStart + [...finding.matchedText].length],
  ]) {
    const range = finding[name];
    assert(range && Number.isSafeInteger(range.start) && Number.isSafeInteger(range.end), `invalid ${name}`);
    assert.equal(range.start, expectedStart, `${name} start disagrees with bytes`);
    assert.equal(range.end, expectedEnd, `${name} end disagrees with bytes`);
  }
  return {start, end};
}
