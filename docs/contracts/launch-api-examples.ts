/** Compile-only examples. Future types do not manufacture runtime methods. */
import type {PreparedWorkspaceChange, Workspace, WorkspaceFinding, WorkspaceHead, WorkspacePreview} from "../../bindings/wasm/index.js";
import type {
  BackupHostDraft, CandidateFinding, LaunchWorkspaceDraft, OperationContextDraft,
  ReviewInputCandidate, ReviewWorkspaceCandidate,
} from "./launch-api-proposal.js";

// This is a trusted-host boundary, not a Core promise of encryption/durability.
type SaveCandidate = (bytes: Uint8Array, expected: WorkspaceHead, candidate: WorkspaceHead) => Promise<WorkspaceHead>;

// Main now includes policy-format metadata; this is not a beta.2 release claim.
type VersionedPlan = PreparedWorkspaceChange;
function upgradeWarning(plan: VersionedPlan): string | null {
  const format = plan.snapshotFormat();
  return format?.currentVersion === 1 && format.candidateVersion === 2
    ? "Saving upgrades this vault. 0.5.0-beta.1 cannot open the upgraded vault." : null;
}

// Existing lifecycle shape, compiled against this checkout (not a beta.2 release claim).
export async function currentRuntime(workspace: Workspace, save: SaveCandidate, approve: (preview: WorkspacePreview, oldReaderWarning: string | null) => Promise<boolean>) {
  const plan: VersionedPlan = workspace.prepare("Jane", [{
    entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4},
  }], crypto.getRandomValues(new Uint8Array(16)));
  const preview = plan.preview();
  if (!(await approve(preview, upgradeWarning(plan)))) { plan.discard(); return; }
  const heads = plan.heads();
  const candidate = plan.exportCandidateBundle();
  try {
    const savedHead = await save(candidate, heads.expectedHead, heads.candidateHead);
    workspace.activateSaved(plan, heads.expectedHead, savedHead);
    return preview.text;
  } finally { candidate.fill(0); }
}

async function activateDraft(workspace: ReviewWorkspaceCandidate, plan: PreparedWorkspaceChange, save: SaveCandidate) {
  const heads = plan.heads(), candidate = plan.exportCandidateBundle();
  try {
    const savedHead = await save(candidate, heads.expectedHead, heads.candidateHead);
    workspace.activateSaved(plan, heads.expectedHead, savedHead);
  } finally { candidate.fill(0); }
}

// Obtain the #173 candidate from its real build; main does not export review yet.
export async function candidateReview(
  workspace: ReviewWorkspaceCandidate, input: ReviewInputCandidate,
  operationId: Uint8Array, save: SaveCandidate,
  approve: (previews: ReturnType<PreparedWorkspaceChange["previews"]>, oldReaderWarning: string | null) => Promise<boolean>,
) {
  const review = workspace.review(input);
  if (review.conflicts.length) return {kind: "edit_required" as const, review};
  const plan = workspace.prepareReview([input], operationId); // Revalidates policy/head and recomputes recall.
  const previews = plan.previews();
  if (!(await approve(previews, upgradeWarning(plan)))) { plan.discard(); return {kind: "cancelled" as const}; }
  await activateDraft(workspace, plan, save);
  return {kind: "approved_text" as const, text: previews[0].text};
}

export async function proposedHandoffRecord(
  workspace: LaunchWorkspaceDraft, releasedText: string,
  operation: OperationContextDraft, save: SaveCandidate,
  insertAndAcknowledge: (text: string) => Promise<{siteConversationId: string}>,
) {
  // Called only with approved text after its mappings were durably saved and activated.
  const receipt = await insertAndAcknowledge(releasedText);
  // The insertion happened first. A crash/save failure here leaves an unrecorded handoff.
  const record = workspace.prepareHandoffRecord({
    conversation: {surface: "chatgpt", siteConversationId: receipt.siteConversationId},
    action: "insert", releasedText,
  }, operation);
  await activateDraft(workspace, record, save);
}

export function currentRecall(workspace: Workspace, text: string): readonly WorkspaceFinding[] {
  return workspace.recall(text).findings.map(span => span.finding);
}

// Host must explain the irreversible old-reader incompatibility before saving v1 -> v2.
export async function currentPolicy(
  workspace: Workspace, save: SaveCandidate,
  approve: (details: {policy: ReturnType<ReturnType<Workspace["preparePolicyChanges"]>["policyPreview"]>; oldReaderWarning: string | null}) => Promise<boolean>,
) {
  const plan = workspace.preparePolicyChanges({rules: [], choices: [{
    entityType: "PERSON", value: "Synthetic Example", disposition: "not_sensitive",
  }]}, crypto.getRandomValues(new Uint8Array(16)));
  const oldReaderWarning = upgradeWarning(plan);
  if (!(await approve({policy: plan.policyPreview(), oldReaderWarning}))) { plan.discard(); return; }
  const heads = plan.heads(), candidate = plan.exportCandidateBundle();
  try {
    const savedHead = await save(candidate, heads.expectedHead, heads.candidateHead);
    workspace.activateSaved(plan, heads.expectedHead, savedHead);
  } finally { candidate.fill(0); }
}

export function proposedBackup(host: BackupHostDraft, workspaceId: Uint8Array, newlyEnteredPassword: string) {
  return host.exportBackup({workspaceId, freshPassword: newlyEnteredPassword, includeRecoveryCode: true});
}

// Negative assertions stop a proposal from being mistaken for installed runtime support.
export function typeBoundaries(current: Workspace, future: LaunchWorkspaceDraft, backup: BackupHostDraft) {
  // @ts-expect-error conversation-aware restore remains future-only in this checkout.
  current.restoreInConversation("text", {surface: "chatgpt", siteConversationId: "synthetic"});
  // @ts-expect-error closed source names are required in the proposal.
  const unsupportedSource: CandidateFinding = {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}, source: "trusted"};
  // @ts-expect-error future interfaces are not an implementation of the current Workspace class.
  const cannotSubstitute: LaunchWorkspaceDraft = current;
  // @ts-expect-error explicit fresh password input is required for host export.
  backup.exportBackup({workspaceId: new Uint8Array(16), includeRecoveryCode: false});
  // @ts-expect-error the committed review API uses candidateKey, not target.
  const obsoleteCorrection: ReviewInputCandidate = {text: "Jane", scannerFindings: [], userSelections: [], corrections: [{target: "old", replacement: {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}}}]};
  void obsoleteCorrection;
  // @ts-expect-error choice values remain a closed set.
  future.preparePolicyChanges({rules: [], choices: [{entityType: "CUSTOM", value: "synthetic", disposition: "allow_everything"}]}, new Uint8Array(16));
  void unsupportedSource; void cannotSubstitute;
}
