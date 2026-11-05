import {Workspace} from '@datafog/wasm';
import {SQLiteVault, SQLiteWorkspace, type WorkspaceVaultPlan, type SavedWorkspaceChange} from '@datafog/core-sqlite-browser';
async function exercise(password: string, bundle: Uint8Array, recordId: Uint8Array, namespace: Uint8Array) {
  const vault = await SQLiteVault.create({id: 'aa'.repeat(16), password, recoveryKey: new Uint8Array(32)});
  const owner = await SQLiteWorkspace.create({Workspace, vault, recordId, namespace});
  const plan: WorkspaceVaultPlan = owner.prepareReview([], new Uint8Array(16));
  const saved: SavedWorkspaceChange = await owner.commit(plan, {approved: true});
  const forgotten: WorkspaceVaultPlan = owner.prepareForget(['ABC123'], new Uint8Array(16));
  forgotten.discard();
  const archive: Uint8Array = await owner.exportBackup({password});
  await owner.changePassword({password, recoveryKey: new Uint8Array(32)});
  owner.restore(saved.previews[0].text);
  await owner.lock();
  const destination = await SQLiteVault.create({id: 'bb'.repeat(16), password});
  const restored = await SQLiteWorkspace.importBackup({Workspace, vault: destination, recordId, backup: archive, password, approved: true});
  await restored.close();
  const recovery = await SQLiteVault.open({id: 'aa'.repeat(16), recoveryKey: new Uint8Array(32)});
  await recovery.reconcileLatest(recordId);
  const reopened = await SQLiteWorkspace.open({Workspace, vault: recovery, recordId, expectedNamespace: namespace, expectedHead: saved.head});
  await reopened.close();
  void bundle;
}
void exercise;
