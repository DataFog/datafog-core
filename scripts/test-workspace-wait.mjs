import assert from "node:assert/strict";
import {test} from "node:test";
import {waitForBrokerState} from "./workspace-test-wait.mjs";

test("broker polling awaits false RPC results until the checkpoint is actually reached", async () => {
  let reads = 0;
  const result = await waitForBrokerState(async () => ({paused: ++reads < 4 ? [] : ["after-import-open"]}),
    state => state.paused.includes("after-import-open"), {timeoutMs: 500, intervalMs: 1});
  assert.equal(reads, 4);
  assert.deepEqual(result.paused, ["after-import-open"]);
});

test("a never-satisfied broker condition rejects with bounded content-free diagnostics", async () => {
  let reads = 0;
  await assert.rejects(waitForBrokerState(async () => {
    reads++;
    return {busy: true, paused: ["after-import-open", "private source"], source: "private source"};
  }, () => false, {label: "await checkpoint", timeoutMs: 20, intervalMs: 1}), error => {
    assert.match(error.message, /await checkpoint.*timed out/);
    assert.match(error.message, /"busy":true/);
    assert.match(error.message, /after-import-open/);
    assert(!error.message.includes("private source"));
    return true;
  });
  assert(reads > 0);
});

test("an unresponsive broker RPC cannot bypass the polling deadline", async () => {
  await assert.rejects(waitForBrokerState(() => new Promise(() => {}), () => true,
    {timeoutMs: 20}), /timed out waiting for broker state/);
});
