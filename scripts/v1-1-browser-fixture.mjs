import {init, Workspace} from './node_modules/@datafog/wasm/index.js';
import {BrowserWorkspace} from './node_modules/@datafog/wasm/cnn/workspace.mjs';
import {SQLiteVault, SQLiteWorkspace} from './node_modules/@datafog/core-sqlite-browser/index.mjs';
import {FogVault, clearRecord} from './node_modules/@datafog/fogvault-browser/dist/client.mjs';
import {FogVaultWorkspace} from './node_modules/@datafog/core-fogvault-browser/index.mjs';

await init();
const text = 'BILL TO\nJohn Smith\n123 Main Street\nSpringfield IL 62704\nUSA\nEmail john@example.test';
const password = 'synthetic v1.1 browser qualification password';
const scan = {entities: ['PERSON', 'ADDRESS', 'EMAIL']};
const random = () => crypto.getRandomValues(new Uint8Array(16));
const hex = value => Array.from(value, n => n.toString(16).padStart(2, '0')).join('');
const check = (condition, message) => {if (!condition) throw Error(message);};
const reject = async (action, code) => {
  try {await action();} catch (error) {if (code) check(error.code === code, `expected ${code}, got ${error.code}`); return;}
  throw Error('operation unexpectedly succeeded');
};
const fogOptions = {limits: {payload: 16 * 1024 * 1024, metadata: 88, records: 64}, storageProfile: 'arraybuffer', kdfProfiles: [{memory_kib: 65536, passes: 3}]};
async function backend(kind, saved) {
  const route = saved || {id: hex(random()), recordId: [...random()], namespace: [...random()]};
  const recordId = new Uint8Array(route.recordId), namespace = new Uint8Array(route.namespace);
  let vault, owner;
  if (kind === 'sqlite') {
    vault = await SQLiteVault[saved ? 'open' : 'create']({id: route.id, password});
    owner = await SQLiteWorkspace[saved ? 'open' : 'create']({Workspace, vault, recordId, ...(saved ? {expectedNamespace: namespace} : {namespace})});
  } else {
    vault = new FogVault(fogOptions);
    if (saved) {await vault.open(route.id); await vault.unlock(password);} else await vault.create(route.id, password);
    owner = await FogVaultWorkspace[saved ? 'open' : 'create']({Workspace, vault, recordId, ...(saved ? {expectedNamespace: namespace} : {namespace})});
  }
  return {owner, route};
}

