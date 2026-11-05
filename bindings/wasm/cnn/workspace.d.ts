import type {ScanConfig, Workspace, WorkspaceHead, WorkspacePreview, WorkspaceFinding, PreparedWorkspaceChange} from '../index.js';
import type {ModelScanResult} from '../models/index.js';

export class BrowserWorkspaceError extends Error {readonly code: string;}
export interface BrowserWorkspaceReview {
  preview(): WorkspacePreview;
  previews(): readonly WorkspacePreview[];
  heads(): ReturnType<PreparedWorkspaceChange['heads']>;
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
  commit(plan: VaultPlan, approval: {approved: true; requestId?: Uint8Array}): Promise<BrowserSavedChange>;
  restore: Workspace['restore'];
  restoreReport: Workspace['restoreReport'];
  head: Workspace['head'];
  close(): Promise<void>;
}
export interface BrowserWorkspaceDetector {
  scan(text: string, config?: ScanConfig): Promise<ModelScanResult>;
  setEnabled(enabled: boolean): Promise<void>;
  close(): Promise<void>;
}
/** Adopt exclusively: do not use the supplied detector or owner behind this coordinator. */
export class BrowserWorkspace {
  private constructor();
  static adopt(options: {workspace: BrowserWorkspaceOwner; detector: BrowserWorkspaceDetector}): BrowserWorkspace;
  readonly closed: boolean;
  readonly generation: number;
  prepare(text: string, operationId: Uint8Array, options?: {scan?: ScanConfig; userSelections?: readonly WorkspaceFinding[]}): Promise<BrowserWorkspaceReview>;
  commit(review: BrowserWorkspaceReview, approval: {approved: true; requestId?: Uint8Array}): Promise<BrowserSavedChange>;
  /** Invalidates pending reviews synchronously. Changing during a submitted save closes the owner and requires reopen. */
  setCnnEnabled(enabled: boolean): Promise<void>;
  restore: Workspace['restore'];
  restoreReport: Workspace['restoreReport'];
  head: Workspace['head'];
  /** Revokes authority synchronously; the Promise reports cleanup. */
  close(): Promise<void>;
  lock(): Promise<void>;
}
