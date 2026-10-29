/**
 * Docs-only contract for #143. Not npm exports or runtime implementations.
 * Main exports recall, policy and combined review;
 * operation/conversation types below remain unapproved beta.3 proposals.
 */
import type {
  PreparedWorkspaceChange, Workspace, WorkspaceMarkerIssue,
} from "../../bindings/wasm/index.js";

/** Fresh host-random ID; proposed untrusted host time, not an authorization. */
export interface OperationContextDraft {
  readonly id: Uint8Array;
  readonly occurredAtMs: number;
}
export type OutcomeKindDraft = "copy_confirmed" | "user_cancelled" | "blocked_input" | "restore_blocked";
export interface OutcomeDraft {
  readonly kind: OutcomeKindDraft;
  readonly issueCount: number;
}
export interface OperationRecordDraft {
  readonly id: readonly number[];
  /** Null for a legacy call without time; never fabricate history. */
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
  readonly action: "insert" | "send";
  /** Validated locally then discarded, never stored as history. */
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
/** Future-only additions; #162/#147/#148 have not frozen these call shapes. */
export interface LaunchWorkspaceDraft extends Pick<Workspace,
  "head" | "recall" | "review" | "prepareReview" | "policy" | "preparePolicyChanges" | "activateSaved"> {
  operations(): readonly OperationRecordDraft[];
  counts(): CountsDraft;
  conversations(): readonly ConversationRecordDraft[];
  prepareOutcome(outcome: OutcomeDraft, operation: OperationContextDraft): PreparedWorkspaceChange;
  prepareHandoffRecord(handoff: HandoffDraft, operation: OperationContextDraft): PreparedWorkspaceChange;
  restoreInConversation(text: string, conversation: ConversationDraft): ConversationRestoreReportDraft;
}
/** Host proposal only; actual #157 flow freshly verifies export passwords. */
export interface BackupHostDraft {
  exportBackup(request: {
    readonly workspaceId: Uint8Array;
    readonly freshPassword: string;
    readonly includeRecoveryCode: boolean;
  }): Promise<Uint8Array>;
}
export type AdditionalErrorCodeDraft =
  | "invalid_conversation" | "invalid_operation" | "operation_reused";
