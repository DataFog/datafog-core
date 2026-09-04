import { Buffer } from "node:buffer";
import {
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

export class DataFogError extends Error {
  constructor({ code, reason, message, path, findingIndex }) {
    super(message);
    this.name = "DataFogError";
    this.code = code;
    this.reason = reason;
    this.path = path;
    this.findingIndex = findingIndex;
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
