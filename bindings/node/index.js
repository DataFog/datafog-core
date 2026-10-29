import { Buffer } from "node:buffer";
import {
  NativeWorkspace,
  openModelSession as nativeOpenModelSession,
  structuredTransform as nativeStructuredTransform,
  structuredScanAndTransform as nativeStructuredScanAndTransform,
  structuredRequiredKeySelectors as nativeStructuredRequiredKeySelectors,
  structuredRequiredTokenizationItems as nativeStructuredRequiredTokenizationItems,
  structuredTransformWithProviderResults as nativeStructuredTransformWithProviderResults,
  structuredRequiredRestoreItems as nativeStructuredRequiredRestoreItems,
  structuredRestoreWithResults as nativeStructuredRestoreWithResults,
  prepareStructuredScanAndTransform as nativePrepareStructuredScanAndTransform,
  discoverFields as nativeDiscoverFields,
  scanStructured as nativeScanStructured,
  prepareScanAndTransform as nativePrepareScanAndTransform,
  requiredKeySelectors as nativeRequiredKeySelectors,
  requiredRestoreItems as nativeRequiredRestoreItems,
  requiredTokenizationItems as nativeRequiredTokenizationItems,
  restoreWithResults as nativeRestoreWithResults,
  scan as nativeScan,
  scanAndTransform as nativeScanAndTransform,
  transform as nativeTransform,
  transformWithProviderResults as nativeTransformWithProviderResults,
} from "./native.js";

export { ModelCancellation, ModelSessionNative, openModelSession } from "./native.js";

// An unknown option, such as a misspelled `config`, would otherwise be ignored.
function modelScanOptions(options, allowed) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new TypeError("model scan options must be an object");
  }
  for (const key of Object.keys(options)) {
    if (!allowed.includes(key)) throw new TypeError(`unknown model scan option: ${key}`);
  }
  return options;
}

