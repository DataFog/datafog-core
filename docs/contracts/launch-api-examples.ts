/** Compile-only actual candidate consumers. No provider, storage or future API runs. */
import type {
  Workspace as BrowserWorkspace, DataFogError as BrowserError,
} from "../../bindings/wasm/index.js";
import type {
  Workspace as NodeWorkspace, DataFogError as NodeError,
} from "../../bindings/node/index.js";
import type {
  CandidateWorkspace, CandidatePlan, WorkspaceFinding, WorkspaceInput,
  WorkspaceHead, WorkspacePreview, WorkspaceReviewInput, WorkspaceOperation,
  WorkspaceOperationRecord, WorkspaceOperationPage, WorkspaceRestoreProgress,
  WorkspaceConversationInput, WorkspaceConversationState, WorkspaceSendIntent, WorkspaceSendStatus,
} from "./launch-api-proposal.js";

// Trusted host obligation, not an SDK promise of approval/encryption/durability.
type SaveCandidate = (bytes: Uint8Array, expected: WorkspaceHead, candidate: WorkspaceHead) => Promise<WorkspaceHead>;
type Approve = (previews: readonly WorkspacePreview[], oldReaderWarning: string | null) => Promise<boolean>;

function upgradeWarning(plan: CandidatePlan): string | null {
  const {currentVersion, candidateVersion} = plan.snapshotFormat();
  if (currentVersion < 3 && candidateVersion === 3)
    return "Saving upgrades this vault. 0.5.0-beta.1 and 0.5.0-beta.2 cannot open v3.";
  if (currentVersion === 1 && candidateVersion === 2)
    return "Saving upgrades this vault. 0.5.0-beta.1 cannot open v2.";
  return null;
}

function discardPending(plan: CandidatePlan): void {
  try { plan.discard(); }
  catch (error) {
    // Already invalidated/closed plans have no retained candidate to release.
    // Do not suppress an unexpected cleanup failure.
    const field = error && typeof error === "object" ? Object.getOwnPropertyDescriptor(error, "code") : undefined;
    if (field?.value !== "inactive_plan" && field?.value !== "closed_workspace") throw error;
  }
}

async function approveAndSave(workspace: CandidateWorkspace, plan: CandidatePlan, save: SaveCandidate, approve: Approve) {
  let consumed = false;
  try {
    const previews = plan.previews();
    if (!(await approve(previews, upgradeWarning(plan)))) return {kind: "cancelled" as const};
    const {expectedHead, candidateHead} = plan.heads();
    const bytes = plan.exportCandidateBundle();
    try {
      // The save callback must perform authorized complete-head CAS and resolve
      // only after the durable saved head is confirmed. Lost acknowledgment rejects.
      const saved = await save(bytes, expectedHead, candidateHead);
      workspace.activateSaved(plan, expectedHead, saved);
      consumed = true;
      return {kind: "approved_text" as const, previews};
    } finally { bytes.fill(0); }
  } finally { if (!consumed) discardPending(plan); }
}

/** Combined review is the launch path; source flags do not authorize disclosure. */
export async function currentReview(
  workspace: CandidateWorkspace, input: WorkspaceReviewInput, operationId: Uint8Array,
  save: SaveCandidate, approve: Approve,
) {
  const review = workspace.review(input);
  if (review.conflicts.length) return {kind: "edit_required" as const, review};
  // Core recomputes recall/policy/head; the previous list is not an authority.
  return approveAndSave(workspace, workspace.prepareReview([input], operationId), save, approve);
}

/** Explicit supplied-finding recording is a separate actual API, without auto-recall. */
export function recordedPreparation(
  workspace: CandidateWorkspace, inputs: readonly WorkspaceInput[], operation: WorkspaceOperation,
  save: SaveCandidate, approve: Approve,
) {
  return approveAndSave(workspace, workspace.prepareBatchWithOperation(inputs, operation), save, approve);
}

