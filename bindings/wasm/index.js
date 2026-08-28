import initWasm, {
  scan as scanWasm,
  scan_and_transform as scanAndTransformWasm,
  transform as transformWasm,
} from "./dist/datafog_wasm.js";

let initialization;
let initialized = false;

export function init() {
  if (!initialization) {
    initialization = initWasm(
      new URL("./dist/datafog_wasm_bg.wasm", import.meta.url),
    )
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
