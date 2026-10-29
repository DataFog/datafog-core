import initWasm, {
  transform_structured as nativeTransformStructured,
  scan_and_transform_structured as nativeScanAndTransformStructured,
  restore_structured as nativeRestoreStructured,
  discover_fields as nativeDiscoverFields,
  scan_structured as nativeScanStructured,
  scan as scanWasm,
  scan_and_transform as scanAndTransformWasm,
  restore as restoreWasm,
  transform as transformWasm,
  NativeWorkspace,
  workspace_byte_range_from_utf16 as nativeWorkspaceByteRange,
} from "./dist/datafog_wasm.js";

let initialization;
let initialized = false;

export function init() {
  if (!initialization) {
    initialization = initWasm({
      module_or_path: new URL("./dist/datafog_wasm_bg.wasm", import.meta.url),
    })
      .then(() => {
        initialized = true;
      })
      .catch((error) => {
        initialization = undefined;
        throw error;
      });
  }

  return initialization;
}

export class DataFogError extends Error {
  constructor({ code, reason, message, path, findingIndex, inputIndex, markerIssues }) {
    super(message);
    this.name = "DataFogError";
    this.code = code;
    this.reason = reason;
    this.path = path;
    this.findingIndex = findingIndex;
    this.inputIndex = inputIndex;
    this.markerIssues = markerIssues;
  }
}

function normalizeError(error, fallbackCode) {
  if (error instanceof DataFogError) return error;
  const source = error instanceof Error ? error.message : String(error);
  try {
    const details = JSON.parse(source);
    if (typeof details.code === "string" && typeof details.message === "string") {
      return new DataFogError(details);
    }
  } catch {
    // Raw WASM conversion errors use the operation-specific fallback below.
  }
  return new DataFogError({
    code: fallbackCode,
    reason: fallbackCode === "invalid_configuration" ? "invalid_type" : undefined,
    message:
      fallbackCode === "invalid_configuration"
        ? "request configuration could not be decoded"
        : "the WASM operation failed unexpectedly",
    path: fallbackCode === "invalid_configuration" ? "" : undefined,
  });
}

export function scan(text, config) {
  if (typeof text !== "string") {
    throw new TypeError("scan text must be a string");
  }

  if (!initialized) {
    throw new Error("Call and await init() before scan().");
  }

  try {
    return scanWasm(text, config);
  } catch (error) {
    throw normalizeError(
      error,
      config === undefined ? "internal_error" : "invalid_configuration",
    );
  }
}

function assertInitialized(operation) {
  if (!initialized) {
    throw new Error(`Call and await init() before ${operation}().`);
  }
}

export function transform(text, findings, config) {
  if (typeof text !== "string") {
    throw new TypeError("transform text must be a string");
  }
  if (!Array.isArray(findings)) {
    throw new TypeError("transform findings must be an array");
  }
  assertInitialized("transform");

  try {
    return transformWasm(text, findings, config);
  } catch (error) {
    throw normalizeError(error, "invalid_configuration");
  }
}

export function scanAndTransform(text, config) {
  if (typeof text !== "string") {
    throw new TypeError("scanAndTransform text must be a string");
  }
  assertInitialized("scanAndTransform");

  try {
    return scanAndTransformWasm(text, config);
  } catch (error) {
    throw normalizeError(error, "invalid_configuration");
  }
}

export function restore(text, context) {
  if (typeof text !== "string") {
    throw new TypeError("restore text must be a string");
  }
  assertInitialized("restore");
  try {
    return restoreWasm(text, context);
  } catch (error) {
    throw normalizeError(error, "invalid_configuration");
  }
}

