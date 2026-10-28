import { init, DataFogError } from "../index.js";
import { BrowserModelContract } from "../dist/datafog_wasm.js";

function modelError(error, code = "model_inference_failed", path = "/backend") {
  if (error instanceof DataFogError) return error;
  try {
    const details = typeof error === "string" ? JSON.parse(error) : undefined;
    if (
      typeof details?.code === "string" &&
      typeof details.message === "string"
    ) {
      return new DataFogError(details);
    }
  } catch {
    // Inference runtime errors are sanitized; they may contain source text.
  }
  return new DataFogError({
    code,
    reason: "operation_failed",
    path,
    message: "browser model operation failed",
  });
}

function stateError(code, reason) {
  return new DataFogError({
    code,
    reason,
    path: "/backend",
    message: "browser model session is unavailable",
  });
}

/** Explicit, optional English GLiNER2.5 Small inference. No runtime is loaded on import. */
export class ModelSession {
  #contract;
  #runtime;
  #labels;
  #threshold;
  #descriptor;
  #busy = false;
  #closed = false;
  #closing;

  static async open({
    modelBaseUrl,
    wasmPaths,
    numThreads = 4,
    labels,
    threshold = 0.5,
  } = {}) {
    if (!modelBaseUrl) throw new TypeError("modelBaseUrl is required");
    if (!Number.isFinite(threshold) || threshold <= 0 || threshold > 1)
      throw new RangeError("threshold must be greater than 0 and at most 1");
    await init();
    let contract;
    try {
      contract = new BrowserModelContract(labels);
      const descriptor = contract.describe();
      const nativeLabels = contract.labels();
      // Validate prompts before fetching assets, then load the optional runtime.
      const { validateLabels } = await import("./model.mjs");
      validateLabels(nativeLabels);
      const { GlinerSession } = await import("./session.mjs");
      const runtime = await GlinerSession.create({
        modelBaseUrl,
        wasmPaths,
        numThreads,
      });
      return new ModelSession(contract, runtime, nativeLabels, threshold, descriptor);
    } catch (error) {
      contract?.free();
      throw modelError(error, "model_load_failed", "/bundle");
    }
  }

  constructor(contract, runtime, labels, threshold, descriptor) {
    this.#contract = contract;
    this.#runtime = runtime;
    this.#labels = labels;
    this.#threshold = threshold;
    this.#descriptor = descriptor;
  }

  /** Configured metadata; independent of runtime readiness or session state. */
  describe() {
    return structuredClone(this.#descriptor);
  }

  /** Rules own supported types; inference runs only when model-only types are requested. */
  async scan(text, config = {}) {
    return this.#scan(text, config);
  }

  /** Model-only output for this session's prompt labels, including rule-supported types. */
  async scanModel(text) {
    return this.#scan(text, undefined);
  }

  async #scan(text, config) {
    if (this.#closed) throw stateError("model_session_closed", "closed");
    if (typeof text !== "string" || !text.isWellFormed())
      throw new TypeError("text must be a well-formed Unicode string");
    try {
      // Core validates input and configuration before inference; it also validates model-only input.
      const requestConfig =
        config === undefined ? undefined : structuredClone(config);
      const needsModel = this.#contract.needs_model(
        text,
        requestConfig === undefined ? {} : requestConfig,
      );
      const runModel = config === undefined || needsModel;
      if (runModel && this.#busy)
        throw stateError("model_session_busy", "busy");
      if (runModel) this.#busy = true;
      try {
        const entities = runModel
          ? (
              await this.#runtime.extract(text, {
                labels: this.#labels,
                threshold: this.#threshold,
              })
            ).entities
          : [];
        const spans = entities.map((entity) => ({
          native_label: entity.label,
          start: entity.byteRange.start,
          end: entity.byteRange.end,
          coordinate_unit: "utf8_byte",
          raw_score: { value: entity.score, scale_id: "gliner25_pair_sigmoid" },
          calibrated_confidence: null,
          matched_text: entity.text,
        }));
        return this.#contract.complete(text, spans, requestConfig);
      } finally {
        if (runModel) this.#busy = false;
      }
    } catch (error) {
      throw modelError(error);
    }
  }

  async close() {
    if (this.#closing) return this.#closing;
    if (this.#busy) throw stateError("model_session_busy", "busy");
    this.#closed = true;
    this.#closing = (async () => {
      try {
        await this.#runtime.close();
      } catch (error) {
        // Keep the runtime reachable so close() can retry a failed release.
        this.#closing = undefined;
        throw modelError(error, "model_cleanup_failed");
      } finally {
        this.#contract?.free();
        this.#contract = undefined;
      }
    })();
    return this.#closing;
  }
}
