/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;
export type TransformationStrategy = "redact" | "mask" | "remove" | "tokenize";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationStrategyConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
  | { readonly strategy: "tokenize"; readonly token_ref: string }
  | {
      readonly strategy: "mask";
      readonly character?: string;
      readonly reveal?: MaskRevealConfig;
    };

export interface RegexAllowRule {
  readonly pattern: string;
  readonly case_sensitive?: boolean;
}

export interface AllowConfig {
  readonly exact?: Readonly<Record<EntityType, readonly string[]>>;
  readonly regex?: Readonly<Record<EntityType, readonly RegexAllowRule[]>>;
}

export interface TransformationConfig {
  readonly default: TransformationStrategyConfig;
  readonly entities?: readonly EntityType[];
  readonly overrides?: Readonly<Record<EntityType, TransformationStrategyConfig>>;
  readonly allow?: AllowConfig;
}

export interface ScanConfig {
  /** Nonempty unique text entity types to run; omission runs all activated detectors.
   * Selection does not activate locale-dependent types or UUID detection. */
  readonly entities?: readonly EntityType[];
  /** Source syntax for email boundaries; defaults to text. */
  readonly format?: "text" | "env" | "sql";
  readonly locale?: string;
  /** Opt-in canonical UUID detection; disabled by default. */
  readonly detect_uuid?: boolean;
}

export interface ScanAndTransformConfig {
  readonly scan?: ScanConfig;
  readonly transform: TransformationConfig;
}

export type DataFogErrorCode =
  | "invalid_configuration"
  | "invalid_batch"
  | "invalid_finding"
  | "closed_workspace"
  | "inactive_plan"
  | "workspace_conflict"
  | "invalid_marker"
  | "invalid_snapshot"
  | "invalid_operation_id"
  | "resource_limit"
  | "key_provider_required"
  | "key_not_found"
  | "key_access_denied"
  | "key_provider_unavailable"
  | "invalid_key_material"
  | "key_provider_error"
  | "token_provider_required"
  | "invalid_token"
  | "unsupported_token_version"
  | "token_not_found"
  | "token_expired"
  | "token_access_denied"
  | "invalid_token_material"
  | "token_provider_unavailable"
  | "token_provider_error"
  | "unsupported_strategy"
  | "model_load_failed"
  | "model_inference_failed"
  | "model_resource_exhausted"
  | "model_session_busy"
  | "model_session_closed"
  | "model_cleanup_failed"
  | "internal_error";

export declare class DataFogError extends Error {
  readonly code: DataFogErrorCode;
  readonly reason?: string;
  readonly path?: string;
  readonly findingIndex?: number;
  readonly inputIndex?: number;
  /** Content-free source issue ranges when workspace preparation rejects marker text. */
  readonly markerIssues?: readonly WorkspaceMarkerIssue[];
}

export interface TextRange {
  readonly start: number;
  readonly end: number;
}

export interface FindingInput {
  readonly entityType: EntityType;
  readonly matchedText: string;
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly confidence?: number;
  readonly detectorName: string;
  readonly detectorVersion?: string;
}

export interface Finding extends FindingInput {
  readonly utf16Range: TextRange;
}

export interface Transformation {
  readonly entityType: EntityType;
  readonly sourceByteRange: TextRange;
  readonly sourceCodepointRange: TextRange;
  readonly sourceUtf16Range: TextRange;
  readonly confidence?: number;
  readonly detectorName: string;
  readonly detectorVersion?: string;
  readonly strategy: TransformationStrategy;
  readonly replacement: string;
  readonly outputByteRange: TextRange;
  readonly outputCodepointRange: TextRange;
  readonly outputUtf16Range: TextRange;
  readonly keyRef?: string;
  readonly resolvedKeyVersion?: string;
  readonly tokenRef?: string;
  readonly resolvedTokenVersion?: string;
}

export interface TransformResult {
  readonly text: string;
  readonly transformations: Transformation[];
}

export function init(): Promise<void>;
export function scan(text: string, config?: ScanConfig): Finding[];
export function transform(
  text: string,
  findings: FindingInput[],
  config: TransformationConfig,
): TransformResult;
export function scanAndTransform(
  text: string,
  config: ScanAndTransformConfig,
): TransformResult;
export interface PrivacyContext { readonly scope: string; }
export interface Restoration {
  readonly sourceByteRange: TextRange;
  readonly sourceCodepointRange: TextRange;
  readonly sourceUtf16Range: TextRange;
  readonly outputByteRange: TextRange;
  readonly outputCodepointRange: TextRange;
  readonly outputUtf16Range: TextRange;
  readonly tokenRef: string;
  readonly resolvedTokenVersion: string;
}
export interface RestoreResult { readonly text: string; readonly restorations: Restoration[]; }
export function restore(text: string, context: PrivacyContext): RestoreResult;