globalThis.runJoint = async () => {
  const cases = [], measurements = [];
  for (const kind of ['sqlite', 'fogvault']) {
    const f = await backend(kind);
    let host;
    const start = performance.now();
    try {
      host = await BrowserWorkspace.open({workspace: f.owner});
      check(host.cnnEnabled && host.detectorStatus === 'ready', 'CNN must load by default');
      const pending = await host.prepare(text, random(), {scan});
      await reject(() => host.commit(pending, {approved: false}), 'approval_required');
      await host.setCnnEnabled(false);
      await reject(() => host.commit(pending, {approved: true}), 'stale_review');
      const disabledReview = await host.prepare(text, random(), {scan});
      const disabledText = disabledReview.preview().text;
      check(!disabledText.includes('john@example.test'), 'disabled CNN removed rules');
      check(disabledText.includes('John Smith') && disabledText.includes('123 Main Street'), 'disabled CNN inferred');
      await host.setCnnEnabled(true);
      const review = await host.prepare(text, random(), {scan});
      const result = await host.commit(review, {approved: true});
      const protectedText = result.previews[0].text;
      check(!protectedText.includes('John Smith') && !protectedText.includes('123 Main Street'), 'CNN findings did not reach Workspace');
      check(host.restore(protectedText) === text, 'exact restoration failed');
      await host.setCnnEnabled(false);
      check(host.restore(protectedText) === text, 'disable destroyed committed mappings');
      localStorage.setItem(`joint-${kind}`, JSON.stringify({...f.route, protectedText, savedHead: result.head}));
      cases.push(`${kind}_default_cnn_review_toggle_save_restore`);
      measurements.push({backend: kind, loadAndFlowMs: performance.now() - start});
      await host.setCnnEnabled(true);
      const pendingScan = host.prepare(text.repeat(20), random(), {scan});
      const closing = host.lock();
      check(host.closed, 'lock did not revoke synchronously');
      await reject(() => pendingScan);
      await closing;
      cases.push(`${kind}_lock_during_inference_rejects_late_review`);
    } finally {if (host) await host.close(); else await f.owner.close();}
  }
  // Authenticate the source through FogVault, then recover into a fresh encrypted
  // destination. No source key wrappers or backend receipts are transplanted.
  const source = JSON.parse(localStorage.getItem('joint-fogvault'));
  const sourceVault = new FogVault(fogOptions);
  let bundle;
  try {
    await sourceVault.open(source.id); await sourceVault.unlock(password);
    const record = await sourceVault.read(new Uint8Array(source.recordId));
    try {bundle = record.payload.slice();} finally {clearRecord(record);}
  } finally {await sourceVault.close();}
  const destination = await SQLiteVault.create({id: hex(random()), password});
  let migrated;
  try {
    migrated = await SQLiteWorkspace.importAuthenticatedBundle({Workspace, vault: destination,
      recordId: random(), bundle, expectedNamespace: new Uint8Array(source.namespace),
      expectedHead: source.savedHead, approved: true, operationId: random()});
    check(migrated.restore(source.protectedText) === text, 'FogVault to SQLite recovery changed originals');
    check(migrated.head().commitId.join(',') !== source.savedHead.commitId.join(','), 'migration reused source head');
    cases.push('fogvault_to_sqlite_approved_fresh_recovery');
  } finally {bundle.fill(0); if (migrated) await migrated.close();}
  for (const kind of ['sqlite', 'fogvault']) {
    const f = await backend(kind), host = await BrowserWorkspace.open({workspace: f.owner});
    const logicalId = random(), operation = () => ({id: random(), occurredAtMs: Date.now()});
    try {
      const review = await host.prepareConversationSend([{input: {text, userSelections: []},
        conversation: {id: logicalId, surface: 'qualification', siteId: null, siteIdByteBudget: 64}}], operation(), {scan});
      const intent = review.sendIntents()[0];
      check(intent, 'conversation review omitted its send intent');
      const saved = await host.commit(review, {approved: true});
      check(host.sendIntentStatus(intent) === 'pending', 'save automatically observed a send');
      await reject(() => host.restoreInConversation(saved.previews[0].text, logicalId));
      const observation = host.prepareObservedSend(intent, logicalId, 'qualification', 'observed-chat', operation());
      check(host.sendIntentStatus(intent) === 'pending', 'preparation activated observation');
      await host.commit(observation, {approved: true});
      check(host.restoreInConversation(saved.previews[0].text, logicalId) === text, 'scoped conversation restoration failed');
      cases.push(`${kind}_cnn_conversation_pending_observed_scoped_restore`);
    } finally {await host.close();}
  }
  return {cases, measurements};
};
globalThis.runJointRestart = async () => {
  const cases = [];
  for (const kind of ['sqlite', 'fogvault']) {
    const route = JSON.parse(localStorage.getItem(`joint-${kind}`));
    const f = await backend(kind, route);
    const host = await BrowserWorkspace.open({workspace: f.owner, cnnEnabled: false});
    try {
      check(host.detectorStatus === 'disabled', 'disabled startup initialized CNN');
      check(host.restore(route.protectedText) === text, 'browser restart failed exact restoration');
      cases.push(`${kind}_process_restart_restore_with_cnn_disabled`);
    } finally {await host.close();}
  }
  return cases;
};
globalThis.jointReady = true;