function structuredJson(data, omitUndefinedOptions = false) {
  try {
    if (data === null || typeof data !== "object") throw new TypeError();
    const pending = [data];
    const seen = new Set();
    while (pending.length) {
      const value = pending.pop();
      if (value === null || typeof value === "string" || typeof value === "boolean") continue;
      if (typeof value === "number") {
        if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) throw new TypeError();
        continue;
      }
      if (typeof value !== "object") throw new TypeError();
      if (seen.has(value)) continue;
      seen.add(value);
      const array = Array.isArray(value);
      if (!array && Object.getPrototypeOf(value) !== Object.prototype && Object.getPrototypeOf(value) !== null) throw new TypeError();
      if (array && Object.keys(value).length !== value.length) throw new TypeError();
      for (const key of Reflect.ownKeys(value)) {
        if (array && key === "length") continue;
        if (typeof key !== "string") throw new TypeError();
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (!descriptor.enumerable || !("value" in descriptor)) throw new TypeError();
        if (array && (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= value.length)) throw new TypeError();
        if (omitUndefinedOptions && !array && descriptor.value === undefined) continue;
        pending.push(descriptor.value);
      }
    }
    return JSON.stringify(data);
  } catch {
    throw new DataFogError({code:"invalid_configuration", reason:"invalid_type", path:"/data", message:"data must be a JSON object or array with finite numbers and safe integers"});
  }
}

