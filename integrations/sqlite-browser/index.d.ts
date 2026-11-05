import type {
  Workspace, WorkspaceHead, PreparedWorkspaceChange, WorkspacePreview,
} from '@datafog/wasm';
export interface SQLiteReceipt {readonly revision: bigint;}
export class SQLiteVault {
  private constructor();
  static create(options: {id: string; password: string; recoveryKey?: Uint8Array; workerUrl?: URL; maxPayloadBytes?: number}): Promise<SQLiteVault>;
  static open(options: {id: string; password?: string; recoveryKey?: Uint8Array; workerUrl?: URL; maxPayloadBytes?: number}): Promise<SQLiteVault>;
  changePassword(options: {password: string; recoveryKey?: Uint8Array}): Promise<void>;
  readonly locked: boolean;
  readonly closed: boolean;
  lock(): Promise<void>;
  close(): Promise<void>;
  reconcileLatest(requestId: Uint8Array): Promise<{revision: bigint; found: boolean}>;
}

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
  readonly receipt: SQLiteReceipt;
}
export interface WorkspaceVaultOptions {
  /** Initialize the actual @datafog/wasm module before supplying its Workspace class. */
  Workspace: typeof Workspace;
  /** Already created/opened and unlocked owner; give the adapter exclusive mutation ownership. */
  vault: SQLiteVault;
  recordId: Uint8Array;
}
export class SQLiteWorkspace {
  private constructor();
  static create(options: WorkspaceVaultOptions & {namespace: Uint8Array; operationId?: Uint8Array; requestId?: Uint8Array}): Promise<SQLiteWorkspace>;
  static open(options: WorkspaceVaultOptions & {expectedNamespace?: Uint8Array; expectedHead?: WorkspaceHead}): Promise<SQLiteWorkspace>;
  /** Host first authenticates/decrypts the source; this does not decrypt legacy encrypted backups. Destination record must be empty. */
  static importAuthenticatedBundle(options: WorkspaceVaultOptions & {bundle: Uint8Array; expectedNamespace: Uint8Array; expectedHead: WorkspaceHead; operationId?: Uint8Array; requestId?: Uint8Array; approved: true}): Promise<SQLiteWorkspace>;
  static importBackup(options: WorkspaceVaultOptions & {backup: Uint8Array; password: string; approved: true; operationId?: Uint8Array; requestId?: Uint8Array}): Promise<SQLiteWorkspace>;
  exportBackup(options: {password: string}): Promise<Uint8Array>;
  /** Replaces password wrapper and optional recovery wrapper atomically; omitted recoveryKey removes recovery. */
  changePassword(options: {password: string; recoveryKey?: Uint8Array}): Promise<void>;
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
  /** Persist mapping retirement before activating it; old markers remain expired after reopen. */
  prepareForget(...args: Parameters<Workspace['prepareForget']>): WorkspaceVaultPlan;
  prepareRecovery(...args: Parameters<Workspace['prepareRecovery']>): WorkspaceVaultPlan;
  prepareWithOperation(...args: Parameters<Workspace['prepareWithOperation']>): WorkspaceVaultPlan;
  prepareBatchWithOperation(...args: Parameters<Workspace['prepareBatchWithOperation']>): WorkspaceVaultPlan;
  prepareRecoveryWithOperation(...args: Parameters<Workspace['prepareRecoveryWithOperation']>): WorkspaceVaultPlan;
  /** Close Core synchronously; Promise covers only SQLite cleanup. Caller copies remain caller owned. */
  lock(): Promise<void>;
  close(): Promise<void>;
}
