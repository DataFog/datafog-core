export class HostError extends Error {
  constructor(code, location = {}) {
    super(code); this.name = "HostError"; this.code = code;
    for (const key of ["inputIndex", "findingIndex"]) if (Number.isSafeInteger(location?.[key]) && location[key] >= 0) this[key] = location[key];
    if (location?.markerIssues !== undefined) {
      const ranges = ["byteRange", "codepointRange", "utf16Range"];
      const valid = issue => ["unknown", "damaged", "unbracketed"].includes(issue?.kind) && ranges.every(key => {
        const range = issue[key];
        return Number.isSafeInteger(range?.start) && Number.isSafeInteger(range?.end) && range.start >= 0 && range.end > range.start && range.end <= 16 * 1024 * 1024;
      });
      if (!Array.isArray(location.markerIssues) || location.markerIssues.length > 10000 || !location.markerIssues.every(valid)) {
        this.code = "internal_error"; this.message = "internal_error"; return;
      }
      this.markerIssues = location.markerIssues.map(issue => Object.fromEntries([
        ["kind", issue.kind], ...ranges.map(key => [key, {start: issue[key].start, end: issue[key].end}]),
      ]));
    }
    if (location?.reviewConflicts !== undefined) {
      const ranges=["byteRange","codepointRange","utf16Range"];
      const valid=issue=>["protected_coverage","type_conflict"].includes(issue?.kind) && Number.isSafeInteger(issue.findingIndex) && issue.findingIndex>=0 && issue.findingIndex<10000 && ranges.every(key=>{
        const range=issue[key];return Number.isSafeInteger(range?.start) && Number.isSafeInteger(range?.end) && range.start>=0 && range.end>range.start && range.end<=1024*1024;
      });
      if(!Array.isArray(location.reviewConflicts) || location.reviewConflicts.length>20000 || !location.reviewConflicts.every(valid)) {this.code="internal_error";this.message="internal_error";return;}
      this.reviewConflicts=location.reviewConflicts.map(issue=>Object.fromEntries([["kind",issue.kind],["findingIndex",issue.findingIndex],...ranges.map(key=>[key,{start:issue[key].start,end:issue[key].end}])]));
    }

  }
}
const known = new Set([
  "closed_workspace", "inactive_plan", "workspace_conflict", "invalid_batch", "invalid_finding", "invalid_marker", "review_conflict",
  "invalid_policy", "invalid_snapshot", "invalid_operation_id", "resource_limit", "invalid_credentials", "invalid_envelope",
  "storage_failure", "save_uncertain", "locked", "stale_approval", "cancelled", "already_exists", "not_found", "internal_error",
  "invalid_backup", "recovery_unavailable", "kdf_unavailable",
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
    if (data.error) request.reject(new HostError(known.has(data.error) ? data.error : "internal_error", data.location));
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
    } catch (error) {
      const safe = new HostError(safeCode(error), error);
      self.postMessage({ id, error: safe.code, location: { inputIndex: safe.inputIndex, findingIndex: safe.findingIndex, markerIssues: safe.markerIssues, reviewConflicts:safe.reviewConflicts } });
    }
  });
}
