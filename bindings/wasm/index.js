import initWasm, { scan as scanWasm } from "./dist/datafog_wasm.js";

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