/** No init() is needed for the native factory; this bootstrap still needs persistence. */
export function nativeFactory(Workspace: typeof NodeWorkspace, namespace: Uint8Array, operation: WorkspaceOperation): NodeWorkspace {
  return Workspace.createWithOperation(namespace, operation);
}
/** Only browser WASM needs awaited initialization. No bootstrap is implicitly saved. */
export async function browserFactory(
  Workspace: typeof BrowserWorkspace, init: () => Promise<unknown>, namespace: Uint8Array, operation: WorkspaceOperation,
): Promise<BrowserWorkspace> {
  await init();
  return Workspace.createWithOperation(namespace, operation);
}

export function typedInventory(browser: BrowserWorkspace, node: NodeWorkspace, operation: WorkspaceOperation) {
  // These assignments compare all declared public instance calls in both directions.
  const shared: readonly CandidateWorkspace[] = [browser, node];
  for (const workspace of shared) {
    const rows: readonly WorkspaceOperationRecord[] = workspace.operations();
    const page: WorkspaceOperationPage = workspace.operationsPage(0, 1000);
    const unknownTime: number | null = rows[0]?.occurredAtMs ?? null;
    const next: number | null = page.nextOffset;
    const covered: true = workspace.historyCoverage().recordedHistoryOnly;
    const prior: "unknown" | "not_applicable" = workspace.historyCoverage().preUpgradeHistory;
    const confirmedHandoffs: number = workspace.counts().confirmedHandoffs; // Saved observed-send rows only.
    // Outcomes/recovery have no text previews or external-delivery authority.
    workspace.prepareOutcome({kind: "user_cancelled", issueCount: 0}, operation).discard();
    workspace.prepareRecoveryWithOperation(operation).discard();
    void unknownTime; void next; void covered; void prior; void confirmedHandoffs;
  }
}

/** Explicit pages are complete only relative to the unchanged head the host owns. */
export function visitPages(workspace: CandidateWorkspace, consume: (page: WorkspaceOperationPage) => void): void {
  let offset = 0;
  while (true) {
    const page = workspace.operationsPage(offset, 1000);
    consume(page); // Process one page; retaining every page defeats bounded host memory.
    if (page.nextOffset === null) return;
    offset = page.nextOffset;
  }
}

/** Strict default returns no feed text and never exposes a failed response prefix. */
export function strictResponse(workspace: CandidateWorkspace, chunks: Iterable<Uint8Array>): string | null {
  const response = workspace.bufferedRestore();
  try {
    for (const chunk of chunks) response.feed(chunk);
    return response.finalize().text; // Null means review required, never empty success.
  } finally { response.close(); }
}

/** The caller explicitly accepts irretractable output; host callback errors propagate incomplete. */
export function progressiveResponse(
  workspace: CandidateWorkspace, chunks: Iterable<Uint8Array>, release: (text: string) => void,
): WorkspaceRestoreProgress {
  const response = workspace.progressiveRestore();
  try {
    for (const chunk of chunks) release(response.feed(chunk).text);
    const final = response.finalize();
    release(final.text);
    return final.delivered;
  } finally { response.close(); }
  // A typed SDK terminal error carries prior delivered counts + complete:false.
  // Never retry this whole function or claim previously released text was retracted.
}

/** Call only after narrowing an actual SDK error; never forward provider error messages. */
export function typedFailure(error: BrowserError | NodeError) {
  const policyFailure: boolean = error.code === "invalid_policy";
  const index: number | null | undefined = error.inputIndex;
  const incomplete: false | undefined = error.complete;
  const priorBytes: number | undefined = error.delivered?.bytes;
  return {code: error.code, policyFailure, index, incomplete, priorBytes};
}

export function currentRecall(workspace: CandidateWorkspace, text: string): readonly WorkspaceFinding[] {
  return workspace.recall(text).findings.map(span => span.finding);
}

