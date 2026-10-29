/** Canonical built-in values are uppercase, but custom detectors may add values. */
export type EntityType = string;

export interface ModelSessionConfig {
  /** Omit to retain legacy selection from the presence of gliner2 settings. */
  engine?: "datafog" | "gliner2";
  bundle: string;
  manifest_sha256: string;
  limits: {
    max_input_bytes: number;
    max_response_bytes: number;
    max_findings: number;
    timeout_ms: number;
    cleanup_timeout_ms: number;
  };
  gliner2?: {
    python: string;
    labels: Array<{label: string; threshold: number; description?: string}>;
    intra_threads: number;
    chunk_size: number;
    chunk_overlap: number;
  };
}
export interface ModelFindingEvidence {
  finding_index: number;
  sources: Array<{
    detector_name: string;
    detector_version: string | null;
    native_label: string | null;
    raw_score: {value: number; scale_id: string} | null;
    calibrated_confidence: number | null;
  }>;
}
export interface ModelDescriptor {
  readonly backend: string;
  readonly model: string;
  readonly adapter_version: string;
  /** Native model label → configured canonical entity type. */
  readonly labels: Readonly<Record<string, string>>;
  readonly score_scale: string;
}
/** Rule scan options; `entities` may also name the session's model labels. */
export interface ModelScanConfig extends Omit<ScanConfig, "entities"> {
  readonly entities?: readonly string[];
}
export declare class ModelSession {
  private constructor();
  static open(config: ModelSessionConfig): Promise<ModelSession>;
  /** Configured metadata, available after close; does not probe runtime readiness. */
  describe(): ModelDescriptor;
  /** Rule detectors handle the entity types they support and the model runs
   * only for requested types it alone provides; rule-only requests skip the
   * model. Omitting `config` equals `{}`: every enabled rule detector plus the
   * model's remaining labels. Unknown option keys throw a TypeError. */
  scan(text: string, options?: {config?: ModelScanConfig; cancellation?: ModelCancellation}): Promise<ModelScanResult>;
  /** Raw model output: every model label, with no rule detectors. */
  scanModel(text: string, options?: {cancellation?: ModelCancellation}): Promise<ModelScanResult>;
  close(): Promise<void>;
}

export type TransformationStrategy =
  | "redact"
  | "mask"
  | "remove"
  | "pseudonymize"
  | "tokenize";

export interface MaskRevealConfig {
  readonly direction: "first" | "last";
  readonly count: number;
}

export type TransformationStrategyConfig =
  | { readonly strategy: "redact" }
  | { readonly strategy: "remove" }
  | {
      readonly strategy: "pseudonymize";
      readonly key_ref: string;
      readonly key_version?: string;
    }
  | {
      readonly strategy: "tokenize";
      readonly token_ref: string;
    }
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
  | "closed_workspace" | "inactive_plan" | "workspace_conflict" | "invalid_batch"
  | "invalid_marker" | "invalid_snapshot" | "invalid_operation_id" | "invalid_policy" | "resource_limit"
  | "invalid_configuration"
  | "invalid_finding"
  | "review_conflict"
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
  | "internal_error";

export declare class DataFogError extends Error {
  readonly code: DataFogErrorCode;
  readonly reason?: string;
  readonly path?: string;
  readonly findingIndex?: number | null;
  readonly inputIndex?: number | null;
  readonly markerIssues?: readonly WorkspaceMarkerIssue[];
  readonly reviewConflicts?: readonly WorkspaceReviewConflict[];
}

export interface KeyProviderRequest {
  readonly keyRef: string;
  readonly keyVersion?: string;
}

export interface KeyProviderResponse {
  readonly key: Uint8Array;
  readonly resolvedVersion: string;
}

export interface KeyProvider {
  resolveKey(request: KeyProviderRequest): Promise<KeyProviderResponse>;
}

export interface PrivacyContext {
  readonly scope: string;
}

export interface TokenizeProviderItem {
  readonly id: string;
  readonly exactValue: string;
  readonly tokenRef: string;
}

export interface TokenizeProviderResult {
  readonly id: string;
  readonly payload: Uint8Array;
  readonly resolvedVersion: string;
}

export interface RestoreProviderItem {
  readonly id: string;
  readonly tokenRef: string;
  readonly resolvedVersion: string;
  readonly payload: Uint8Array;
}

export interface RestoreProviderResult {
  readonly id: string;
  readonly value: string;
}

