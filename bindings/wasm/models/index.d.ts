import type { Finding, ScanConfig } from "../index.js";

export interface ModelSessionOptions {
  /** Base URL of the pinned tokenizer files and onnx/model.onnx; assets are hash verified. */
  readonly modelBaseUrl: string | URL;
  /** Optional URL of ONNX Runtime Web's matching WASM assets. */
  readonly wasmPaths?: string;
  /** 1–4; cross-origin isolation is required for more than one thread. */
  readonly numThreads?: number;
  /** Model prompt → canonical entity type. Defaults to the seven documented Small prompts. */
  readonly labels?: Readonly<Record<string, string>>;
  /** Raw pair sigmoid cutoff, greater than zero and at most one; defaults to 0.5. */
  readonly threshold?: number;
}

export interface SourceEvidence {
  readonly detector_name: string;
  readonly detector_version?: string;
  readonly native_label?: string;
  readonly raw_score?: { readonly value: number; readonly scale_id: string };
  readonly calibrated_confidence?: number;
}

export interface ModelScanResult {
  readonly contractVersion: 1;
  readonly model: string;
  readonly findings: Finding[];
  readonly evidence: readonly {
    readonly finding_index: number;
    readonly sources: readonly SourceEvidence[];
  }[];
}

/** Experimental English-only model session; does not change the synchronous rule scan API. */
export declare class ModelSession {
  private constructor();
  static open(options: ModelSessionOptions): Promise<ModelSession>;
  /** Routes rule-supported entities to Core rules and other configured entities to Small. */
  scan(text: string, config?: ScanConfig): Promise<ModelScanResult>;
  scanModel(text: string): Promise<ModelScanResult>;
  /** Idempotent after success; rejects while inference is active. */
  close(): Promise<void>;
}
