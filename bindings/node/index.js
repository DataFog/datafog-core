import { Buffer } from "node:buffer";
import {
  prepareScanAndTransform as nativePrepareScanAndTransform,
  requiredKeySelectors as nativeRequiredKeySelectors,
  scan as nativeScan,
  scanAndTransform as nativeScanAndTransform,
  transform as nativeTransform,
  transformWithResolvedKeys as nativeTransformWithResolvedKeys,
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
};

function normalizeProviderError(error, path) {
  const candidateCode = error?.code;
  const code =
    typeof candidateCode === "string" && candidateCode in providerErrorMessages
      ? candidateCode
      : "key_provider_error";
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

function assertProvider(provider) {
  if (!provider || typeof provider.resolveKey !== "function") {
    throw new TypeError("PrivacyManager provider must define resolveKey(request)");
  }
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
  #provider;

  constructor(provider) {
    assertProvider(provider);
    this.#provider = provider;
  }

  async #resolve(selectors, pathPrefix = "") {
    const resolved = [];
    for (const selector of selectors) {
      let response;
      try {
        response = await this.#provider.resolveKey({
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

  async transform(text, findings, config) {
    if (typeof text !== "string") {
      throw new TypeError("transform text must be a string");
    }
    if (!Array.isArray(findings)) {
      throw new TypeError("transform findings must be an array");
    }
    let resolved = [];
    try {
      const selectors = nativeRequiredKeySelectors(text, findings, config);
      resolved = await this.#resolve(selectors);
      return nativeTransformWithResolvedKeys(text, findings, config, resolved);
    } catch (error) {
      if (error instanceof DataFogError) throw error;
      throw normalizeError(error, "invalid_configuration");
    } finally {
      resolved.forEach(({ key }) => key.fill(0));
    }
  }

  async scanAndTransform(text, config) {
    if (typeof text !== "string") {
      throw new TypeError("scanAndTransform text must be a string");
    }
    let prepared;
    try {
      prepared = nativePrepareScanAndTransform(text, config);
    } catch (error) {
      throw normalizeError(error, "invalid_configuration");
    }
    const resolved = await this.#resolve(prepared.selectors, "/transform");
    try {
      return nativeTransformWithResolvedKeys(
        text,
        prepared.findings,
        config.transform,
        resolved,
      );
    } catch (error) {
      throw withPathPrefix(error, "/transform");
    } finally {
      resolved.forEach(({ key }) => key.fill(0));
    }
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