export interface TokenProvider {
  tokenizeBatch(scope: string, items: TokenizeProviderItem[]): Promise<TokenizeProviderResult[]>;
  restoreBatch(scope: string, items: RestoreProviderItem[]): Promise<RestoreProviderResult[]>;
}

export interface PrivacyManagerProviders {
  readonly keyProvider?: KeyProvider;
  readonly tokenProvider?: TokenProvider;
}

export declare class PrivacyManager {
  transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig, context?: PrivacyContext): Promise<StructuredTransformResult>;
  scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig, context?: PrivacyContext): Promise<StructuredTransformResult>;
  restoreStructured(data: JsonDocument, context: PrivacyContext): Promise<StructuredRestoreResult>;

  constructor(provider: KeyProvider | PrivacyManagerProviders, tokenProvider?: TokenProvider);
  transform(
    text: string,
    findings: FindingInput[],
    config: TransformationConfig,
    context?: PrivacyContext,
  ): Promise<TransformResult>;
  scanAndTransform(
    text: string,
    config: ScanAndTransformConfig,
    context?: PrivacyContext,
  ): Promise<TransformResult>;
  restore(text: string, context: PrivacyContext): Promise<RestoreResult>;
}

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

export interface StructuredScanAndTransformConfig {
  readonly scan?: StructuredScanConfig;
  readonly transform: TransformationConfig;
}
export declare function transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig): StructuredTransformResult;
export declare function scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig): StructuredTransformResult;

export interface StructuredTransformResult { readonly data: JsonDocument; readonly transformations: StructuredTransformation[]; }
export interface StructuredRestoreResult { readonly data: JsonDocument; readonly restorations: StructuredRestoration[]; }

/** Caller-declared review origin, not an authorization or authenticity claim. */
export type WorkspaceFindingSource = "scanner" | "recall" | "user";

