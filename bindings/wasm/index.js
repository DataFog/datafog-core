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

export function scan(text) {
  if (typeof text !== "string") {
    throw new TypeError("scan text must be a string");
  }

  if (!initialized) {
    throw new Error("Call and await init() before scan().");
  }

  return scanWasm(text);
}

function assertInitialized(operation) {
  if (!initialized) {
    throw new Error(`Call and await init() before ${operation}().`);
  }
}

function assertStrategy(strategy) {
  if (strategy !== "redact") {
    throw new TypeError("strategy must be 'redact'");
  }
}

export function transform(text, findings, strategy) {
  if (typeof text !== "string") {
    throw new TypeError("transform text must be a string");
  }
  if (!Array.isArray(findings)) {
    throw new TypeError("transform findings must be an array");
  }
  assertStrategy(strategy);
  assertInitialized("transform");

  try {
    return transformWasm(text, findings, strategy);
  } catch (error) {
    throw error instanceof Error ? error : new Error(String(error));
  }
}

export function scanAndTransform(text, strategy) {
  if (typeof text !== "string") {
    throw new TypeError("scanAndTransform text must be a string");
  }
  assertStrategy(strategy);
  assertInitialized("scanAndTransform");

  try {
    return scanAndTransformWasm(text, strategy);
  } catch (error) {
    throw error instanceof Error ? error : new Error(String(error));
  }
}
