import type {
  Workspace, WorkspaceHead, PreparedWorkspaceChange, WorkspacePreview,
} from '@datafog/wasm';
import type {FogVault, Receipt} from '@datafog/fogvault-browser';

export class WorkspaceVaultError extends Error {readonly code: string; readonly requestId?: Uint8Array;}
/** One live prepared change per owner; discard unused plans promptly. */
export interface WorkspaceVaultPlan {
  preview(): WorkspacePreview;
  previews(): readonly WorkspacePreview[];
  heads(): ReturnType<PreparedWorkspaceChange['heads']>;
  snapshotFormat(): ReturnType<PreparedWorkspaceChange['snapshotFormat']>;
  policyPreview(): ReturnType<PreparedWorkspaceChange['policyPreview']>;
  sendIntents(): ReturnType<PreparedWorkspaceChange['sendIntents']>;
  discard(): void;
}
export interface SavedWorkspaceChange {
  readonly previews: readonly WorkspacePreview[];
  readonly head: WorkspaceHead;
  readonly recordRevision: bigint;
  readonly receipt: Receipt;
}
export interface WorkspaceVaultOptions {
  /** Initialize the actual @datafog/wasm module before supplying its Workspace class. */
  Workspace: typeof Workspace;
  /** Already created/opened and unlocked owner; give the adapter exclusive mutation ownership. */
  vault: FogVault;
  recordId: Uint8Array;
}
export class FogVaultWorkspace {
  private constructor();
  static create(options: WorkspaceVaultOptions & {namespace: Uint8Array; operationId?: Uint8Array; requestId?: Uint8Array}): Promise<FogVaultWorkspace>;
  static open(options: WorkspaceVaultOptions & {expectedNamespace?: Uint8Array; expectedHead?: WorkspaceHead}): Promise<FogVaultWorkspace>;
  /** Host first authenticates/decrypts the source; this does not decrypt legacy encrypted backups. Destination record must be empty. */
  static importAuthenticatedBundle(options: WorkspaceVaultOptions & {bundle: Uint8Array; expectedNamespace: Uint8Array; expectedHead: WorkspaceHead; operationId?: Uint8Array; requestId?: Uint8Array; approved: true}): Promise<FogVaultWorkspace>;
  readonly closed: boolean;
  commit(plan: WorkspaceVaultPlan, approval: {approved: true; requestId?: Uint8Array}): Promise<SavedWorkspaceChange>;
  head: Workspace['head'];
  namespace: Workspace['namespace'];
  recordRevision(): bigint;
  policy: Workspace['policy'];
  recall: Workspace['recall'];
  review: Workspace['review'];
  operations: Workspace['operations'];
  operationsPage: Workspace['operationsPage'];
  counts: Workspace['counts'];
  historyCoverage: Workspace['historyCoverage'];
  conversation: Workspace['conversation'];
  conversationForSite: Workspace['conversationForSite'];
  sendIntentStatus: Workspace['sendIntentStatus'];
  restore: Workspace['restore'];
  restoreReport: Workspace['restoreReport'];
  restoreInConversation: Workspace['restoreInConversation'];
  restoreReportInConversation: Workspace['restoreReportInConversation'];
  bufferedRestore: Workspace['bufferedRestore'];
  progressiveRestore: Workspace['progressiveRestore'];
  bufferedRestoreInConversation: Workspace['bufferedRestoreInConversation'];
  progressiveRestoreInConversation: Workspace['progressiveRestoreInConversation'];
  /** A second live preparation fails with resource_limit before Core clones state. */
  prepare(...args: Parameters<Workspace['prepare']>): WorkspaceVaultPlan;
  prepareBatch(...args: Parameters<Workspace['prepareBatch']>): WorkspaceVaultPlan;
  prepareReview(...args: Parameters<Workspace['prepareReview']>): WorkspaceVaultPlan;
  prepareConversationSend(...args: Parameters<Workspace['prepareConversationSend']>): WorkspaceVaultPlan;
  prepareObservedSend(...args: Parameters<Workspace['prepareObservedSend']>): WorkspaceVaultPlan;
  prepareDefinitelyNotSent(...args: Parameters<Workspace['prepareDefinitelyNotSent']>): WorkspaceVaultPlan;
  preparePolicyChanges(...args: Parameters<Workspace['preparePolicyChanges']>): WorkspaceVaultPlan;
  prepareOutcome(...args: Parameters<Workspace['prepareOutcome']>): WorkspaceVaultPlan;
  prepareRecovery(...args: Parameters<Workspace['prepareRecovery']>): WorkspaceVaultPlan;
  prepareWithOperation(...args: Parameters<Workspace['prepareWithOperation']>): WorkspaceVaultPlan;
  prepareBatchWithOperation(...args: Parameters<Workspace['prepareBatchWithOperation']>): WorkspaceVaultPlan;
  prepareRecoveryWithOperation(...args: Parameters<Workspace['prepareRecoveryWithOperation']>): WorkspaceVaultPlan;
  /** Close Core synchronously; Promise covers only FogVault cleanup. Caller copies remain caller owned. */
  lock(): ReturnType<FogVault['lock']>;
  close(): ReturnType<FogVault['close']>;
}
