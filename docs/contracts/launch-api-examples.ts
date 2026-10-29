/** Compile-only examples. Future types do not manufacture runtime methods. */
import type {Workspace, WorkspaceHead, WorkspacePreview} from "../../bindings/wasm/index.js";
import type {
  BackupHostDraft, FindingDraft, LaunchWorkspaceDraft, OperationContextDraft,
  PreparedChangeDraft, ReviewInputDraft,
} from "./launch-api-proposal.js";

// This is a trusted-host boundary, not a Core promise of encryption/durability.
type SaveCandidate = (bytes: Uint8Array, expected: WorkspaceHead, candidate: WorkspaceHead) => Promise<WorkspaceHead>;

export async function currentBeta1(workspace: Workspace, save: SaveCandidate, approve: (preview: WorkspacePreview) => Promise<boolean>) {
  const plan = workspace.prepare("Jane", [{
    entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4},
  }], crypto.getRandomValues(new Uint8Array(16)));
  const preview = plan.preview();
  if (!(await approve(preview))) { plan.discard(); return; }
  const heads = plan.heads();
  const candidate = plan.exportCandidateBundle();
  try {
    const savedHead = await save(candidate, heads.expectedHead, heads.candidateHead);
    workspace.activateSaved(plan, heads.expectedHead, savedHead);
    return preview.text;
  } finally { candidate.fill(0); }
}

async function activateDraft(workspace: LaunchWorkspaceDraft, plan: PreparedChangeDraft, save: SaveCandidate) {
  const heads = plan.heads(), candidate = plan.exportCandidateBundle();
  try {
    const savedHead = await save(candidate, heads.expectedHead, heads.candidateHead);
    workspace.activateSaved(plan, heads.expectedHead, savedHead);
  } finally { candidate.fill(0); }
}

// The future host must obtain this object from a reviewed future build, never by casting beta.1.
export async function proposedReview(
  workspace: LaunchWorkspaceDraft, input: ReviewInputDraft,
  operation: OperationContextDraft, save: SaveCandidate,
  approve: (previews: ReturnType<PreparedChangeDraft["previews"]>) => Promise<boolean>,
) {
  const review = workspace.review(input);
  if (review.conflicts.length) return {kind: "edit_required" as const, review};
  const plan = workspace.prepareReview([input], operation); // Revalidates policy/head and recomputes recall.
  const previews = plan.previews();
  if (!(await approve(previews))) { plan.discard(); return {kind: "cancelled" as const}; }
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

export function proposedRecall(workspace: LaunchWorkspaceDraft, text: string): readonly FindingDraft[] {
  return workspace.recall(text).findings.map(span => span.finding);
}

export function proposedBackup(host: BackupHostDraft, workspaceId: Uint8Array, newlyEnteredPassword: string) {
  return host.exportBackup({workspaceId, freshPassword: newlyEnteredPassword, includeRecoveryCode: true});
}

// Negative assertions stop a proposal from being mistaken for installed runtime support.
export function typeBoundaries(current: Workspace, future: LaunchWorkspaceDraft, backup: BackupHostDraft) {
  // @ts-expect-error beta.1 has no recall method.
  current.recall("Jane");
  // @ts-expect-error beta.1 has no conversation-aware restore method.
  current.restoreInConversation("text", {surface: "chatgpt", siteConversationId: "synthetic"});
  // @ts-expect-error closed source names are required in the proposal.
  const unsupportedSource: FindingDraft = {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}, source: "trusted"};
  // @ts-expect-error future interfaces are not an implementation of the current Workspace class.
  const cannotSubstitute: LaunchWorkspaceDraft = current;
  // @ts-expect-error explicit fresh password input is required for host export.
  backup.exportBackup({workspaceId: new Uint8Array(16), includeRecoveryCode: false});
  // @ts-expect-error choice values remain a closed set.
  future.preparePolicyChange([{kind: "remember_choice", entityType: "CUSTOM", value: "synthetic", disposition: "allow_everything"}], {id: new Uint8Array(16), occurredAtMs: 0});
  void unsupportedSource; void cannotSubstitute;
}