/** Provisional workspace input; exact values and type codes are never normalized. */
export interface WorkspaceFinding {
  readonly entityType: string;
  readonly matchedText: string;
  readonly range: TextRange & { readonly unit: "utf8_bytes" };
  readonly confidence?: number | null;
  readonly detectorName?: string | null;
  readonly detectorVersion?: string | null;
  /** Defaults to scanner for existing callers; not inferred from detectorName. */
  readonly source?: WorkspaceFindingSource;
  /** Defaults to true. False does not waive host approval/save/activation. */
  readonly needsReview?: boolean;
}
/** One batch input; finding offsets are relative to this text only. */
export interface WorkspaceInput {
  readonly text: string;
  readonly findings: readonly WorkspaceFinding[];
}
export interface WorkspaceHead { readonly revision: string; readonly commitId: readonly number[]; }
export interface WorkspaceRecalledFinding {
  readonly finding: WorkspaceFinding & {readonly source: "recall"; readonly needsReview: true};
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly utf16Range: TextRange;
}
export interface WorkspaceRecallResult {
  readonly head: WorkspaceHead;
  /** All matches or an explicit failure. Empty is not a clean-text verdict. */
  readonly complete: true;
  readonly findings: readonly WorkspaceRecalledFinding[];
}
export interface WorkspacePreview {
  readonly text: string;
  readonly transformations: readonly {
    readonly source: WorkspaceFindingSource;
    readonly needsReview: boolean;
    readonly sourceByteRange: TextRange;
    readonly sourceCodepointRange: TextRange;
    readonly sourceUtf16Range: TextRange;
    readonly outputByteRange: TextRange;
    readonly outputCodepointRange: TextRange;
    readonly outputUtf16Range: TextRange;
  }[];
}
export type WorkspaceTypeRule = "hide" | "keep";
export type WorkspaceChoiceDisposition = "keep" | "not_sensitive";
export interface WorkspacePolicyEdits {
  readonly rules: readonly {readonly entityType: string; readonly rule: WorkspaceTypeRule | null}[];
  readonly choices: readonly {readonly entityType: string; readonly value: string; readonly disposition: WorkspaceChoiceDisposition | null}[];
}
export interface WorkspacePolicy {
  readonly snapshotVersion: 1 | 2;
  readonly defaultRule: "hide";
  readonly protectedTypes: readonly string[];
  readonly rules: readonly {readonly entityType: string; readonly rule: WorkspaceTypeRule}[];
  readonly choices: readonly {readonly entityType: string; readonly value: string; readonly disposition: WorkspaceChoiceDisposition}[];
}
export interface WorkspacePolicyPreview {
  readonly rules: readonly {readonly entityType: string; readonly before: WorkspaceTypeRule | null; readonly after: WorkspaceTypeRule | null}[];
  readonly choices: readonly {readonly entityType: string; readonly value: string; readonly before: WorkspaceChoiceDisposition | null; readonly after: WorkspaceChoiceDisposition | null}[];
}
export interface PreparedWorkspaceChange {
  /** Warn before saving a v1-to-v2 upgrade; beta.1 cannot open v2. */
  snapshotFormat(): {readonly currentVersion: 1 | 2; readonly candidateVersion: 1 | 2};
  /** Present only for settings edits; values are sensitive and remain local. */
  policyPreview(): WorkspacePolicyPreview | null;
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
export interface WorkspaceReviewInput {
  readonly text: string;
  /** Model/rule findings; caller source flags are ignored for channel attribution. */
  readonly scannerFindings: readonly WorkspaceFinding[];
  readonly userSelections: readonly WorkspaceFinding[];
  readonly corrections?: readonly { readonly candidateKey: string; readonly replacement: WorkspaceFinding }[];
}
export interface WorkspaceReviewConflict {
  readonly kind: "protected_coverage" | "type_conflict";
  readonly findingIndex: number;
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly utf16Range: TextRange;
}
export interface WorkspaceReviewCandidate {
  /** Binds exact source/head/type/range; changes on source edits or saved-head changes. */
  readonly key: string;
  readonly finding: WorkspaceFinding;
  readonly sources: readonly ("scanner" | "recall" | "user")[];
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly utf16Range: TextRange;
  /** Committed policy requires this candidate to be hidden. */
  readonly eligible: boolean;
  /** Conflict-free selection only; false for every candidate while conflicts remain. Never handoff permission. */
  readonly selected: boolean;
}
export interface WorkspaceReviewResult {
  readonly head: WorkspaceHead;
  readonly candidates: readonly WorkspaceReviewCandidate[];
  readonly conflicts: readonly WorkspaceReviewConflict[];
}
export class Workspace {
  private constructor();
  static create(namespace: Uint8Array, operationId: Uint8Array): Workspace;
  static openBundle(bytes: Uint8Array): Workspace;
  head(): WorkspaceHead;
  namespace(): Uint8Array;
  exportBundle(): Uint8Array;
  /** Read committed values only, returning all overlaps in original coordinates. No aliases, plans or state changes. */
  recall(text: string): WorkspaceRecallResult;
  /** Combine supplied scanner/manual findings with committed recall, without plans or state mutation. */
  review(input: WorkspaceReviewInput): WorkspaceReviewResult;
  /** Recompute review for every input. Any unresolved overlap rejects the whole batch before a plan exists. Approval and durable save remain required. */
  prepareReview(inputs: readonly WorkspaceReviewInput[], operationId: Uint8Array): PreparedWorkspaceChange;
  policy(): WorkspacePolicy;
  /** Explicit exact-value corrections can override real PII; source flags cannot. Null removes an override. */
  preparePolicyChanges(edits: WorkspacePolicyEdits, operationId: Uint8Array): PreparedWorkspaceChange;
  /** Stage a fresh recovery head without changing saved markers. No text previews or handoff permission. */
  prepareRecovery(operationId: Uint8Array): PreparedWorkspaceChange;
  /** Supplied findings only: omissions are never proof of PII-free input; use prepareReview for combined recall. Committed full markers pass through; damaged/unknown markers and bare committed codes reject with markerIssues before a plan is created. Findings must not overlap markers. */
  prepare(text: string, findings: readonly WorkspaceFinding[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** Atomic batch: preserves order, validates committed markers, and applies aggregate limits. */
  prepareBatch(inputs: readonly WorkspaceInput[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** The trusted host must confirm atomic persistence first. Success consumes this plan and drops retained plans based on older heads; failure preserves current plans. */
  activateSaved(change: PreparedWorkspaceChange, expectedHead: WorkspaceHead, savedHead: WorkspaceHead): void;
  /** All-or-nothing; throws `invalid_marker` when `restoreReport` would list an issue. */
  restore(text: string): string;
  /** All-or-nothing restoration that locates every unknown, damaged or unbracketed marker. */
  restoreReport(text: string): WorkspaceRestoreReport;
  close(): void;
}
export function workspaceByteRangeFromUtf16(text: string, start: number, end: number): TextRange & { readonly unit: "utf8_bytes" };