/** Explicit offline model session. Inference and cleanup run outside the event loop. */
export class ModelSession {
  #native;
  constructor(native) { this.#native = native; }
  describe() {
    // JSON.parse preserves custom labels such as __proto__ as own properties.
    try { return JSON.parse(this.#native.describe()); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  static async open(config) {
    const snapshot = JSON.parse(structuredJson(config, true));
    try { return new ModelSession(await nativeOpenModelSession(snapshot)); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  async scan(text, options = {}) {
    if (typeof text !== "string") throw new TypeError("model scan text must be a string");
    const { cancellation, config } = modelScanOptions(options, ["cancellation", "config"]);
    const snapshot = config === undefined ? undefined : JSON.parse(structuredJson(config, true));
    try { return await this.#native.scan(text, cancellation, snapshot); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  async scanModel(text, options = {}) {
    if (typeof text !== "string") throw new TypeError("model scan text must be a string");
    const { cancellation } = modelScanOptions(options, ["cancellation"]);
    try { return await this.#native.scanModel(text, cancellation); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  async close() { try { await this.#native.close(); } catch (error) { throw normalizeError(error, "internal_error"); } }
}

export class DataFogError extends Error {
  constructor({ code, reason, message, path, findingIndex, inputIndex, markerIssues, reviewConflicts, delivered, complete }) {
    super(message);
    this.name = "DataFogError";
    this.code = code;
    this.reason = reason;
    this.path = path;
    this.findingIndex = findingIndex;
    this.inputIndex = inputIndex;
    this.markerIssues = markerIssues;
    this.reviewConflicts = reviewConflicts;
    this.delivered = delivered;
    this.complete = complete;
  }
}

function normalizeError(error, fallbackCode) {
  if (error instanceof DataFogError) return error;
  try {
    const details = JSON.parse(error?.message ?? "");
    if (typeof details.code === "string" && typeof details.message === "string") {
      return new DataFogError(details);
    }
  } catch {
    // Native conversion errors use the operation-specific fallback below.
  }
  return new DataFogError({
    code: fallbackCode,
    reason: fallbackCode === "invalid_configuration" ? "invalid_type" : undefined,
    message:
      fallbackCode === "invalid_configuration"
        ? "request configuration could not be decoded"
        : "the native operation failed unexpectedly",
    path: fallbackCode === "invalid_configuration" ? "" : undefined,
  });
}

const providerErrorMessages = {
  key_not_found: "key provider could not find the requested key",
  key_access_denied: "key provider denied access to the requested key",
  key_provider_unavailable: "key provider is temporarily unavailable",
  key_provider_error: "key provider could not resolve the requested key",
  token_not_found: "token was not found",
  token_expired: "token has expired",
  token_access_denied: "token access was denied",
  token_provider_unavailable: "token provider is temporarily unavailable",
  token_provider_error: "token provider could not complete the request",
};

function normalizeProviderError(error, path, fallback = "key_provider_error") {
  const candidateCode = error?.code;
  const expectedPrefix = fallback.startsWith("token_") ? "token_" : "key_";
  const code =
    typeof candidateCode === "string" &&
      candidateCode.startsWith(expectedPrefix) &&
      candidateCode in providerErrorMessages
      ? candidateCode
      : fallback;
  return new DataFogError({
    code,
    message: providerErrorMessages[code],
    path,
  });
}

function withPathPrefix(error, prefix) {
  const normalized = normalizeError(error, "internal_error");
  return new DataFogError({
    code: normalized.code,
    reason: normalized.reason,
    message: normalized.message,
    path: normalized.path ? `${prefix}${normalized.path}` : normalized.path,
    findingIndex: normalized.findingIndex,
  });
}

function resolvedKeyInput(selector, response) {
  if (
    !response ||
    !(response.key instanceof Uint8Array) ||
    typeof response.resolvedVersion !== "string"
  ) {
    return {
      selectorIndex: selector.index,
      key: Buffer.alloc(0),
      resolvedVersion: "",
    };
  }
  return {
    selectorIndex: selector.index,
    key: Buffer.from(response.key),
    resolvedVersion: response.resolvedVersion,
  };
}

export class PrivacyManager {
  #keyProvider;
  #tokenProvider;

  constructor(provider, tokenProvider) {
    const options = provider && ("keyProvider" in provider || "tokenProvider" in provider)
      ? provider
      : { keyProvider: provider, tokenProvider };
    if (options.keyProvider && typeof options.keyProvider.resolveKey !== "function") {
      throw new TypeError("key provider must define resolveKey(request)");
    }
    if (options.tokenProvider &&
        (typeof options.tokenProvider.tokenizeBatch !== "function" ||
         typeof options.tokenProvider.restoreBatch !== "function")) {
      throw new TypeError("token provider must define tokenizeBatch(scope, items) and restoreBatch(scope, items)");
    }
    this.#keyProvider = options.keyProvider;
    this.#tokenProvider = options.tokenProvider;
  }

  async #resolve(selectors, pathPrefix = "") {
    const resolved = [];
    if (selectors.length > 0 && !this.#keyProvider) {
      throw new DataFogError({
        code: "key_provider_required",
        message: "pseudonymization requires a runtime key provider",
        path: `${pathPrefix}${selectors[0].path}`,
      });
    }
    for (const selector of selectors) {
      let response;
      try {
        response = await this.#keyProvider.resolveKey({
          keyRef: selector.keyRef,
          keyVersion: selector.keyVersion,
        });
      } catch (error) {
        throw normalizeProviderError(error, `${pathPrefix}${selector.path}`);
      }
      resolved.push(resolvedKeyInput(selector, response));
    }
    return resolved;
  }

  async #tokenize(items, context) {
    if (items.length === 0) return [];
    if (!this.#tokenProvider) {
      throw new DataFogError({
        code: "token_provider_required",
        message: "tokenization requires a runtime token provider and request scope",
      });
    }
    let results;
    try {
      results = await this.#tokenProvider.tokenizeBatch(context.scope, items);
    } catch (error) {
      throw normalizeProviderError(error, undefined, "token_provider_error");
    }
    if (!Array.isArray(results)) return [];
    return results.map((result) => ({
      id: result?.id ?? "",
      payload: result?.payload instanceof Uint8Array ? Buffer.from(result.payload) : Buffer.alloc(0),
      resolvedVersion: typeof result?.resolvedVersion === "string" ? result.resolvedVersion : "",
    }));
  }

  async transform(text, findings, config, context) { return this.#transform(text, findings, config, context); }
  async scanAndTransform(text, config, context) { return this.#scanAndTransform(text, config, context); }
  async restore(text, context) { return this.#restore(text, context); }
  async transformStructured(data, findings, config, context) { return this.#transform(structuredJson(data), structuredOptions(findings), structuredOptions(config), structuredOptions(context), true); }
  async scanAndTransformStructured(data, config, context) { return this.#scanAndTransform(structuredJson(data), structuredOptions(config), structuredOptions(context), true); }
  async restoreStructured(data, context) { return this.#restore(structuredJson(data), structuredOptions(context), true); }

  async #transform(text, findings, config, context, structured = false) {
    if (typeof text !== "string") {
      throw new TypeError("transform text must be a string");
    }
    if (!Array.isArray(findings)) {
      throw new TypeError("transform findings must be an array");
    }
    let resolved = [];
    try {
      const selectors = (structured ? nativeStructuredRequiredKeySelectors : nativeRequiredKeySelectors)(text, findings, config);
      const preparedItems = structured
        ? nativeStructuredRequiredTokenizationItems(text, findings, config, context)
        : undefined;
      resolved = await this.#resolve(selectors);
      const items = preparedItems ?? nativeRequiredTokenizationItems(text, findings, config, context);
      const tokens = await this.#tokenize(items, context);
      const result = (structured ? nativeStructuredTransformWithProviderResults : nativeTransformWithProviderResults)(text, findings, config, context, resolved, tokens);
      return structured ? structuredResult(result) : result;
    } catch (error) {
      if (error instanceof DataFogError) throw error;
      throw normalizeError(error, "invalid_configuration");
    } finally {
      resolved.forEach(({ key }) => key.fill(0));
    }
  }

  async #scanAndTransform(text, config, context, structured = false) {
    if (typeof text !== "string") {
      throw new TypeError("scanAndTransform text must be a string");
    }
    let prepared;
    try {
      prepared = (structured ? nativePrepareStructuredScanAndTransform : nativePrepareScanAndTransform)(text, config);
    } catch (error) {
      throw normalizeError(error, "invalid_configuration");
    }
    let preparedItems;
    if (structured) {
      try {
        preparedItems = nativeStructuredRequiredTokenizationItems(text, prepared.findings, config.transform, context);
      } catch (error) {
        throw withPathPrefix(error, "/transform");
      }
    }
    const resolved = await this.#resolve(prepared.selectors, "/transform");
    try {
      const items = preparedItems ?? nativeRequiredTokenizationItems(text, prepared.findings, config.transform, context);
      const tokens = await this.#tokenize(items, context);
      const result = (structured ? nativeStructuredTransformWithProviderResults : nativeTransformWithProviderResults)(
        text,
        prepared.findings,
        config.transform,
        context,
        resolved,
        tokens,
      );
      return structured ? structuredResult(result) : result;
    } catch (error) {
      throw withPathPrefix(error, "/transform");
    } finally {
      resolved.forEach(({ key }) => key.fill(0));
    }
  }


  async #restore(text, context, structured = false) {
    if (typeof text !== "string") {
      throw new TypeError("restore text must be a string");
    }
    let items;
    try {
      items = (structured ? nativeStructuredRequiredRestoreItems : nativeRequiredRestoreItems)(text, context);
    } catch (error) {
      throw normalizeError(error, "invalid_configuration");
    }
    if (items.length === 0) {
      const result = (structured ? nativeStructuredRestoreWithResults : nativeRestoreWithResults)(text, context, []);
      return structured ? structuredResult(result) : result;
    }
    if (!this.#tokenProvider) {
      throw new DataFogError({
        code: "token_provider_required",
        message: "restoration requires a runtime token provider",
      });
    }
    let results;
    try {
      results = await this.#tokenProvider.restoreBatch(context.scope, items);
    } catch (error) {
      throw normalizeProviderError(error, undefined, "token_provider_error");
    }
    const result = (structured ? nativeStructuredRestoreWithResults : nativeRestoreWithResults)(text, context, Array.isArray(results) ? results : []);
    return structured ? structuredResult(result) : result;
  }
}

export function scan(text, config) {
  if (typeof text !== "string") {
    throw new TypeError("scan text must be a string");
  }

  try {
    return nativeScan(text, config);
  } catch (error) {
    throw normalizeError(
      error,
      config === undefined ? "internal_error" : "invalid_configuration",
    );
  }
}

export function transform(text, findings, config) {
  if (typeof text !== "string") {
    throw new TypeError("transform text must be a string");
  }
  if (!Array.isArray(findings)) {
    throw new TypeError("transform findings must be an array");
  }

  try {
    return nativeTransform(text, findings, config);
  } catch (error) {
    throw normalizeError(error, "invalid_configuration");
  }
}

export function scanAndTransform(text, config) {
  if (typeof text !== "string") {
    throw new TypeError("scanAndTransform text must be a string");
  }

  try {
    return nativeScanAndTransform(text, config);
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
  const json = structuredJson(data);
  try { return nativeDiscoverFields(json, structuredOptions(config)); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

export function scanStructured(data, config) {
  const json = structuredJson(data);
  try { return nativeScanStructured(json, structuredOptions(config)); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

export function transformStructured(data, findings, config) {
  const json = structuredJson(data);
  try { return structuredResult(nativeStructuredTransform(json, structuredOptions(findings), structuredOptions(config))); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}
export function scanAndTransformStructured(data, config) {
  const json = structuredJson(data);
  try { return structuredResult(nativeStructuredScanAndTransform(json, structuredOptions(config))); } catch (error) { throw normalizeError(error, "invalid_configuration"); }
}

function structuredResult({dataJson, ...records}) {
  return {data:JSON.parse(dataJson), ...records};
}

function structuredOptions(value) {
  if (value === undefined) return undefined;
  try { return JSON.parse(structuredJson(value, true)); } catch {
    throw new DataFogError({code:"invalid_configuration", reason:"invalid_type", path:"", message:"structured request options must be JSON-compatible"});
  }
}

const workspacePlans = new WeakMap();
const workspaceFinalizer = new FinalizationRegistry(({state, handle}) => {
  if (!state.closed) state.native.release(handle);
});
const workspaceToken = Symbol("workspace");
function workspacePlan(change) {
  const plan = workspacePlans.get(change);
  if (!plan) throw workspaceFailure("inactive_plan");
  return plan;
}
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
  try {
    const value = state.native[method](...args.map(value => value instanceof Uint8Array ? Buffer.from(value) : value));
    if (value instanceof Uint8Array) {
      try { return new Uint8Array(value); } finally { value.fill(0); }
    }
    return value;
  }
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
    const f = object(value, ["entityType", "matchedText", "range", "confidence", "detectorName", "detectorVersion", "source", "needsReview"]);
    const range = object(f.range, ["unit", "start", "end"]);
    budget += 256; // A conservative bound for keys, punctuation and scalar numbers.
    string(f.entityType); string(f.matchedText); string(range.unit);
    for (const field of ["detectorName", "detectorVersion"]) if (f[field] !== undefined && f[field] !== null) string(f[field]);
    if (f.source !== undefined) string(f.source);
    if (f.needsReview !== undefined && typeof f.needsReview !== "boolean") throw workspaceFailure("invalid_finding");
    if (!Number.isSafeInteger(range.start) || !Number.isSafeInteger(range.end) || (f.confidence !== undefined && f.confidence !== null && (typeof f.confidence !== "number" || !Number.isFinite(f.confidence)))) throw workspaceFailure("invalid_finding");
    if (budget > maximum) throw workspaceFailure("resource_limit");
  }
  try { return structuredJson(findings, true); }
  catch { throw workspaceFailure("invalid_finding"); }
}
function workspacePolicyJson(edits) {
  const maximum = 16 * 1024 * 1024;
  let budget = 2;
  function object(value, fields) {
    if (!value || typeof value !== "object" || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) throw workspaceFailure("invalid_policy");
    const properties = Object.getOwnPropertyDescriptors(value);
    if (Reflect.ownKeys(properties).length !== fields.length) throw workspaceFailure("invalid_policy");
    for (const field of fields) if (!properties[field]?.enumerable || !("value" in properties[field])) throw workspaceFailure("invalid_policy");
    return Object.fromEntries(fields.map(field => [field,properties[field].value]));
  }
  function string(value) {
    if (typeof value !== "string" || !value.isWellFormed()) throw workspaceFailure("invalid_policy");
    if (value.length > 1024 * 1024) throw workspaceFailure("resource_limit");
    budget += new TextEncoder().encode(value).length * 6 + 128;
    if (budget > maximum) throw workspaceFailure("resource_limit");
  }
  const request = object(edits,["rules","choices"]);
  if (!Array.isArray(request.rules) || !Array.isArray(request.choices)) throw workspaceFailure("invalid_policy");
  if (request.rules.length + request.choices.length > 10000) throw workspaceFailure("resource_limit");
  for (const [name,fields] of [["rules",["entityType","rule"]],["choices",["entityType","value","disposition"]]]) {
    const values=request[name];
    if (Reflect.ownKeys(values).length !== values.length+1) throw workspaceFailure("invalid_policy");
    for (let i=0;i<values.length;i++) {
      const property=Object.getOwnPropertyDescriptor(values,String(i));
      if (!property?.enumerable || !("value" in property)) throw workspaceFailure("invalid_policy");
      const row=object(property.value,fields);
      string(row.entityType);
      if (name === "choices") string(row.value);
      const action = name === "rules" ? row.rule : row.disposition;
      if (action !== null) string(action);
    }
  }
  try { return structuredJson(edits,true); } catch { throw workspaceFailure("invalid_policy"); }
}
function workspaceReviewJson(input) {
  function object(value, fields, required) {
    if (!value || typeof value !== "object" || Array.isArray(value) || ![Object.prototype,null].includes(Object.getPrototypeOf(value))) throw workspaceFailure("invalid_finding");
    const props=Object.getOwnPropertyDescriptors(value);
    for (const name of Reflect.ownKeys(props)) if (typeof name !== "string" || !fields.includes(name) || !props[name].enumerable || !("value" in props[name])) throw workspaceFailure("invalid_finding");
    for (const name of required) if (!props[name]) throw workspaceFailure("invalid_finding");
    return Object.fromEntries(Object.entries(props).map(([name,property])=>[name,property.value]));
  }
  function array(value) {
    if (!Array.isArray(value)) throw workspaceFailure("invalid_finding");
    if (value.length>10000) throw workspaceFailure("resource_limit");
    if (Reflect.ownKeys(value).length !== value.length+1) throw workspaceFailure("invalid_finding");
    return value;
  }
  const fields=object(input,["text","scannerFindings","userSelections","corrections"],["text","scannerFindings","userSelections"]);
  workspaceText(fields.text,1024*1024);
  const scanner=array(fields.scannerFindings), user=array(fields.userSelections), corrections=array(fields.corrections === undefined ? [] : fields.corrections);
  if (scanner.length+user.length+corrections.length>10000) throw workspaceFailure("resource_limit");
  const scannerJson=workspaceFindingsJson(scanner), userJson=workspaceFindingsJson(user);
  let budget=new TextEncoder().encode(scannerJson).length+new TextEncoder().encode(userJson).length+new TextEncoder().encode(JSON.stringify(fields.text)).length+128;
  const corrected=[];
  for(let index=0;index<corrections.length;index++) {
    const item=Object.getOwnPropertyDescriptor(corrections,String(index));
    if(!item?.enumerable || !("value" in item)) throw workspaceFailure("invalid_finding");
    const correction=object(item.value,["candidateKey","replacement"],["candidateKey","replacement"]);
    if(typeof correction.candidateKey !== "string" || !/^[a-f0-9]{64}$/.test(correction.candidateKey)) throw workspaceFailure("workspace_conflict");
    const replacement=workspaceFindingsJson([correction.replacement]).slice(1,-1);
    const part=`{"candidateKey":${JSON.stringify(correction.candidateKey)},"replacement":${replacement}}`;
    budget+=new TextEncoder().encode(part).length+2;
    if(budget>16*1024*1024) throw workspaceFailure("resource_limit");
    corrected.push(part);
  }
  if(budget>16*1024*1024) throw workspaceFailure("resource_limit");
  return `{"text":${JSON.stringify(fields.text)},"scannerFindings":${scannerJson},"userSelections":${userJson},"corrections":[${corrected.join(",")}]}`;
}

function workspaceOperation(operation) {
  if (!operation || typeof operation !== "object" || ![Object.prototype, null].includes(Object.getPrototypeOf(operation))) throw workspaceFailure("invalid_operation");
  const properties = Object.getOwnPropertyDescriptors(operation);
  if (Reflect.ownKeys(properties).length !== 2 || !properties.id?.enumerable || !("value" in properties.id) || !properties.occurredAtMs?.enumerable || !("value" in properties.occurredAtMs)) throw workspaceFailure("invalid_operation");
  const id = properties.id.value, time = properties.occurredAtMs.value;
  workspaceBytes(id, 16, true);
  if (!Number.isSafeInteger(time) || time < 0 || time > 253402300799999) throw workspaceFailure("invalid_operation");
  return {id, json: JSON.stringify({id: Array.from(id), occurredAtMs: time})};
}
function conversationRecord(value,fields) {
  if(!value || typeof value!=="object" || ![Object.prototype,null].includes(Object.getPrototypeOf(value))) throw workspaceFailure("invalid_conversation");
  const properties=Object.getOwnPropertyDescriptors(value);
  if(Reflect.ownKeys(properties).length!==fields.length) throw workspaceFailure("invalid_conversation");
  const result={};
  for(const field of fields) {
    if(!properties[field]?.enumerable || !("value" in properties[field])) throw workspaceFailure("invalid_conversation");
    result[field]=properties[field].value;
  }
  return result;
}
function conversationId(value) {
  if(value instanceof Uint8Array) {if(value.length!==16)throw workspaceFailure("invalid_conversation");return Array.from(value);}
  if(!Array.isArray(value) || value.length!==16 || Reflect.ownKeys(value).length!==17) throw workspaceFailure("invalid_conversation");
  const bytes=[];
  for(let i=0;i<16;i++) {
    const item=Object.getOwnPropertyDescriptor(value,String(i));
    if(!item?.enumerable || !("value" in item) || !Number.isInteger(item.value) || item.value<0 || item.value>255) throw workspaceFailure("invalid_conversation");
    bytes.push(item.value);
  }
  return bytes;
}
function conversationText(value) {
  if(typeof value!=="string" || !value.isWellFormed())throw workspaceFailure("invalid_conversation");
  workspaceText(value,16*1024*1024);
}
function conversationInput(value) {
  const fields=conversationRecord(value,["id","surface","siteId","siteIdByteBudget"]);
  fields.id=conversationId(fields.id);
  conversationText(fields.surface);
  if(fields.siteId!==null) conversationText(fields.siteId);
  if(!Number.isSafeInteger(fields.siteIdByteBudget) || fields.siteIdByteBudget<1 || fields.siteIdByteBudget>16*1024*1024) throw workspaceFailure("invalid_conversation");
  return fields;
}
function sendIntentJson(value) {
  let fields;
  try {
    fields=conversationRecord(value,["preparationId","inputIndex"]);
    fields.preparationId=conversationId(fields.preparationId);
    if(!Number.isSafeInteger(fields.inputIndex) || fields.inputIndex<0 || fields.inputIndex>=32) throw workspaceFailure("invalid_send_intent");
  } catch {throw workspaceFailure("invalid_send_intent");}
  return JSON.stringify(fields);
}
class PreparedWorkspaceChange {
  constructor(state, handle) { workspacePlans.set(this, { state, handle }); workspaceFinalizer.register(this, {state, handle}, this); Object.freeze(this); }
  sendIntents() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state,"send_intents",handle)); }
  snapshotFormat() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state,"snapshot_format",handle)); }
  policyPreview() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state, "policy_preview", handle)); }
  previews() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state, "previews", handle)); }
  preview() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state, "preview", handle)); }
  heads() { const {state, handle} = workspacePlan(this); return JSON.parse(workspaceCall(state, "plan_heads", handle)); }
  exportCandidateBundle() { const {state, handle} = workspacePlan(this); return workspaceCall(state, "export_candidate_bundle", handle); }
  discard() { const {state, handle} = workspacePlan(this); workspaceCall(state, "discard", handle); workspaceFinalizer.unregister(this); }
}

