import { init } from "../index.js";
import { EmbeddedCnnDetector } from "../dist/datafog_wasm.js";

let detector;
let ready = false;
function safeError(error) {
  try {
    const parsed = JSON.parse(typeof error === "string" ? error : "");
    if (typeof parsed.code === "string" && typeof parsed.reason === "string")
      return { code: parsed.code, reason: parsed.reason, path: parsed.path, message: "embedded CNN operation failed" };
  } catch { /* Never return source text or unsanitized runtime details. */ }
  return { code: "model_inference_failed", reason: "operation_failed", path: "/cnn", message: "embedded CNN operation failed" };
}
self.onmessage = async ({data}) => {
  const {id, operation} = data;
  try {
    if (operation === "open") {
      await init();
      detector = data.cnnEnabled ? new EmbeddedCnnDetector() : undefined;
      ready = true;
      self.postMessage({id, result: detector ? JSON.parse(detector.describe()) : undefined});
    } else if (operation === "scan" && ready) {
      const result = detector ? detector.scan(data.text, data.config ?? {}, data.modelOnly === true) :
        EmbeddedCnnDetector.scan_disabled(data.text, data.modelOnly ? {entities:["PERSON","ADDRESS"]} : (data.config ?? {}));
      if (!detector) result.model = null;
      self.postMessage({id, result});
    } else throw new Error("invalid worker operation");
  } catch (error) { self.postMessage({id, error: safeError(error)}); }
};
