import type { ScanConfig } from "../index.js";
import type { ModelDescriptor, ModelScanResult } from "../models/index.js";
export interface CnnSessionOptions {
  /** Enabled by default. False skips weight authentication/decoding and inference. */
  cnnEnabled?: boolean;
  /** Local module Worker asset; defaults to the packaged worker.js. */
  workerUrl?: string | URL;
  /** Startup and per-scan deadline, 1..120000 ms; default 30000. */
  timeoutMs?: number;
}
export class CnnSession {
  private constructor();
  static open(options?: CnnSessionOptions): Promise<CnnSession>;
  readonly status: "loading" | "ready" | "disabled" | "error" | "closed";
  readonly cnnEnabled: boolean;
  readonly generation: number;
  describe(): ModelDescriptor | undefined;
  setEnabled(cnnEnabled: boolean): Promise<void>;
  scan(text: string, config?: ScanConfig): Promise<Omit<ModelScanResult, "model"> & {model: string | null}>;
  scanModel(text: string): Promise<Omit<ModelScanResult, "model"> & {model: string | null}>;
  close(): void;
}