// The native child owns parser state. Transport copies cannot mutate caller buffers.
class WorkspaceRestoreStream {
  #native;
  constructor(native) { this.#native = native; Object.freeze(this); }
  #call(method, ...args) {
    try { const result = JSON.parse(this.#native[method](...args)); return result === null ? undefined : result; }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  feed(bytes) {
    if (!(bytes instanceof Uint8Array)) return this.#call("reject_encoding");
    let copied;
    try { copied = Buffer.from(bytes); } catch { return this.#call("reject_encoding"); }
    try { return this.#call("feed", copied); } finally { copied.fill(0); }
  }
  feedUtf16(units) {
    if (!(units instanceof Uint16Array)) return this.#call("reject_encoding");
    let copied;
    try { copied = new Uint16Array(units); } catch { return this.#call("reject_encoding"); }
    try { return this.#call("feed_utf16", copied); } finally { copied.fill(0); }
  }
  finalize() { return this.#call("finalize"); }
  reset() { this.#call("reset"); }
  close() { this.#native.close(); }
}

/** Provisional reversible-workspace API. Credentials and durable storage remain host-owned. */
export class Workspace {
  #state;
  constructor(token, native) {
    if (token !== workspaceToken) throw workspaceFailure("invalid_configuration");
    this.#state = { native, closed: false };
  }
  static create(namespace, operationId) {
    workspaceBytes(namespace, 16, true); workspaceBytes(operationId, 16, true);
    try { return new Workspace(workspaceToken, NativeWorkspace.create(Buffer.from(namespace), Buffer.from(operationId))); }
    catch (error) { throw normalizeError(error, "internal_error"); }
  }
  static createWithOperation(namespace, context) {
    workspaceBytes(namespace,16,true);
    const operation=workspaceOperation(context);
    try {return new Workspace(workspaceToken,NativeWorkspace.create_with_operation(Buffer.from(namespace),operation.json));}
    catch(error){throw normalizeError(error,"internal_error");}
  }
  static openBundle(bytes) {
    workspaceBytes(bytes, 16 * 1024 * 1024);
    try { return new Workspace(workspaceToken, NativeWorkspace.open_bundle(Buffer.from(bytes))); }
    catch (error) { throw normalizeError(error, "invalid_snapshot"); }
  }
  head() { return JSON.parse(workspaceCall(this.#state, "head")); }
  namespace() { return workspaceCall(this.#state, "namespace"); }
  exportBundle() { return workspaceCall(this.#state, "export_bundle"); }
  operations() { return JSON.parse(workspaceCall(this.#state, "operations")); }
  operationsPage(offset, limit) {
    if (!Number.isSafeInteger(offset) || offset < 0 || offset > 100000 || !Number.isSafeInteger(limit) || limit < 1 || limit > 100000) throw workspaceFailure("resource_limit");
    return JSON.parse(workspaceCall(this.#state, "operations_page", offset, limit));
  }
  counts() { return JSON.parse(workspaceCall(this.#state, "counts")); }
  historyCoverage() { return JSON.parse(workspaceCall(this.#state, "history_coverage")); }
  policy() { return JSON.parse(workspaceCall(this.#state,"policy")); }
  preparePolicyChanges(edits, operationId) {
    workspaceBytes(operationId,16,true);
    let json;
    try { json=workspacePolicyJson(edits); }
    catch (error) { if (error instanceof DataFogError) throw error; throw workspaceFailure("invalid_policy"); }
    return new PreparedWorkspaceChange(this.#state,workspaceCall(this.#state,"prepare_policy_changes",json,operationId));
  }
  recall(text) { workspaceText(text, 1024 * 1024); return JSON.parse(workspaceCall(this.#state, "recall", text)); }
  /** Read-only review; returned candidates are never handoff permission. */
  review(input) { return JSON.parse(workspaceCall(this.#state,"review",workspaceReviewJson(input))); }
  prepareReview(inputs, operationId) {
    workspaceBytes(operationId,16,true);
    if(!Array.isArray(inputs) || inputs.length===0) throw workspaceFailure("invalid_batch");
    if(inputs.length>32) throw workspaceFailure("resource_limit");
    if(Reflect.ownKeys(inputs).length!==inputs.length+1) throw workspaceFailure("invalid_batch");
    const parts=[];let budget=2;
    for(let inputIndex=0;inputIndex<inputs.length;inputIndex++) {
      try {
        const item=Object.getOwnPropertyDescriptor(inputs,String(inputIndex));
        if(!item?.enumerable || !("value" in item)) throw workspaceFailure("invalid_batch");
        const part=workspaceReviewJson(item.value);budget+=new TextEncoder().encode(part).length+1;
        if(budget>16*1024*1024) throw workspaceFailure("resource_limit");
        parts.push(part);
      } catch(error) { const failure=error instanceof DataFogError?error:workspaceFailure("invalid_finding");failure.inputIndex=inputIndex;throw failure; }
    }
    return new PreparedWorkspaceChange(this.#state,workspaceCall(this.#state,"prepare_review",`[${parts.join(",")}]`,operationId));
  }
  prepareConversationSend(inputs, operation) {
    const context=workspaceOperation(operation);
    if(!Array.isArray(inputs) || inputs.length===0 || Reflect.ownKeys(inputs).length!==inputs.length+1) throw workspaceFailure("invalid_batch");
    if(inputs.length>32) throw workspaceFailure("resource_limit");
    const parts=[];let budget=2;
    for(let index=0;index<inputs.length;index++) {
      try {
        const item=Object.getOwnPropertyDescriptor(inputs,String(index));
        if(!item?.enumerable || !("value" in item)) throw workspaceFailure("invalid_batch");
        const row=conversationRecord(item.value,["input","conversation"]);
        const part=`{"input":${workspaceReviewJson(row.input)},"conversation":${JSON.stringify(conversationInput(row.conversation))}}`;
        budget+=new TextEncoder().encode(part).length+1;
        if(budget>16*1024*1024) throw workspaceFailure("resource_limit");parts.push(part);
      } catch(error) {const failure=error instanceof DataFogError?error:workspaceFailure("invalid_conversation");failure.inputIndex=index;throw failure;}
    }
    return new PreparedWorkspaceChange(this.#state,workspaceCall(this.#state,"prepare_conversation_send",`[${parts.join(",")}]`,context.json));
  }
  conversation(logicalId) {workspaceBytes(logicalId,16,true);return JSON.parse(workspaceCall(this.#state,"conversation",logicalId));}
  conversationForSite(surface,siteId) {
    conversationText(surface);conversationText(siteId);
    return workspaceCall(this.#state,"conversation_for_site",surface,siteId);
  }
  sendIntentStatus(intent) {return JSON.parse(workspaceCall(this.#state,"send_intent_status",sendIntentJson(intent)));}
  prepareObservedSend(intent,logicalId,surface,siteId,operation) {
    workspaceBytes(logicalId,16,true);conversationText(surface);conversationText(siteId);
    const context=workspaceOperation(operation);
    return new PreparedWorkspaceChange(this.#state,workspaceCall(this.#state,"prepare_observed_send",sendIntentJson(intent),logicalId,surface,siteId,context.json));
  }
  prepareDefinitelyNotSent(intent,logicalId,operation) {
    workspaceBytes(logicalId,16,true);const context=workspaceOperation(operation);
    return new PreparedWorkspaceChange(this.#state,workspaceCall(this.#state,"prepare_definitely_not_sent",sendIntentJson(intent),logicalId,context.json));
  }
  restoreInConversation(text,logicalId) {workspaceText(text,16*1024*1024);workspaceBytes(logicalId,16,true);return workspaceCall(this.#state,"restore_in_conversation",text,logicalId);}
  restoreReportInConversation(text,logicalId) {workspaceText(text,16*1024*1024);workspaceBytes(logicalId,16,true);return JSON.parse(workspaceCall(this.#state,"restore_report_in_conversation",text,logicalId));}
  bufferedRestoreInConversation(logicalId,encoding="utf8") {
    workspaceBytes(logicalId,16,true);if(encoding!=="utf8" && encoding!=="utf16") throw workspaceFailure("invalid_encoding");
    return new WorkspaceRestoreStream(workspaceCall(this.#state,"buffered_restore_in_conversation",logicalId,encoding));
  }
  progressiveRestoreInConversation(logicalId,encoding="utf8") {
    workspaceBytes(logicalId,16,true);if(encoding!=="utf8" && encoding!=="utf16") throw workspaceFailure("invalid_encoding");
    return new WorkspaceRestoreStream(workspaceCall(this.#state,"progressive_restore_in_conversation",logicalId,encoding));
  }
  prepareRecovery(operationId) {
    workspaceBytes(operationId, 16, true);
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_recovery", operationId));
  }
  prepareRecoveryWithOperation(context) {
    const operation = workspaceOperation(context);
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_recovery", operation.id, operation.json));
  }
  prepareWithOperation(text, findings, context) { return this.prepareBatchWithOperation([{text, findings}], context); }
  prepareOutcome(outcome, context) {
    const operation = workspaceOperation(context);
    if (!outcome || typeof outcome !== "object" || ![Object.prototype, null].includes(Object.getPrototypeOf(outcome))) throw workspaceFailure("invalid_operation");
    const fields = Object.getOwnPropertyDescriptors(outcome);
    if (Reflect.ownKeys(fields).length !== 2 || !fields.kind?.enumerable || !("value" in fields.kind) || !fields.issueCount?.enumerable || !("value" in fields.issueCount)) throw workspaceFailure("invalid_operation");
    const kind = fields.kind.value, issueCount = fields.issueCount.value;
    if (typeof kind !== "string" || kind.length > 32 || !Number.isSafeInteger(issueCount) || issueCount < 0 || issueCount > 10000) throw workspaceFailure("invalid_operation");
    const json = JSON.stringify({kind, issueCount});
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_outcome", json, operation.json));
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
  prepareBatch(inputs,operationId) {return this.#prepareBatch(inputs,operationId);}
  prepareBatchWithOperation(inputs,context) {
    const operation=workspaceOperation(context);
    return this.#prepareBatch(inputs,operation.id,operation.json);
  }
  #prepareBatch(inputs, operationId, context) {
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
    return new PreparedWorkspaceChange(this.#state, workspaceCall(this.#state, "prepare_batch", `[${parts.join(",")}]`, operationId, context));
  }
  activateSaved(change, expectedHead, savedHead) {
    const plan = workspacePlans.get(change);
    if (!plan || plan.state !== this.#state) throw workspaceFailure("workspace_conflict");
    let expected, saved;
    try { expected = structuredJson(expectedHead); saved = structuredJson(savedHead); }
    catch { throw workspaceFailure("workspace_conflict"); }
    workspaceCall(this.#state, "activate_saved", plan.handle, expected, saved);
    workspaceFinalizer.unregister(change);
  }
  /** Strict buffered default: feeds release no text. */
  bufferedRestore(encoding = "utf8") {
    if (encoding !== "utf8" && encoding !== "utf16") throw workspaceFailure("invalid_encoding");
    return new WorkspaceRestoreStream(workspaceCall(this.#state,"buffered_restore",encoding));
  }
  /** Explicit irreversible delivery; terminal failures preserve prior delivered counts. */
  progressiveRestore(encoding = "utf8") {
    if (encoding !== "utf8" && encoding !== "utf16") throw workspaceFailure("invalid_encoding");
    return new WorkspaceRestoreStream(workspaceCall(this.#state,"progressive_restore",encoding));
  }
  restore(text) { workspaceText(text, 16 * 1024 * 1024); return workspaceCall(this.#state, "restore", text); }
  restoreReport(text) { workspaceText(text, 16 * 1024 * 1024); return JSON.parse(workspaceCall(this.#state, "restore_report", text)); }
  close() {
    if (!this.#state.closed) {
      workspaceCall(this.#state, "close");
      this.#state.closed = true;
    }
  }
}

export function workspaceByteRangeFromUtf16(text, start, end) {
  workspaceText(text, 1024 * 1024);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < 0 || start > text.length || end > text.length) throw workspaceFailure("invalid_finding");
  try { return JSON.parse(NativeWorkspace.byte_range_from_utf16(text, start, end)); }
  catch (error) { throw normalizeError(error, "invalid_finding"); }
}
