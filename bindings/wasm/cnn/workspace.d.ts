import type {Finding, ScanConfig, Workspace, WorkspaceHead, WorkspacePreview, WorkspaceFinding, PreparedWorkspaceChange, WorkspaceConversationInput, WorkspaceOperation} from '../index.js';

export class BrowserWorkspaceError extends Error {readonly code: string;}
export interface BrowserWorkspaceReview {
  preview(): WorkspacePreview;
  previews(): readonly WorkspacePreview[];
  heads(): ReturnType<PreparedWorkspaceChange['heads']>;
  sendIntents(): ReturnType<PreparedWorkspaceChange['sendIntents']>;
}
export interface BrowserWorkspaceConversationInput {
  readonly input: {readonly text: string; readonly userSelections?: readonly WorkspaceFinding[]};
  readonly conversation: WorkspaceConversationInput['conversation'];
}
interface VaultPlan extends BrowserWorkspaceReview {discard(): void;}
export interface BrowserSavedChange {
  readonly previews: readonly WorkspacePreview[];
  readonly head: WorkspaceHead;
  readonly recordRevision: bigint;
  readonly receipt: unknown;
}
export interface BrowserWorkspaceOwner {
  readonly closed: boolean;
  prepareReview(inputs: Parameters<Workspace['prepareReview']>[0], operationId: Uint8Array): VaultPlan;
  prepareConversationSend(inputs: Parameters<Workspace['prepareConversationSend']>[0], operation: WorkspaceOperation): VaultPlan;
  prepareObservedSend(...args: Parameters<Workspace['prepareObservedSend']>): VaultPlan;
  prepareDefinitelyNotSent(...args: Parameters<Workspace['prepareDefinitelyNotSent']>): VaultPlan;
  commit(plan: VaultPlan, approval: {approved: true; requestId?: Uint8Array}): Promise<BrowserSavedChange>;
  conversation: Workspace['conversation'];
  conversationForSite: Workspace['conversationForSite'];
  sendIntentStatus: Workspace['sendIntentStatus'];
  restoreInConversation: Workspace['restoreInConversation'];
  restoreReportInConversation: Workspace['restoreReportInConversation'];
  restore: Workspace['restore'];
  restoreReport: Workspace['restoreReport'];
  head: Workspace['head'];
  close(): Promise<unknown>;
}
export interface BrowserWorkspaceDetector {
  readonly cnnEnabled: boolean;
  readonly status: string;
  scan(text: string, config?: ScanConfig): Promise<{readonly findings: readonly Finding[]}>;
  setEnabled(enabled: boolean): Promise<unknown>;
  close(): Promise<void> | void;
}
/** Adopt exclusively: do not use the supplied detector or owner behind this coordinator. */
export class BrowserWorkspace {
  private constructor();
  /** Loads the default-enabled bundled detector, closing the vault owner if startup fails. */
  static open(options: {workspace: BrowserWorkspaceOwner; cnnEnabled?: boolean; workerUrl?: URL; timeoutMs?: number}): Promise<BrowserWorkspace>;
  static adopt(options: {workspace: BrowserWorkspaceOwner; detector: BrowserWorkspaceDetector}): BrowserWorkspace;
  readonly cnnEnabled: boolean;
  readonly detectorStatus: string;
  readonly closed: boolean;
  readonly generation: number;
  prepare(text: string, operationId: Uint8Array, options?: {scan?: ScanConfig; userSelections?: readonly WorkspaceFinding[]}): Promise<BrowserWorkspaceReview>;
  prepareConversationSend(inputs: readonly BrowserWorkspaceConversationInput[], operation: WorkspaceOperation, options?: {scan?: ScanConfig}): Promise<BrowserWorkspaceReview>;
  prepareObservedSend(...args: Parameters<Workspace['prepareObservedSend']>): BrowserWorkspaceReview;
  prepareDefinitelyNotSent(...args: Parameters<Workspace['prepareDefinitelyNotSent']>): BrowserWorkspaceReview;
  commit(review: BrowserWorkspaceReview, approval: {approved: true; requestId?: Uint8Array}): Promise<BrowserSavedChange>;
  /** Invalidates pending reviews synchronously. Changing during a submitted save closes the owner and requires reopen. */
  setCnnEnabled(enabled: boolean): Promise<void>;
  conversation: Workspace['conversation'];
  conversationForSite: Workspace['conversationForSite'];
  sendIntentStatus: Workspace['sendIntentStatus'];
  restoreInConversation: Workspace['restoreInConversation'];
  restoreReportInConversation: Workspace['restoreReportInConversation'];
  restore: Workspace['restore'];
  restoreReport: Workspace['restoreReport'];
  head: Workspace['head'];
  /** Revokes authority synchronously; the Promise reports cleanup. */
  close(): Promise<void>;
  lock(): Promise<void>;
}
