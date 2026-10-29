import {setTimeout as delay} from "node:timers/promises";

const checkpoints = new Set(["before-save", "after-save", "after-import-open", "before-import-save", "after-import-save"]);
function diagnostics(state) {
  if (!state) return {};
  return {
    ...Object.fromEntries(["busy", "pending", "importPending", "locked"].filter(key => typeof state[key] === "boolean").map(key => [key, state[key]])),
    ...(Array.isArray(state.paused) ? {paused: state.paused.filter(name => checkpoints.has(name))} : {}),
  };
}

/** Await RPC state in Node; a Promise is not a truthy browser polling result. */
export async function waitForBrokerState(readState, predicate, {label = "broker condition", timeoutMs = 30000, intervalMs = 25} = {}) {
  const deadline = performance.now() + timeoutMs;
  let last;
  const timeout = () => new Error(`${label}: timed out waiting for broker state ${JSON.stringify(diagnostics(last))}`);
  while (true) {
    const remaining = deadline - performance.now();
    if (remaining <= 0) throw timeout();
    let timer;
    try {
      const accepted = await Promise.race([
        (async () => { last = await readState(); return await predicate(last); })(),
        new Promise((_, reject) => { timer = setTimeout(() => reject(timeout()), remaining); }),
      ]);
      if (accepted) return last;
    } finally { clearTimeout(timer); }
    await delay(Math.min(intervalMs, Math.max(0, deadline - performance.now())));
  }
}