/** JSON input uses finite numbers; integer values must be JavaScript-safe. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonDocument = JsonValue[] | { [key: string]: JsonValue };
export interface StructuredScanConfig {
  /** Source syntax for email boundaries in each string leaf; defaults to text. */
  readonly format?: "text" | "env" | "sql";
  readonly locale?: string;
  /** Opt-in canonical UUID detection; disabled by default. */
  readonly detect_uuid?: boolean;
  readonly discover_person?: boolean;
  readonly mappings?: Readonly<Record<string, "PERSON">>;
  readonly exclude?: readonly string[];
}

export interface FieldMapping {
  readonly path: string;
  readonly entityType: "PERSON";
  readonly source: "field_alias" | "explicit_mapping";
  readonly rule: string;
}
export interface StructuredFinding { readonly path: string; readonly finding: Finding; }
export interface StructuredScanResult { readonly mappings: FieldMapping[]; readonly findings: StructuredFinding[]; }
export function discoverFields(data: JsonDocument, config?: StructuredScanConfig): FieldMapping[];
export function scanStructured(data: JsonDocument, config?: StructuredScanConfig): StructuredScanResult;

export interface StructuredScanAndTransformConfig { readonly scan?: StructuredScanConfig; readonly transform: TransformationConfig; }
export interface StructuredFindingInput { readonly path: string; readonly finding: FindingInput; }
export interface StructuredTransformation { readonly path: string; readonly transformation: Transformation; }
export interface StructuredTransformResult { readonly data: JsonDocument; readonly transformations: StructuredTransformation[]; }
export function transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): StructuredTransformResult;
export function scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig): StructuredTransformResult;
/** Always rejects with unsupported_strategy after input validation. */
export function restoreStructured(data: JsonDocument, context: PrivacyContext): never;

/** Provisional workspace input; exact values and type codes are never normalized. */
export interface WorkspaceFinding {
  readonly entityType: string;
  readonly matchedText: string;
  readonly range: TextRange & { readonly unit: "utf8_bytes" };
  readonly confidence?: number;
  readonly detectorName?: string;
  readonly detectorVersion?: string;
}
/** One batch input; finding offsets are relative to this text only. */
export interface WorkspaceInput {
  readonly text: string;
  readonly findings: readonly WorkspaceFinding[];
}
export interface WorkspaceHead { readonly revision: string; readonly commitId: readonly number[]; }
export interface WorkspacePreview {
  readonly text: string;
  readonly transformations: readonly {
    readonly sourceByteRange: TextRange;
    readonly sourceCodepointRange: TextRange;
    readonly sourceUtf16Range: TextRange;
    readonly outputByteRange: TextRange;
    readonly outputCodepointRange: TextRange;
    readonly outputUtf16Range: TextRange;
  }[];
}
export interface PreparedWorkspaceChange {
  /** Requires a single-input plan. */
  preview(): WorkspacePreview;
  previews(): readonly WorkspacePreview[];
  heads(): { readonly expectedHead: WorkspaceHead; readonly candidateHead: WorkspaceHead };
  /** Sensitive plaintext for the trusted encrypting host only. */
  exportCandidateBundle(): Uint8Array;
  discard(): void;
}
/** Content-free source or output issue ranges; never contains a source or restored value. */
export interface WorkspaceMarkerIssue {
  /** unknown: well formed but never committed here; damaged: unparseable marker; unbracketed: committed code without brackets. */
  readonly kind: "unknown" | "damaged" | "unbracketed";
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly utf16Range: TextRange;
}
export interface WorkspaceRestoreReport {
  /** Present only when every marker resolved and `issues` is empty. */
  readonly text: string | null;
  readonly restoredMarkers: number;
  readonly issues: readonly WorkspaceMarkerIssue[];
}
export class Workspace {
  private constructor();
  static create(namespace: Uint8Array, operationId: Uint8Array): Workspace;
  static openBundle(bytes: Uint8Array): Workspace;
  head(): WorkspaceHead;
  namespace(): Uint8Array;
  exportBundle(): Uint8Array;
  /** Stage a fresh recovery head without changing saved markers. No text previews or handoff permission. */
  prepareRecovery(operationId: Uint8Array): PreparedWorkspaceChange;
  /** Committed full markers pass through; damaged/unknown markers and bare committed codes reject with markerIssues before a plan is created. Findings must not overlap markers. */
  prepare(text: string, findings: readonly WorkspaceFinding[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** Atomic batch: preserves order, validates committed markers, and applies aggregate limits. */
  prepareBatch(inputs: readonly WorkspaceInput[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** The trusted host must confirm atomic persistence before calling this method. */
  activateSaved(change: PreparedWorkspaceChange, expectedHead: WorkspaceHead, savedHead: WorkspaceHead): void;
  /** All-or-nothing; throws `invalid_marker` when `restoreReport` would list an issue. */
  restore(text: string): string;
  /** All-or-nothing restoration that locates every unknown, damaged or unbracketed marker. */
  restoreReport(text: string): WorkspaceRestoreReport;
  close(): void;
}
export function workspaceByteRangeFromUtf16(text: string, start: number, end: number): TextRange & { readonly unit: "utf8_bytes" };
