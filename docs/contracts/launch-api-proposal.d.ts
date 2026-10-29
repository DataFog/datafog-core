/**
 * REVIEW DRAFT for #143, targeting 0.5.0-beta.2 / 0.5.0-beta.3.
 * Types only: not part of @datafog/wasm exports and no implementation exists here.
 * See launch-api-0.5.0-beta.2-beta.3.md before implementing any proposal.
 */
import type {
  PreparedWorkspaceChange, TextRange, WorkspaceFinding, WorkspaceHead,
  WorkspaceMarkerIssue, WorkspacePreview,
} from "../../bindings/wasm/index.js";

// #140 / draft PR #156; released beta.1 does not support these fields.
export type FindingSourceDraft = "scanner" | "recall" | "user";
export interface FindingDraft extends WorkspaceFinding {
  readonly source?: FindingSourceDraft;
  readonly needsReview?: boolean;
}
export interface ReviewSpanDraft {
  readonly source: FindingSourceDraft;
  readonly needsReview: boolean;
}
export interface PreviewDraft extends Omit<WorkspacePreview, "transformations"> {
  readonly transformations: readonly (WorkspacePreview["transformations"][number] & ReviewSpanDraft)[];
}
export interface PreparedChangeDraft extends Omit<PreparedWorkspaceChange, "preview" | "previews"> {
  preview(): PreviewDraft;
  previews(): readonly PreviewDraft[];
}
export interface LocatedFindingDraft {
  readonly finding: FindingDraft;
  readonly byteRange: TextRange;
  readonly codepointRange: TextRange;
  readonly utf16Range: TextRange;
}
export interface RecallResultDraft {
  readonly head: WorkspaceHead;
  readonly findings: readonly LocatedFindingDraft[];
  /** Empty results are not a statement that input contains no PII. */
  readonly complete: true;
}

export type TypeRuleDraft = "hide" | "keep";
export type ChoiceDispositionDraft = "keep" | "not_sensitive";
export type PolicyCommandDraft =
  | {readonly kind: "set_type_rule"; readonly entityType: string; readonly rule: TypeRuleDraft}
  | {readonly kind: "remember_choice"; readonly entityType: string; readonly value: string; readonly disposition: ChoiceDispositionDraft}
  | {readonly kind: "forget_choice"; readonly entityType: string; readonly value: string};
export interface ChoiceDraft {
  readonly entityType: string;
  /** Sensitive display value, for a trusted local review surface only. */
  readonly value: string;
  readonly disposition: ChoiceDispositionDraft;
}
export interface PolicyDraft {
  readonly protectedTypes: readonly string[];
  readonly rules: readonly {readonly entityType: string; readonly rule: TypeRuleDraft}[];
  readonly choices: readonly ChoiceDraft[];
}

/** Review-local key; invalid after the source/head changes, never persisted. */
export type CandidateKeyDraft = string;
export interface CorrectionDraft {
  readonly target: CandidateKeyDraft;
  readonly replacement: FindingDraft;
}
export interface ReviewInputDraft {
  readonly text: string;
  readonly scannerFindings: readonly FindingDraft[];
  readonly userSelections: readonly FindingDraft[];
  readonly corrections: readonly CorrectionDraft[];
}
export interface ReviewCandidateDraft extends LocatedFindingDraft {
  readonly key: CandidateKeyDraft;
  /** Local attribution for exact duplicates, not trusted detector evidence. */
  readonly contributingSources: readonly FindingSourceDraft[];
}
export interface ReviewResultDraft {
  readonly head: WorkspaceHead;
  readonly candidates: readonly ReviewCandidateDraft[];
  readonly conflicts: readonly {
    readonly kind: "partial_overlap" | "protected_coverage" | "conflicting_types";
    readonly candidates: readonly CandidateKeyDraft[];
    readonly byteRange: TextRange;
    readonly codepointRange: TextRange;
    readonly utf16Range: TextRange;
  }[];
}

/** Same fresh random 16-byte ID discipline as beta.1; time is untrusted host metadata. */
export interface OperationContextDraft {
  readonly id: Uint8Array;
  readonly occurredAtMs: number;
}
export type OutcomeKindDraft = "copy_confirmed" | "user_cancelled" | "blocked_input" | "restore_blocked";
export interface OutcomeDraft {
  readonly kind: OutcomeKindDraft;
  /** A bounded count, not text or detector/identity strings. */
  readonly issueCount: number;
}
export interface OperationRecordDraft {
  readonly id: readonly number[];
  /** Null only for a legacy call shape without host time; never fabricate a timestamp. */
  readonly occurredAtMs: number | null;
  readonly kind: "create" | "prepare" | "policy_change" | "recovery" | "handoff_confirmed" | OutcomeKindDraft;
}
export interface CountsDraft {
  readonly operations: number;
  readonly preparedInputs: number;
  readonly transformedSpans: number;
  readonly confirmedHandoffs: number;
  readonly reportedBlockedAttempts: number;
}
export interface ConversationDraft {
  readonly surface: "chatgpt";
  readonly siteConversationId: string;
}
export interface HandoffDraft {
  readonly conversation: ConversationDraft;
  /** Host-observed insertion/send acknowledgment; not proof of provider acceptance. */
  readonly action: "insert" | "send";
  /** Actual released text is validated locally and discarded, never stored as history. */
  readonly releasedText: string;
}
export interface ConversationRecordDraft {
  readonly conversation: ConversationDraft;
  readonly createdOperationId: readonly number[];
  readonly handedOffMarkers: readonly {readonly entityType: string; readonly code: string}[];
}
export interface ConversationRestoreReportDraft {
  readonly text: string | null;
  readonly restoredMarkers: number;
  readonly issues: readonly (Omit<WorkspaceMarkerIssue, "kind"> & {
    readonly kind: WorkspaceMarkerIssue["kind"] | "wrong_conversation";
  })[];
}

/** Proposed capabilities only. Never cast a released Workspace to this interface. */
export interface LaunchWorkspaceDraft {
  head(): WorkspaceHead;
  recall(text: string): RecallResultDraft;
  review(input: ReviewInputDraft): ReviewResultDraft;
  policy(): PolicyDraft;
  operations(): readonly OperationRecordDraft[];
  counts(): CountsDraft;
  conversations(): readonly ConversationRecordDraft[];
  prepareReview(inputs: readonly ReviewInputDraft[], operation: OperationContextDraft): PreparedChangeDraft;
  preparePolicyChange(commands: readonly PolicyCommandDraft[], operation: OperationContextDraft): PreparedChangeDraft;
  prepareOutcome(outcome: OutcomeDraft, operation: OperationContextDraft): PreparedChangeDraft;
  prepareHandoffRecord(handoff: HandoffDraft, operation: OperationContextDraft): PreparedChangeDraft;
  activateSaved(change: PreparedChangeDraft, expectedHead: WorkspaceHead, savedHead: WorkspaceHead): void;
  restoreInConversation(text: string, conversation: ConversationDraft): ConversationRestoreReportDraft;
}

/** Host proposal, not a Core/WASM package export. Export must freshly verify the password. */
export interface BackupHostDraft {
  exportBackup(request: {
    readonly workspaceId: Uint8Array;
    readonly freshPassword: string;
    readonly includeRecoveryCode: boolean;
  }): Promise<Uint8Array>;
}
export type AdditionalErrorCodeDraft =
  | "invalid_conversation" | "invalid_operation" | "operation_reused"
  | "protected_type" | "review_conflict";
