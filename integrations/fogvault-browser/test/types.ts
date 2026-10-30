import {Workspace, type WorkspaceReviewInput} from '@datafog/wasm';
import {FogVault} from '@datafog/fogvault-browser';
import {FogVaultWorkspace, type WorkspaceVaultPlan} from '@datafog/core-fogvault-browser';

async function consumer(vault: FogVault, recordId: Uint8Array, namespace: Uint8Array, input: WorkspaceReviewInput) {
  const owner = await FogVaultWorkspace.create({Workspace, vault, recordId, namespace});
  const plan: WorkspaceVaultPlan = owner.prepareReview([input], crypto.getRandomValues(new Uint8Array(16)));
  const saved = await owner.commit(plan, {approved: true});
  const exact: bigint = saved.recordRevision;
  const head: string = saved.head.revision;
  owner.restoreReport(saved.previews[0].text);
  // @ts-expect-error Explicit literal approval is required.
  await owner.commit(plan, {approved: false});
  // @ts-expect-error Raw Core handles are not exposed by the adapter.
  owner.workspace;
  // @ts-expect-error Sensitive plaintext bundle exports are not exposed by wrapped plans.
  plan.exportCandidateBundle();
  await owner.lock();
  return {exact, head};
}
void consumer;