// Negative checks describe unsupported calls, not promises of their future names.
export function typeBoundaries(workspace: CandidateWorkspace, operation: WorkspaceOperation) {
  // @ts-expect-error There is no plural enumeration API; use exact logical/site lookup.
  workspace.conversations();
  // @ts-expect-error Scoped restore exists but requires the exact Uint8Array logical ID.
  workspace.restoreInConversation("synthetic", {});
  // @ts-expect-error No timed combined-review method exists.
  workspace.prepareReviewWithOperation([], operation);
  // @ts-expect-error Page inputs are numbers; runtime also enforces integer bounds.
  workspace.operationsPage("0", 1000);
  // @ts-expect-error Existing explicit outcomes do not include observed-send records.
  workspace.prepareOutcome({kind: "handoff_confirmed", issueCount: 0}, operation);
  // @ts-expect-error Contexts cannot acquire free-form content.
  workspace.prepareWithOperation("", [], {...operation, text: "forbidden"});
  // @ts-expect-error Sources are closed display metadata, never trusted precedence.
  const source: WorkspaceFinding = {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}, source: "trusted"};
  // @ts-expect-error Corrections use candidateKey, not the old target field.
  const correction: WorkspaceReviewInput = {text: "Jane", scannerFindings: [], userSelections: [], corrections: [{target: "old", replacement: {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}}}]};
  void source; void correction;
}


/** First stage only: return approved text/intent IDs after the pending state saves.
 * The trusted host must correlate transport to this exact saved attempt. */
export async function saveConversationPreparation(
  workspace: CandidateWorkspace, inputs: readonly WorkspaceConversationInput[], operation: WorkspaceOperation,
  save: SaveCandidate, approve: Approve,
) {
  const plan = workspace.prepareConversationSend(inputs, operation);
  const intents: readonly WorkspaceSendIntent[] = plan.sendIntents(); // Before activation consumes it.
  const result = await approveAndSave(workspace, plan, save, approve);
  return result.kind === "cancelled" ? null : {previews: result.previews, intents};
}

/** Call only for a trusted actual-send observation of the saved exact attempt.
 * Approval alone cannot invent that observation; this function sends nothing. */
export async function saveObservedConversationSend(
  workspace: CandidateWorkspace, intent: WorkspaceSendIntent, logicalId: Uint8Array,
  surface: string, siteId: string, operation: WorkspaceOperation,
  save: SaveCandidate, approve: Approve,
): Promise<boolean> {
  const plan = workspace.prepareObservedSend(intent, logicalId, surface, siteId, operation);
  const result = await approveAndSave(workspace, plan, save, approve);
  return result.kind !== "cancelled"; // No text previews or resend permission.
}

/** Definite non-delivery is a separate trusted assertion, never a timeout fallback.
 * This example discards the staged resolution; it does not clear a saved pause. */
export function stageDefiniteNonDelivery(
  workspace: CandidateWorkspace, intent: WorkspaceSendIntent, logicalId: Uint8Array, operation: WorkspaceOperation,
): void {
  workspace.prepareDefinitelyNotSent(intent, logicalId, operation).discard();
}

/** Invoke against a confirmed context; pending contexts reject even ordinary text.
 * Existing unscoped methods are compatibility calls, never a fallback here. */
export function scopedConversationTypes(
  workspace: CandidateWorkspace, logicalId: Uint8Array, surface: string, siteId: string,
  intent: WorkspaceSendIntent, text: string,
) {
  const state: WorkspaceConversationState = workspace.conversation(logicalId);
  const binding: Uint8Array = workspace.conversationForSite(surface, siteId);
  const status: WorkspaceSendStatus = workspace.sendIntentStatus(intent);
  const restored: string = workspace.restoreInConversation(text, logicalId);
  const report = workspace.restoreReportInConversation(text, logicalId);
  const strict = workspace.bufferedRestoreInConversation(logicalId, "utf8");
  try {
    const progressive = workspace.progressiveRestoreInConversation(logicalId, "utf16");
    try { return {state, binding, status, restored, report}; }
    finally { progressive.close(); }
  } finally { strict.close(); }
}

/** All four actual C2 codes belong to both JavaScript SDK declarations. */
export function conversationFailureCodes(error: BrowserError | NodeError) {
  const codes: readonly (BrowserError["code"] & NodeError["code"])[] = [
    "invalid_conversation", "conversation_unknown", "conversation_pending", "invalid_send_intent",
  ];
  return codes.some(code => code === error.code);
}
