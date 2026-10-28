export class HostError extends Error {
  constructor(code) { super(code); this.name = "HostError"; this.code = code; }
}
const known = new Set([
  "closed_workspace", "inactive_plan", "workspace_conflict", "invalid_finding", "invalid_marker",
  "invalid_snapshot", "invalid_operation_id", "resource_limit", "invalid_credentials", "invalid_envelope",
  "storage_failure", "save_uncertain", "locked", "stale_approval", "cancelled", "already_exists", "not_found", "internal_error",
]);
export function safeCode(error) { return known.has(error?.code) ? error.code : "internal_error"; }

export function client(worker) {
  let next = 1;
  const pending = new Map();
  let stopped = false;
  worker.addEventListener("message", ({ data }) => {
    const request = pending.get(data.id);
    if (!request) return;
    pending.delete(data.id);
    if (data.error) request.reject(new HostError(known.has(data.error) ? data.error : "internal_error"));
    else request.resolve(data.result);
  });
  const fail = event => {
    event?.preventDefault?.(); stopped = true; worker.terminate();
    for (const request of pending.values()) request.reject(new HostError("internal_error"));
    pending.clear();
  };
  worker.addEventListener("error", fail);
  worker.addEventListener("messageerror", fail);
  return {
    call(method, args = []) {
      if (stopped) return Promise.reject(new HostError("closed_workspace"));
      const id = next++;
      return new Promise((resolve, reject) => {
        pending.set(id, {resolve, reject});
        try { worker.postMessage({ id, method, args }); }
        catch { pending.delete(id); reject(new HostError("internal_error")); }
      });
    },
    stop() { stopped = true; worker.terminate(); fail(); },
  };
}

export function serve(methods) {
  self.addEventListener("message", async ({ data }) => {
    const {id, method, args} = data;
    try {
      if (!Object.hasOwn(methods, method)) throw new HostError("internal_error");
      const result = await methods[method](...args);
      if (result instanceof Uint8Array) self.postMessage({ id, result }, [result.buffer]);
      else self.postMessage({ id, result });
    } catch (error) { self.postMessage({ id, error: safeCode(error) }); }
  });
}