export function discoverFields(data, config) {
  assertInitialized("discoverFields");
  const json = structuredJson(data);
  try { return nativeDiscoverFields(json, structuredOptions(config)); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

export function scanStructured(data, config) {
  assertInitialized("scanStructured");
  const json = structuredJson(data);
  try { return nativeScanStructured(json, structuredOptions(config)); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

export function transformStructured(data, findings, config) {
  assertInitialized("transformStructured");
  const json = structuredJson(data);
  try { const {dataJson, transformations} = nativeTransformStructured(json, structuredOptions(findings), structuredOptions(config)); return {data:JSON.parse(dataJson), transformations}; } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}
export function scanAndTransformStructured(data, config) {
  assertInitialized("scanAndTransformStructured");
  const json = structuredJson(data);
  try { const {dataJson, transformations} = nativeScanAndTransformStructured(json, structuredOptions(config)); return {data:JSON.parse(dataJson), transformations}; } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}
export function restoreStructured(data, context) {
  assertInitialized("restoreStructured");
  const json = structuredJson(data);
  try { return nativeRestoreStructured(json, structuredOptions(context)); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

function structuredOptions(value) {
  if (value === undefined) return undefined;
  try { return JSON.parse(structuredJson(value, true)); } catch {
    throw new DataFogError({code:"invalid_configuration", reason:"invalid_type", path:"", message:"structured request options must be JSON-compatible"});
  }
}

const workspacePlans = new WeakMap();
const workspaceToken = Symbol("workspace");
function workspaceFailure(code) {
  return new DataFogError({ code, message: code });
}
function workspaceText(text, limit) {
  if (typeof text !== "string" || !text.isWellFormed()) throw workspaceFailure("invalid_finding");
  if (text.length > limit || new TextEncoder().encode(text).length > limit) throw workspaceFailure("resource_limit");
}
function workspaceBytes(bytes, limit, exact = false) {
  if (!(bytes instanceof Uint8Array)) throw workspaceFailure(exact ? "invalid_operation_id" : "invalid_snapshot");
  if (exact ? bytes.length !== limit : bytes.length > limit) throw workspaceFailure(exact ? "invalid_operation_id" : "resource_limit");
}
function workspaceCall(state, method, ...args) {
  if (state.closed) throw workspaceFailure("closed_workspace");
  try { return state.native[method](...args); }
  catch (error) { throw normalizeError(error, "internal_error"); }
}

// Bound the fixed finding transport before JSON.stringify can expand repeated
// references or large nested values. Core still owns span/type/selection semantics.
function workspaceFindingsJson(findings) {
  const maximum = 16 * 1024 * 1024;
  let budget = 2;
  function object(value, allowed) {
    if (!value || typeof value !== "object" || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) throw workspaceFailure("invalid_finding");
    const properties = Object.getOwnPropertyDescriptors(value);
    for (const key of Reflect.ownKeys(properties)) {
      const property = properties[key];
      if (typeof key !== "string" || !allowed.includes(key) || !property.enumerable || !("value" in property)) throw workspaceFailure("invalid_finding");
    }
    return Object.fromEntries(Object.entries(properties).map(([key, property]) => [key, property.value]));
  }
  function string(value) {
    if (typeof value !== "string" || !value.isWellFormed()) throw workspaceFailure("invalid_finding");
    if (value.length > maximum) throw workspaceFailure("resource_limit");
    budget += 2;
    for (const character of value) {
      const point = character.codePointAt(0);
      budget += point < 32 ? 6 : point === 34 || point === 92 ? 2 : point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
      if (budget > maximum) throw workspaceFailure("resource_limit");
    }
  }
  for (let i = 0; i < findings.length; i++) {
    const item = Object.getOwnPropertyDescriptor(findings, String(i));
    if (!item?.enumerable || !("value" in item)) throw workspaceFailure("invalid_finding");
    const value = item.value;
    const f = object(value, ["entityType", "matchedText", "range", "confidence", "detectorName", "detectorVersion"]);
    const range = object(f.range, ["unit", "start", "end"]);
    budget += 256; // A conservative bound for keys, punctuation and scalar numbers.
    string(f.entityType); string(f.matchedText); string(range.unit);
    for (const field of ["detectorName", "detectorVersion"]) if (f[field] !== undefined && f[field] !== null) string(f[field]);
    if (!Number.isSafeInteger(range.start) || !Number.isSafeInteger(range.end) || (f.confidence !== undefined && f.confidence !== null && typeof f.confidence !== "number")) throw workspaceFailure("invalid_finding");
    if (budget > maximum) throw workspaceFailure("resource_limit");
  }
  try { return structuredJson(findings, true); }
  catch { throw workspaceFailure("invalid_finding"); }
}
class PreparedWorkspaceChange {
  constructor(state, handle) { workspacePlans.set(this, { state, handle }); Object.freeze(this); }
  previews() { const {state, handle} = workspacePlans.get(this); return JSON.parse(workspaceCall(state, "previews", handle)); }
  preview() { const {state, handle} = workspacePlans.get(this); return JSON.parse(workspaceCall(state, "preview", handle)); }
  heads() { const {state, handle} = workspacePlans.get(this); return JSON.parse(workspaceCall(state, "plan_heads", handle)); }
  exportCandidateBundle() { const {state, handle} = workspacePlans.get(this); return workspaceCall(state, "export_candidate_bundle", handle); }
  discard() { const {state, handle} = workspacePlans.get(this); workspaceCall(state, "discard", handle); }
}

/** Provisional reversible-workspace API. Credentials and durable storage remain host-owned. */
export class Workspace {
  #state;
  constructor(token, native) {
    if (token !== workspaceToken) throw workspaceFailure("invalid_configuration");
    this.#state = { native, closed: false };
  }
  static create(namespace, operationId) {
    assertInitialized("Workspace.create");
    workspaceBytes(namespace, 16, true); workspaceBytes(operationId, 16, true);
    try { return new Workspace(workspaceToken, NativeWorkspace.create(namespace, operationId)); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  static openBundle(bytes) {
    assertInitialized("Workspace.openBundle"); workspaceBytes(bytes, 16 * 1024 * 1024);
    try { return new Workspace(workspaceToken, NativeWorkspace.open_bundle(bytes)); }
    catch (error) { throw normalizeError(error, "invalid_snapshot"); }
  }
  head() { return JSON.parse(workspaceCall(this.#state, "head")); }
  namespace() { return workspaceCall(this.#state, "namespace"); }
  exportBundle() { return workspaceCall(this.#state, "export_bundle"); }
  prepareRecovery(operationId) {
    workspaceBytes(operationId, 16, true);
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_recovery", operationId));
  }
  prepare(text, findings, operationId) {
    workspaceText(text, 1024 * 1024); workspaceBytes(operationId, 16, true);
    if (!Array.isArray(findings)) throw workspaceFailure("invalid_finding");
    if (findings.length > 10000) throw workspaceFailure("resource_limit");
    let json;
    try { json = workspaceFindingsJson(findings); }
    catch (error) { if (error instanceof DataFogError) throw error; throw workspaceFailure("invalid_finding"); }
    workspaceText(json, 16 * 1024 * 1024);
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare", text, json, operationId));
  }
  prepareBatch(inputs, operationId) {
    workspaceBytes(operationId, 16, true);
    if (!Array.isArray(inputs) || inputs.length === 0) throw workspaceFailure("invalid_batch");
    if (inputs.length > 32) throw workspaceFailure("resource_limit");
    if (Reflect.ownKeys(inputs).length !== inputs.length + 1) throw workspaceFailure("invalid_batch");
    const parts = [];
    let sourceBytes = 0, findingCount = 0, transportBytes = 2;
    for (let inputIndex = 0; inputIndex < inputs.length; inputIndex++) {
      try {
        const item = Object.getOwnPropertyDescriptor(inputs, String(inputIndex));
        if (!item?.enumerable || !("value" in item)) throw workspaceFailure("invalid_batch");
        const input = item.value;
        if (!input || typeof input !== "object" || ![Object.prototype, null].includes(Object.getPrototypeOf(input))) throw workspaceFailure("invalid_batch");
        const properties = Object.getOwnPropertyDescriptors(input);
        if (Reflect.ownKeys(properties).length !== 2 || !properties.text?.enumerable || !("value" in properties.text) || !properties.findings?.enumerable || !("value" in properties.findings)) throw workspaceFailure("invalid_batch");
        const {value: text} = properties.text, {value: findings} = properties.findings;
        workspaceText(text, 1024 * 1024);
        if (!Array.isArray(findings)) throw workspaceFailure("invalid_finding");
        sourceBytes += new TextEncoder().encode(text).length;
        findingCount += findings.length;
        if (sourceBytes > 1024 * 1024 || findingCount > 10000) throw workspaceFailure("resource_limit");
        const findingsJson = workspaceFindingsJson(findings);
        const textJson = JSON.stringify(text);
        transportBytes += 32 + new TextEncoder().encode(textJson).length + new TextEncoder().encode(findingsJson).length;
        if (transportBytes > 16 * 1024 * 1024) throw workspaceFailure("resource_limit");
        parts.push(`{"text":${textJson},"findings":${findingsJson}}`);
      } catch (error) {
        const failure = error instanceof DataFogError ? error : workspaceFailure("invalid_finding");
        failure.inputIndex = inputIndex;
        throw failure;
      }
    }
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_batch", `[${parts.join(",")}]`, operationId));
  }
  activateSaved(change, expectedHead, savedHead) {
    const plan = workspacePlans.get(change);
    if (!plan || plan.state !== this.#state) throw workspaceFailure("workspace_conflict");
    let expected, saved;
    try { expected = structuredJson(expectedHead); saved = structuredJson(savedHead); }
    catch { throw workspaceFailure("workspace_conflict"); }
    workspaceCall(this.#state, "activate_saved", plan.handle, expected, saved);
  }
  restore(text) { workspaceText(text, 16 * 1024 * 1024); return workspaceCall(this.#state, "restore", text); }
  restoreReport(text) { workspaceText(text, 16 * 1024 * 1024); return JSON.parse(workspaceCall(this.#state, "restore_report", text)); }
  close() {
    if (!this.#state.closed) {
      workspaceCall(this.#state, "close");
      this.#state.native.free();
      this.#state.closed = true;
    }
  }
}

export function workspaceByteRangeFromUtf16(text, start, end) {
  assertInitialized("workspaceByteRangeFromUtf16"); workspaceText(text, 1024 * 1024);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < 0) throw workspaceFailure("invalid_finding");
  try { return JSON.parse(nativeWorkspaceByteRange(text, start, end)); }
  catch (error) { throw normalizeError(error, "invalid_finding"); }
}
