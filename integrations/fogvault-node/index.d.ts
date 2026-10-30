export type Id = Uint8Array;
export interface Head { revision:string; commitId:readonly number[] }
export interface HeadInput {revision:string; commitId:Uint8Array|readonly number[]}
export interface Receipt {generation:string;requestId:readonly number[];headDigest:readonly number[]}
export interface FactoryOptions {tenant:string;protectedControlPath:string;vaultPath:string;password:Uint8Array;recordId:Id;namespace:Id;profile?:'password_only';maxPayloadBytes?:number}
export interface CreateOptions extends FactoryOptions {operationId:Id;requestId:Id}
export interface OpenOptions extends FactoryOptions {expectedHead?:HeadInput}
export interface ImportOptions extends CreateOptions {sourceHead:HeadInput}
export type ReconcileOptions = Omit<FactoryOptions,'password'>;
export interface Range {start:number;end:number}
export interface Finding {entityType:string;matchedText:string;range:Range&{unit:'utf8_bytes'};confidence?:number|null;detectorName?:string|null;detectorVersion?:string|null;source?:'scanner'|'recall'|'user';needsReview?:boolean}
export interface ReviewInput {text:string;scannerFindings:readonly Finding[];userSelections:readonly Finding[];corrections?:readonly {candidateKey:string;replacement:Finding}[]}
export interface Operation {id:Id|readonly number[];occurredAtMs:number}
export interface Conversation {id:Id|readonly number[];surface:string;siteId:string|null;siteIdByteBudget:number}
export interface SendInput {input:ReviewInput;conversation:Conversation}
export interface SendIntent {preparationId:readonly number[];inputIndex:number}
export interface MarkerIssue {kind:string;byteRange:Range;codepointRange:Range;utf16Range:Range}
export interface RestoreReport {text:string|null;restoredMarkers:number;issues:MarkerIssue[]}
export interface Transformation {source:string;needsReview:boolean;sourceByteRange:Range;sourceCodepointRange:Range;sourceUtf16Range:Range;outputByteRange:Range;outputCodepointRange:Range;outputUtf16Range:Range}
export interface Preview {text:string;transformations:Transformation[]}
export interface OperationRecord {id:readonly number[];kind:string;occurredAtMs:number|null;preparedInputs:number;transformedSpans:number}
export interface Counts {operations:number;preparedInputs:number;transformedSpans:number;confirmedHandoffs:number;reportedBlockedAttempts:number}
export interface Committed {head:Head;receipt:Receipt;recordRevision:string;previews:string[]}
export interface PolicyEdits {rules:readonly {entityType:string;rule:'hide'|'keep'|null}[];choices:readonly {entityType:string;value:string;disposition:'keep'|'not_sensitive'|null}[]}
export interface Delivered {bytes:number;codepoints:number;utf16:number;restoredMarkers:number}
export interface RestoreChunk {text:string;complete:boolean;delivered:Delivered}
export class DataFogVaultError extends Error {readonly code:string;readonly requestId?:readonly number[];readonly requiresReopen?:boolean;readonly markerIssues?:MarkerIssue[];readonly complete?:boolean;readonly delivered?:Delivered}
export class PreparedChange {
 private constructor();
 previews(tenant:string):Preview[];
 heads(tenant:string):{parent:Head;candidate:Head};
 sendIntents(tenant:string):SendIntent[];
 snapshotFormat(tenant:string):{currentVersion:number;candidateVersion:number};
 policyPreview(tenant:string):unknown;
 discard(tenant:string):void;
}
export class RestoreStream {
 private constructor();
 feed(tenant:string,value:Uint8Array|Uint16Array):null|RestoreChunk;
 finalize(tenant:string):RestoreReport|RestoreChunk;
 reset(tenant:string):void;
 close(tenant:string):void;
}
export class NativeWorkspace {
 private constructor();
 static createNew(options:CreateOptions):NativeWorkspace;
 static openExisting(options:OpenOptions):NativeWorkspace;
 static importAuthenticatedBundle(options:ImportOptions,bundle:Uint8Array,approved:true):NativeWorkspace;
 static reconcileExisting(options:ReconcileOptions,requestId:Id):Receipt;
 head(tenant:string):Head;
 counts(tenant:string):Counts;
 historyCoverage(tenant:string):{recordedHistoryOnly:boolean;preUpgradeHistory:'unknown'|'not_applicable'};
 policy(tenant:string):unknown;
 operationsPage(tenant:string,offset:number,limit:number):{records:OperationRecord[];total:number;offset:number;nextOffset:number|null};
 review(tenant:string,input:ReviewInput):{head:Head;candidates:unknown[];conflicts:unknown[]};
 prepareReview(tenant:string,inputs:readonly ReviewInput[],operationId:Id):PreparedChange;
 prepareConversationSend(tenant:string,inputs:readonly SendInput[],operation:Operation):PreparedChange;
 prepareObservedSend(tenant:string,intent:SendIntent,logicalId:Id,surface:string,siteId:string,operation:Operation):PreparedChange;
 prepareDefinitelyNotSent(tenant:string,intent:SendIntent,logicalId:Id,operation:Operation):PreparedChange;
 preparePolicyChanges(tenant:string,edits:PolicyEdits,operationId:Id):PreparedChange;
 prepareRecovery(tenant:string,operationId:Id):PreparedChange;
 commit(tenant:string,change:PreparedChange,approved:true,requestId:Id):Committed;
 reconcile(tenant:string,requestId:Id):Receipt;
 restore(tenant:string,text:string):string;
 restoreReport(tenant:string,text:string):RestoreReport;
 restoreInConversation(tenant:string,text:string,logicalId:Id):string;
 restoreReportInConversation(tenant:string,text:string,logicalId:Id):RestoreReport;
 bufferedRestore(tenant:string,encoding?:'utf8'|'utf16',logicalId?:Id):RestoreStream;
 progressiveRestore(tenant:string,encoding?:'utf8'|'utf16',logicalId?:Id):RestoreStream;
 lock(tenant:string):void;
 close(tenant:string):void;
}
