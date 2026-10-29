/**
 * Compile-only inventory for #143/#154, outside every published SDK.
 * The legacy filename is retained for links; aliases below derive from ACTUAL
 * committed candidate declarations. No future methods or module augmentation.
 */
import type {
  Workspace as BrowserWorkspace,
  PreparedWorkspaceChange,
} from "../../bindings/wasm/index.js";
import type {Workspace as NativeNodeWorkspace} from "../../bindings/node/index.js";

export type CandidateWorkspace = Pick<BrowserWorkspace, keyof BrowserWorkspace>;
export type CandidatePlan = PreparedWorkspaceChange;
export type CandidateFactories = Pick<typeof BrowserWorkspace,
  "create" | "createWithOperation" | "openBundle">;

type RequireTrue<T extends true> = T;
/** Check both directions so a missing or extra instance call cannot hide here. */
export type NodeMatchesBrowser = RequireTrue<NativeNodeWorkspace extends CandidateWorkspace ? true : false>;
export type BrowserMatchesNode = RequireTrue<BrowserWorkspace extends Pick<NativeNodeWorkspace, keyof NativeNodeWorkspace> ? true : false>;
export type NodeFactoriesMatch = RequireTrue<Pick<typeof NativeNodeWorkspace,
  "create" | "createWithOperation" | "openBundle"> extends CandidateFactories ? true : false>;

export type {
  WorkspaceFinding, WorkspaceInput, WorkspaceHead, WorkspacePreview,
  WorkspaceReviewInput, WorkspaceReviewResult, WorkspaceMarkerIssue,
  WorkspaceOperation, WorkspaceOutcome, WorkspaceOperationRecord,
  WorkspaceOperationPage, WorkspaceHistoryCoverage, WorkspaceCounts,
  WorkspaceRestoreReport, WorkspaceRestoreProgress,
  WorkspaceConversation, WorkspaceConversationInput, WorkspaceConversationState,
  WorkspaceSendIntent, WorkspaceSendStatus,
} from "../../bindings/wasm/index.js";
