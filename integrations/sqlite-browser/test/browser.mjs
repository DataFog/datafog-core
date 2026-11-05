import {init, Workspace, scan} from './sdk/wasm/index.js';
import {SQLiteVault, SQLiteWorkspace} from './sdk/core-sqlite-browser/index.mjs';
await init();
const password = 'synthetic password for qualification';
const id = 'aa'.repeat(16), recordId = new Uint8Array(16).fill(2), namespace = new Uint8Array(16).fill(3);
const rotatedPassword = 'rotated synthetic password';
const recovery = new Uint8Array(32).fill(7);
const random = () => crypto.getRandomValues(new Uint8Array(16));
const check = (value, message) => { if (!value) throw Error(message); };
const rejects = async (action, code) => { try { await action(); } catch (error) { check(!code || error.code === code, `expected ${code}, received ${error.code}`); return; } throw Error('expected rejection'); };
globalThis.runFirst = async () => {
  const cases = [];
  const vault = await SQLiteVault.create({id, password});
  const owner = await SQLiteWorkspace.create({Workspace, vault, recordId, namespace});
  const text = 'Email alice@example.test';
  const findings = scan(text).map(f => ({entityType: f.entityType, matchedText: f.matchedText, range: {unit: 'utf8_bytes', ...f.byteRange}, detectorName: f.detectorName}));
  const plan = owner.prepareReview([{text, scannerFindings: findings, userSelections: []}], random());
  await rejects(() => owner.commit(plan, {approved: false}), 'approval_required');
  const saved = await owner.commit(plan, {approved: true});
  check(owner.restore(saved.previews[0].text) === text, 'restore failed');
  localStorage.setItem('protected', saved.previews[0].text);
  await rejects(() => vault.read(recordId), 'ownership_transferred');
  cases.push('raw_provider_mutation_rejected_after_transfer');
  cases.push('approved_save_before_activation_and_exact_restore');
  await owner.lock();
  await rejects(() => owner.restore(saved.previews[0].text), 'owner_closed');
  await rejects(() => SQLiteVault.open({id, password: 'wrong password value'}));
  cases.push('lock_revokes_owner_and_wrong_password_rejected');
  const reopenedVault = await SQLiteVault.open({id, password});
  const reopened = await SQLiteWorkspace.open({Workspace, vault: reopenedVault, recordId, expectedNamespace: namespace});
  check(reopened.restore(saved.previews[0].text) === text, 'reopen restore failed');
  const backup = await reopened.exportBackup({password});
  await reopened.changePassword({password: rotatedPassword, recoveryKey: recovery});
  await reopened.close();
  await rejects(() => SQLiteVault.open({id, password}));
  const recoveryVault = await SQLiteVault.open({id, recoveryKey: recovery});
  const recovered = await SQLiteWorkspace.open({Workspace, vault: recoveryVault, recordId, expectedNamespace: namespace});
  check(recovered.restore(saved.previews[0].text) === text, 'recovery restore failed');
  await recovered.close();
  cases.push('password_rotation_and_independent_recovery');
  const destination = await SQLiteVault.create({id: 'bb'.repeat(16), password});
  const imported = await SQLiteWorkspace.importBackup({Workspace, vault: destination, recordId: random(), backup, password, approved: true});
  check(imported.restore(saved.previews[0].text) === text, 'backup restore failed');
  check(imported.head().revision !== saved.head.revision || JSON.stringify(imported.head().commitId) !== JSON.stringify(saved.head.commitId), 'recovery did not create a fresh head');
  await imported.close();
  const corrupt = backup.slice(); corrupt[corrupt.length - 15] ^= 1;
  const spare = await SQLiteVault.create({id: 'cc'.repeat(16), password});
  await rejects(() => SQLiteWorkspace.importBackup({Workspace, vault: spare, recordId: random(), backup: corrupt, password, approved: true}), 'invalid_credentials');
  await spare.close();
  cases.push('authenticated_encrypted_backup_fresh_destination_recovery_and_tamper_rejection');
  cases.push('encrypted_opfs_close_and_reopen');
  return cases;
};
globalThis.runForget = async () => {
  const text = 'Email alice@example.test', expiredId = 'ee'.repeat(16);
  const vault = await SQLiteVault.create({id: expiredId, password});
  const owner = await SQLiteWorkspace.create({Workspace, vault, recordId, namespace});
  const prepare = target => target.prepareReview([{text, scannerFindings: scan(text).map(f => ({entityType: f.entityType, matchedText: f.matchedText,
    range: {unit: 'utf8_bytes', ...f.byteRange}, detectorName: f.detectorName})), userSelections: []}], random());
  const saved = await owner.commit(prepare(owner), {approved: true});
  const marker = saved.previews[0].text.match(/\[\[EMAIL_[0-9A-HJKMNP-TV-Z]{6}\]\]/)[0];
  const code = marker.slice(2, -2).split('_').at(-1);
  const discarded = owner.prepareForget([code], random()); discarded.discard();
  check(owner.restore(marker) === 'alice@example.test', 'discard applied retirement');
  const stream = owner.bufferedRestore(); stream.feed(new TextEncoder().encode(marker));
  const plan = owner.prepareForget([code], random());
  check(plan.snapshotFormat().candidateVersion === 4 && plan.previews().length === 0, 'retirement plan metadata incorrect');
  await rejects(() => owner.commit(plan, {approved: false}), 'approval_required');
  check(owner.restore(marker) === 'alice@example.test', 'rejected approval applied retirement');
  const forgotten = await owner.commit(plan, {approved: true});
  check(forgotten.previews.length === 0, 'forget released a preview');
  await rejects(() => stream.finalize(), 'workspace_conflict'); stream.close();
  await rejects(() => owner.restore(marker), 'invalid_marker');
  await owner.lock();
  const reopenedVault = await SQLiteVault.open({id: expiredId, password});
  const reopened = await SQLiteWorkspace.open({Workspace, vault: reopenedVault, recordId, expectedNamespace: namespace, expectedHead: forgotten.head});
  const report = reopened.restoreReport(`👋 ${marker}`);
  check(report.text === null && report.issues[0].kind === 'expired', 'encrypted reopen revived retired marker');
  check(report.issues[0].byteRange.start === 5 && report.issues[0].codepointRange.start === 2 && report.issues[0].utf16Range.start === 3, 'expired marker source offsets changed');
  check(reopened.recall('alice@example.test').findings.length === 0, 'retired value recalled');
  await rejects(() => reopened.prepareForget([code], random()), 'invalid_marker');
  const again = await reopened.commit(prepare(reopened), {approved: true});
  check(!again.previews[0].text.includes(marker) && reopened.restore(again.previews[0].text) === text, 'retired code reused');
  check(reopened.restoreReport(marker).issues[0].kind === 'expired', 'returning value revived old marker');
  localStorage.setItem('retired-marker', marker);
  await reopened.close();
  return ['forget_persists_retirement_revokes_streams_and_does_not_reissue_codes'];
};
globalThis.runConversation = async () => {
  const text = 'Email conversation@example.test', conversationId = random(), surface = 'synthetic-sqlite-no-network';
  const vault = await SQLiteVault.create({id: 'ff'.repeat(16), password});
  const owner = await SQLiteWorkspace.create({Workspace, vault, recordId, namespace});
  const findings = scan(text).map(f => ({entityType: f.entityType, matchedText: f.matchedText,
    range: {unit: 'utf8_bytes', ...f.byteRange}, detectorName: f.detectorName}));
  const plan = owner.prepareConversationSend([{input: {text, scannerFindings: findings, userSelections: []},
    conversation: {id: conversationId, surface, siteId: null, siteIdByteBudget: 64}}], {id: random(), occurredAtMs: 1});
  const intents = plan.sendIntents();
  check(intents.length === 1, 'missing conversation intent');
  const saved = await owner.commit(plan, {approved: true});
  check(owner.sendIntentStatus(intents[0]) === 'pending', 'save automatically observed send');
  await rejects(() => owner.restoreInConversation(saved.previews[0].text, conversationId));
  await owner.close();
  const reopenedVault = await SQLiteVault.open({id: 'ff'.repeat(16), password});
  const reopened = await SQLiteWorkspace.open({Workspace, vault: reopenedVault, recordId, expectedNamespace: namespace, expectedHead: saved.head});
  check(reopened.sendIntentStatus(intents[0]) === 'pending', 'reopen automatically observed send');
  await rejects(() => reopened.restoreInConversation(saved.previews[0].text, conversationId));
  localStorage.setItem('conversation-pending', JSON.stringify({text, protectedText: saved.previews[0].text,
    conversationId: Array.from(conversationId), intent: {preparationId: Array.from(intents[0].preparationId), inputIndex: intents[0].inputIndex}, surface}));
  await reopened.close();
  return ['conversation_pending_save_survives_encrypted_reopen_without_scoped_restore'];
};
globalThis.runFaults = async () => {
  const cases = [];
  const text = 'Email fault@example.test';
  const input = () => [{text, scannerFindings: scan(text).map(f => ({entityType: f.entityType, matchedText: f.matchedText, range: {unit: 'utf8_bytes', ...f.byteRange}, detectorName: f.detectorName})), userSelections: []}];
  for (const [number, fault] of ['lost', 'quota', 'late', 'crash'].entries()) {
    const vaultId = (number + 4).toString(16).repeat(32);
    const initial = await SQLiteVault.create({id: vaultId, password});
    const empty = await SQLiteWorkspace.create({Workspace, vault: initial, recordId, namespace});
    await empty.close();
    const NativeWorker = globalThis.Worker;
    let saved;
    const savedPromise = new Promise(resolve => { saved = resolve; });
    globalThis.Worker = class extends NativeWorker {
      constructor(...args) { super(...args); this.addEventListener('message', event => { if (event.data.testSaved) saved(); }); }
    };
    let vault;
    try { vault = await SQLiteVault.open({id: vaultId, password, workerUrl: new URL(`./fault-worker.mjs?fault=${fault}`, location.href)}); }
    finally { globalThis.Worker = NativeWorker; }
    const owner = await SQLiteWorkspace.open({Workspace, vault, recordId, expectedNamespace: namespace});
    const request = random();
    const plan = owner.prepareReview(input(), random());
    const committing = owner.commit(plan, {approved: true, requestId: request});
    // Attach a rejection handler before a lock may synchronously revoke the Worker.
    const outcome = committing.then(() => false, failure => failure);
    if (fault === 'late') { await savedPromise; await owner.lock(); }
    check(await outcome, 'fault authorized handoff');
    check(owner.closed, 'fault kept owner active');
    const faultDirectory = await (await navigator.storage.getDirectory()).getDirectoryHandle(`.datafog-sqlite-${vaultId}`);
    async function inspectArtifacts(directory) {
      for await (const entry of directory.values()) {
        if (entry.kind === 'directory') await inspectArtifacts(entry);
        else check(!new TextDecoder().decode(await (await entry.getFile()).arrayBuffer()).includes('fault@example.test'), 'plaintext in interrupted database/journal');
      }
    }
    await inspectArtifacts(faultDirectory);
    const fresh = await SQLiteVault.open({id: vaultId, password});
    const reconciliation = await fresh.reconcileLatest(request);
    check(reconciliation.found === (!['quota', 'crash'].includes(fault)), 'wrong durable reconciliation');
    const reopened = await SQLiteWorkspace.open({Workspace, vault: fresh, recordId, expectedNamespace: namespace});
    check(reopened.head().revision === (['quota', 'crash'].includes(fault) ? '0' : '1'), 'partial candidate after fault');
    await reopened.close();
    cases.push(`${fault}_save_never_grants_handoff_and_reopens_complete_state`);
  }
  const credentialVault = await SQLiteVault.open({id, password: rotatedPassword, workerUrl: new URL('./fault-worker.mjs?fault=credential', location.href)});
  const credentialOwner = await SQLiteWorkspace.open({Workspace, vault: credentialVault, recordId, expectedNamespace: namespace});
  await rejects(() => credentialOwner.changePassword({password: 'should never replace existing credentials'}), 'storage_failed');
  check(credentialOwner.closed && credentialVault.closed, 'credential failure retained lock authority');
  const oldCredential = await SQLiteVault.open({id, password: rotatedPassword});
  await oldCredential.close();
  await rejects(() => SQLiteVault.open({id, password: 'should never replace existing credentials'}), 'invalid_credentials');
  cases.push('failed_credential_write_closes_owner_and_preserves_existing_credentials');
  const validationVault = await SQLiteVault.open({id, password: rotatedPassword});
  await rejects(() => SQLiteWorkspace.open({Workspace, vault: validationVault, recordId: new Uint8Array(16)}), 'invalid_identity');
  await validationVault.close();
  cases.push('invalid_options_do_not_transfer_or_strand_vault_ownership');
  const interruptedId = '88'.repeat(16);
  await rejects(() => SQLiteVault.create({id: interruptedId, password, workerUrl: new URL('./fault-worker.mjs?fault=init', location.href)}));
  const resumed = await SQLiteVault.open({id: interruptedId, password});
  const resumedOwner = await SQLiteWorkspace.create({Workspace, vault: resumed, recordId, namespace});
  await resumedOwner.close();
  cases.push('interrupted_bootstrap_resumes_only_after_credential_authentication');
  const exclusive = await SQLiteVault.open({id, password: rotatedPassword});
  await rejects(() => SQLiteVault.open({id, password: rotatedPassword}));
  await exclusive.close();
  cases.push('competing_opfs_worker_rejected_without_mutation');
  // SQL compare-and-set is checked even inside the exclusive Worker profile.
  const raw = await SQLiteVault.open({id, password: rotatedPassword});
  const receipt = await raw.receipt(), row = await raw.read(recordId);
  await raw.save(recordId, row.metadata, row.payload, {expectedRevision: row.revision, expectedVaultRevision: receipt.revision, requestId: random()});
  await rejects(() => raw.save(recordId, row.metadata, row.payload, {expectedRevision: row.revision, expectedVaultRevision: receipt.revision, requestId: random()}), 'conflict');
  await raw.close();
  cases.push('stale_complete_storage_revision_cas_rejected');
  const damagedId = '99'.repeat(16);
  const disposable = await SQLiteVault.create({id: damagedId, password});
  const disposableOwner = await SQLiteWorkspace.create({Workspace, vault: disposable, recordId, namespace});
  const disposablePlan = disposableOwner.prepareReview(input(), random());
  await disposableOwner.commit(disposablePlan, {approved: true});
  await disposableOwner.close();
  const directory = await (await navigator.storage.getDirectory()).getDirectoryHandle(`.datafog-sqlite-${damagedId}`);
  const files = [];
  async function visit(dir) {
    for await (const handle of dir.values()) {
      if (handle.kind === 'directory') await visit(handle);
      else { const file = await handle.getFile(); files.push({handle, bytes: new Uint8Array(await file.arrayBuffer())}); }
    }
  }
  await visit(directory);
  check(files.length > 0, 'no OPFS evidence');
  for (const file of files) check(!new TextDecoder().decode(file.bytes).includes('fault@example.test'), 'plaintext persisted');
  const largest = files.sort((a,b) => b.bytes.length - a.bytes.length)[0];
  check(largest.bytes.length > 8192, 'encrypted database evidence absent');
  largest.bytes[largest.bytes.length - 100] ^= 1;
  const writable = await largest.handle.createWritable(); await writable.write(largest.bytes); await writable.close();
  await rejects(async () => {
    const corrupted = await SQLiteVault.open({id: damagedId, password});
    try { await SQLiteWorkspace.open({Workspace, vault: corrupted, recordId, expectedNamespace: namespace}); }
    finally { if (!corrupted.closed) await corrupted.close(); }
  });
  cases.push('persistent_artifacts_contain_no_fixture_plaintext_and_page_tamper_rejects');
  return cases;
};
globalThis.runRestart = async () => {
  const vault = await SQLiteVault.open({id, password: rotatedPassword});
  const owner = await SQLiteWorkspace.open({Workspace, vault, recordId, expectedNamespace: namespace});
  check(owner.restore(localStorage.getItem('protected')) === 'Email alice@example.test', 'process restart lost state');
  await owner.close();
  const forgottenVault = await SQLiteVault.open({id: 'ee'.repeat(16), password});
  const forgotten = await SQLiteWorkspace.open({Workspace, vault: forgottenVault, recordId, expectedNamespace: namespace});
  check(forgotten.restoreReport(localStorage.getItem('retired-marker')).issues[0].kind === 'expired', 'process restart revived retirement');
  await forgotten.close();
  const route = JSON.parse(localStorage.getItem('conversation-pending'));
  const conversationId = Uint8Array.from(route.conversationId);
  const pendingVault = await SQLiteVault.open({id: 'ff'.repeat(16), password});
  const pending = await SQLiteWorkspace.open({Workspace, vault: pendingVault, recordId, expectedNamespace: namespace});
  check(pending.sendIntentStatus(route.intent) === 'pending', 'process restart automatically observed send');
  await rejects(() => pending.restoreInConversation(route.protectedText, conversationId));
  const observed = pending.prepareObservedSend(route.intent, conversationId, route.surface, 'synthetic-sqlite-confirmed-chat', {id: random(), occurredAtMs: 2});
  check(pending.sendIntentStatus(route.intent) === 'pending', 'preparation observed send before save');
  await rejects(() => pending.commit(observed, {approved: false}), 'approval_required');
  check(pending.sendIntentStatus(route.intent) === 'pending', 'unapproved observation changed send status');
  await pending.commit(observed, {approved: true});
  check(pending.sendIntentStatus(route.intent) === 'observed_sent', 'approved persisted observation did not activate');
  check(pending.restoreInConversation(route.protectedText, conversationId) === route.text, 'saved observation scoped restore mismatch');
  await pending.close();
  return ['browser_process_restart_preserves_exact_restore', 'browser_process_restart_preserves_expired_marker',
    'conversation_pending_survives_process_restart_until_approved_observation_save'];
};
globalThis.runObservedRestart = async () => {
  const route = JSON.parse(localStorage.getItem('conversation-pending'));
  const vault = await SQLiteVault.open({id: 'ff'.repeat(16), password});
  const owner = await SQLiteWorkspace.open({Workspace, vault, recordId, expectedNamespace: namespace});
  check(owner.sendIntentStatus(route.intent) === 'observed_sent', 'process restart lost saved observation');
  check(owner.restoreInConversation(route.protectedText, Uint8Array.from(route.conversationId)) === route.text, 'observed scoped restore changed after restart');
  await owner.close();
  return ['conversation_saved_observation_and_scoped_restore_survive_process_restart'];
};
globalThis.ready = true;
