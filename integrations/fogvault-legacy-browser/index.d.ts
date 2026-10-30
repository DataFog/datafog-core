import type {Workspace,WorkspaceHead} from '@datafog/wasm';
import type {FogVaultWorkspace,WorkspaceVaultOptions} from '@datafog/core-fogvault-browser';
export class LegacyImportError extends Error {readonly code:string;readonly requestId?:Uint8Array;readonly requiresReopen?:boolean;readonly cleanupFailed?:boolean;}
export interface LegacyOpenOptions {
 Workspace:typeof Workspace; archive:Uint8Array; credential:string; method?:'password'|'recovery';
 signal?:AbortSignal; expectedNamespace?:Uint8Array; expectedHead?:WorkspaceHead;
}
export class LegacyBackupImport {
 private constructor();
 static open(options:LegacyOpenOptions):Promise<LegacyBackupImport>;
 preview():{exportedAt:string;namespace:Uint8Array;head:WorkspaceHead;counts:ReturnType<Workspace['counts']>};
 /** Fresh unlocked destination exclusively owned by this operation. Exact literal approval required. */
 commit(options:Omit<WorkspaceVaultOptions,'Workspace'> & {operationId:Uint8Array;requestId:Uint8Array;approved:true}):Promise<FogVaultWorkspace>;
 /** Wipes the owned bundle; once publication starts also locks the destination. */
 close():Promise<void>;
}
