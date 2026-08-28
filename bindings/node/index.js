import {
  scan as nativeScan,
  scanAndTransform as nativeScanAndTransform,
  transform as nativeTransform,
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
