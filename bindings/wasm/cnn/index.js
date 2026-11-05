import { DataFogError } from "../index.js";

const error = (code, reason) => new DataFogError({code, reason, path: "/cnn", message: "embedded CNN session is unavailable"});
function boolean(value) { if (typeof value !== "boolean") throw new TypeError("cnnEnabled must be boolean"); }
function textInput(text) {
  if (typeof text !== "string") throw new TypeError("scan text must be a string");
  if (text.length > 65536 || new TextEncoder().encode(text).length > 65536) throw error("model_resource_exhausted", "limit_exceeded");
  // Reject lone surrogates before the JS-to-WASM conversion replaces them.
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw error("invalid_input", "invalid_unicode");
    } else if (c >= 0xdc00 && c <= 0xdfff) throw error("invalid_input", "invalid_unicode");
  }
}

/** Bundled default-on statements CNN. All inference runs inside a dedicated Worker. */
export class CnnSession {
  #worker;
  #pending;
  #next = 0;
  #state = "loading";
  #enabled;
  #generation = 0;
  #descriptor;
  #workerUrl;
  #timeoutMs;

  static async open({cnnEnabled = true, workerUrl = new URL("./worker.js", import.meta.url), timeoutMs = 30000} = {}) {
    boolean(cnnEnabled);
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000) throw new RangeError("timeoutMs must be 1..120000");
    const session = new CnnSession();
    session.#enabled = cnnEnabled;
    session.#workerUrl = workerUrl;
    session.#timeoutMs = timeoutMs;
    await session.#start();
    return session;
  }
  get status() { return this.#state; }
  get cnnEnabled() { return this.#enabled; }
  get generation() { return this.#generation; }
  describe() { return this.#descriptor ? structuredClone(this.#descriptor) : undefined; }

  async #start() {
    try {
      this.#worker = new Worker(this.#workerUrl, {type: "module"});
      this.#worker.onmessage = ({data}) => {
        const pending = this.#pending;
        if (!pending || pending.id !== data.id) return;
        clearTimeout(pending.timer);
        this.#pending = undefined;
        if (data.error) pending.reject(new DataFogError(data.error));
        else pending.resolve(data.result);
      };
      this.#worker.onerror = () => this.#fail(error("model_inference_failed", "worker_failed"));
      this.#worker.onmessageerror = () => this.#fail(error("model_inference_failed", "invalid_output"));
      this.#descriptor = await this.#request("open", {cnnEnabled:this.#enabled});
      this.#state = this.#enabled ? "ready" : "disabled";
    } catch (cause) {
      if (this.#state !== "closed") this.#state = "error";
      this.#worker?.terminate();
      throw cause instanceof DataFogError ? cause : error("model_load_failed", "worker_failed");
    }
  }
  #request(operation, payload) {
    if (this.#pending) return Promise.reject(error("model_session_busy", "busy"));
    return new Promise((resolve,reject) => {
      const id = ++this.#next;
      const timer = setTimeout(() => this.#fail(error("model_timeout", "deadline_exceeded")),this.#timeoutMs);
      this.#pending = {id,resolve,reject,timer};
      try { this.#worker.postMessage({id,operation,...payload}); }
      catch { this.#fail(error("invalid_configuration", "invalid_type")); }
    });
  }
  #fail(cause) {
    const pending = this.#pending;
    this.#pending = undefined;
    if (pending) { clearTimeout(pending.timer); pending.reject(cause); }
    this.#worker?.terminate();
    this.#worker = undefined;
    this.#descriptor = undefined;
    if (this.#state !== "closed") this.#state = "error";
  }
  /** Invalidates previous requests immediately. Host must also invalidate Workspace reviews. */
  async setEnabled(cnnEnabled) {
    boolean(cnnEnabled);
    if (this.#state === "closed") throw error("model_session_closed", "closed");
    if (this.#state === "loading") throw error("model_session_busy", "busy");
    if (cnnEnabled === this.#enabled && (this.#state === "ready" || this.#state === "disabled")) return;
    this.#generation++;
    this.#fail(error("model_request_cancelled", "configuration_changed"));
    this.#enabled = cnnEnabled;
    this.#state = "loading";
    await this.#start();
  }
  async scan(text, config) {
    textInput(text);
    if (this.#state === "closed") throw error("model_session_closed", "closed");
    if (this.#state !== "ready" && this.#state !== "disabled") throw error("model_unavailable", "not_ready");
    return this.#request("scan", {text,config});
  }
  async scanModel(text) {
    textInput(text);
    if (this.#state === "closed") throw error("model_session_closed", "closed");
    if (this.#state !== "ready" && this.#state !== "disabled") throw error("model_unavailable", "not_ready");
    return this.#request("scan", {text,modelOnly:true});
  }
  close() {
    if (this.#state === "closed") return;
    this.#state = "closed";
    this.#generation++;
    this.#fail(error("model_session_closed", "closed"));
  }
}
