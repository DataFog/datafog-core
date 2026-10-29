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
  | "review_conflict"
  | "invalid_operation" | "operation_reused"
  | "invalid_conversation" | "conversation_unknown" | "conversation_pending" | "invalid_send_intent"
  | "closed_workspace"
  | "inactive_plan"
  | "inactive_stream"
  | "invalid_encoding"
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
  readonly reviewConflicts?: readonly WorkspaceReviewConflict[];
  /** Present on terminal progressive errors; counts only prior successful calls. */
  readonly delivered?: WorkspaceRestoreProgress;
  readonly complete?: false;
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
  readonly snapshotVersion: 1 | 2 | 3;
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
  /** Warn before a saved upgrade; beta.1/beta.2 cannot open v3. */
  snapshotFormat(): {readonly currentVersion: 1 | 2 | 3; readonly candidateVersion: 1 | 2 | 3};
  /** Present only for settings edits; values are sensitive and remain local. */
  policyPreview(): WorkspacePolicyPreview | null;
  /** Saved intention only; neither preview nor this ID proves a send. */
  sendIntents(): readonly WorkspaceSendIntent[];
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
  readonly kind: "unknown" | "damaged" | "unbracketed" | "not_issued_in_conversation";
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
export interface WorkspaceOperation { readonly id: Uint8Array; readonly occurredAtMs: number; }
export interface WorkspaceConversation {
  /** Private immutable host-generated ID, independent of the site's ID. */
  readonly id: Uint8Array | readonly number[];
  readonly surface: string;
  /** Required null for an initially unbound first chat. Bind once after observed send. */
  readonly siteId: string | null;
  /** Immutable pre-send maximum UTF-8 bytes; the host must substantiate this bound. */
  readonly siteIdByteBudget: number;
}
export interface WorkspaceConversationInput { readonly input: WorkspaceReviewInput; readonly conversation: WorkspaceConversation; }
export interface WorkspaceSendIntent { readonly preparationId: Uint8Array | readonly number[]; readonly inputIndex: number; }
export interface WorkspaceConversationState { readonly conversation: WorkspaceConversation; readonly pendingIntents: number; readonly issuedMarkers: number; }
export type WorkspaceSendStatus = "pending" | "observed_sent" | "definitely_not_sent";
export type WorkspaceOutcomeKind = "copy_confirmed" | "user_cancelled" | "blocked_input" | "restore_blocked";
export interface WorkspaceOutcome { readonly kind: WorkspaceOutcomeKind; readonly issueCount: number; }
export interface WorkspaceOperationRecord {
  readonly id: readonly number[];
  readonly kind: "create" | "prepare" | "recovery" | "policy_change" | "conversation_prepare" | "send_confirmed" | "send_not_sent" | WorkspaceOutcomeKind;
  readonly occurredAtMs: number | null;
  readonly preparedInputs: number;
  readonly transformedSpans: number;
}
export interface WorkspaceOperationPage {
  readonly records: readonly WorkspaceOperationRecord[];
  readonly total: number;
  readonly offset: number;
  readonly nextOffset: number | null;
}
export interface WorkspaceHistoryCoverage {
  readonly recordedHistoryOnly: true;
  readonly preUpgradeHistory: "unknown" | "not_applicable";
}
export interface WorkspaceCounts {
  readonly operations: number; readonly preparedInputs: number; readonly transformedSpans: number;
  readonly confirmedHandoffs: number; readonly reportedBlockedAttempts: number;
}
export type WorkspaceRestoreEncoding = "utf8" | "utf16";
export interface WorkspaceRestoreProgress {
  readonly bytes: number;
  readonly codepoints: number;
  readonly utf16: number;
  readonly restoredMarkers: number;
}
export interface WorkspaceRestoreChunk {
  readonly text: string;
  readonly delivered: WorkspaceRestoreProgress;
  /** Only successful finalization is complete; prior chunks cannot be retracted. */
  readonly complete: boolean;
}
export interface WorkspaceBufferedRestore {
  feed(bytes: Uint8Array): void;
  feedUtf16(units: Uint16Array): void;
  finalize(): WorkspaceRestoreReport;
  reset(): void;
  close(): void;
}
export interface WorkspaceProgressiveRestore {
  feed(bytes: Uint8Array): WorkspaceRestoreChunk;
  feedUtf16(units: Uint16Array): WorkspaceRestoreChunk;
  finalize(): WorkspaceRestoreChunk;
  reset(): void;
  close(): void;
}
export class Workspace {
  private constructor();
  static createWithOperation(namespace: Uint8Array, operation: WorkspaceOperation): Workspace;
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
  /** Funds the later resolution before any external handoff; save/activate explicitly. */
  prepareConversationSend(inputs: readonly WorkspaceConversationInput[], operation: WorkspaceOperation): PreparedWorkspaceChange;
  conversation(logicalId: Uint8Array): WorkspaceConversationState;
  conversationForSite(surface: string, siteId: string): Uint8Array;
  sendIntentStatus(intent: WorkspaceSendIntent): WorkspaceSendStatus;
  /** Trusted actual-send observation only; binding and issuance save atomically. */
  prepareObservedSend(intent: WorkspaceSendIntent, logicalId: Uint8Array, surface: string, siteId: string, operation: WorkspaceOperation): PreparedWorkspaceChange;
  /** Trusted definite non-delivery, never a generic timeout or manual send assertion. */
  prepareDefinitelyNotSent(intent: WorkspaceSendIntent, logicalId: Uint8Array, operation: WorkspaceOperation): PreparedWorkspaceChange;
  restoreInConversation(text: string, logicalId: Uint8Array): string;
  restoreReportInConversation(text: string, logicalId: Uint8Array): WorkspaceRestoreReport;
  bufferedRestoreInConversation(logicalId: Uint8Array, encoding?: WorkspaceRestoreEncoding): WorkspaceBufferedRestore;
  progressiveRestoreInConversation(logicalId: Uint8Array, encoding?: WorkspaceRestoreEncoding): WorkspaceProgressiveRestore;
  policy(): WorkspacePolicy;
  /** Explicit exact-value corrections can override real PII; source flags cannot. Null removes an override. */
  preparePolicyChanges(edits: WorkspacePolicyEdits, operationId: Uint8Array): PreparedWorkspaceChange;
  /** Stage a fresh recovery head without changing saved markers. No text previews or handoff permission. */
  operations(): readonly WorkspaceOperationRecord[];
  /** Explicit read-only window. Offset may equal total; limit must be 1..100000. Full operations() is unchanged. */
  operationsPage(offset: number, limit: number): WorkspaceOperationPage;
  counts(): WorkspaceCounts;
  historyCoverage(): WorkspaceHistoryCoverage;
  prepareRecoveryWithOperation(operation: WorkspaceOperation): PreparedWorkspaceChange;
  prepareWithOperation(text: string, findings: readonly WorkspaceFinding[], operation: WorkspaceOperation): PreparedWorkspaceChange;
  prepareBatchWithOperation(inputs: readonly WorkspaceInput[], operation: WorkspaceOperation): PreparedWorkspaceChange;
  prepareOutcome(outcome: WorkspaceOutcome, operation: WorkspaceOperation): PreparedWorkspaceChange;
  prepareRecovery(operationId: Uint8Array): PreparedWorkspaceChange;
  /** Supplied findings only: omissions are never proof of PII-free input; use prepareReview for combined recall. Committed full markers pass through; damaged/unknown markers and bare committed codes reject with markerIssues before a plan is created. Findings must not overlap markers. */
  prepare(text: string, findings: readonly WorkspaceFinding[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** Atomic batch: preserves order, validates committed markers, and applies aggregate limits. */
  prepareBatch(inputs: readonly WorkspaceInput[], operationId: Uint8Array): PreparedWorkspaceChange;
  /** The trusted host must confirm atomic persistence first. Success consumes this plan and drops retained plans based on older heads; failure preserves current plans. */
  activateSaved(change: PreparedWorkspaceChange, expectedHead: WorkspaceHead, savedHead: WorkspaceHead): void;
  /** Strict R2-compatible default, including literal EOF handling. */
  bufferedRestore(encoding?: WorkspaceRestoreEncoding): WorkspaceBufferedRestore;
  /** Explicit irreversible delivery; unfinished/oversized possible markers stop incomplete. */
  progressiveRestore(encoding?: WorkspaceRestoreEncoding): WorkspaceProgressiveRestore;
  /** All-or-nothing; throws `invalid_marker` when `restoreReport` would list an issue. */
  restore(text: string): string;
  /** All-or-nothing restoration that locates every unknown, damaged or unbracketed marker. */
  restoreReport(text: string): WorkspaceRestoreReport;
  close(): void;
}
export function workspaceByteRangeFromUtf16(text: string, start: number, end: number): TextRange & { readonly unit: "utf8_bytes" };
