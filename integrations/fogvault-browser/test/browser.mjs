import {init, Workspace, scan} from './node_modules/@datafog/wasm/index.js';
import {FogVault, randomId, clearRecord} from './node_modules/@datafog/fogvault-browser/dist/client.mjs';
import {FogVaultWorkspace} from './node_modules/@datafog/core-fogvault-browser/index.mjs';

const options = {limits: {payload: 16 * 1024 * 1024, metadata: 88, records: 64},
  storageProfile: 'arraybuffer', kdfProfiles: [{memory_kib: 65536, passes: 3}]};
const password = 'SYNTHETIC Core–FogVault integration password';
const cases = [], measurements = [], names = [], owners = new Set();
const same = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
function require(value, message) { if (!value) throw Error(message); }
async function rejected(run, code) {
  try { await run(); } catch (error) { require(!code || error.code === code, `expected ${code}; received ${error.code}`); return error; }
  throw Error('expected rejection');
}
function findings(text) { return scan(text).map(f => ({entityType: f.entityType, matchedText: f.matchedText,
  range: {unit: 'utf8_bytes', ...f.byteRange}, detectorName: f.detectorName})); }
function review(owner, text = '🦀 Email alice@example.test') {
  return owner.prepareReview([{text, scannerFindings: findings(text), userSelections: []}], randomId());
}
async function make() {
  const vault = new FogVault(options); owners.add(vault);
  const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name);
  await vault.create(name, password);
  const recordId = randomId(), namespace = randomId();
  const owner = await FogVaultWorkspace.create({Workspace, vault, recordId, namespace});
  return {vault, owner, name, recordId, namespace};
}
function receiptBarrier(vault) {
  let entered, release, first = true;
  const arrived = new Promise(resolve => { entered = resolve; });
  const gate = new Promise(resolve => { release = resolve; });
  const proxy = new Proxy(vault, {get(target, property) {
    if (property === 'receipt') return async () => {
      if (first) { first = false; entered(); await gate; }
      return target.receipt();
    };
    const value = Reflect.get(target, property, target);
    return typeof value === 'function' ? value.bind(target) : value;
  }});
  return {vault: proxy, arrived, release: () => release()};
}
async function record(id, run) {
  const started = performance.now();
  try { await run(); cases.push({id, status: 'pass', executed: true, duration_ms: performance.now() - started}); }
  catch (error) { cases.push({id, status: 'fail', executed: true, code: error.code || error.name, message: error.message}); throw error; }
}

window.coreFogVaultDone = (async () => {
  let failure;
  try {
    await init();
    await record('create_approve_save_activate_lock_reopen_restore', async () => {
      const f = await make(), text = '🦀 Email alice@example.test', plan = review(f.owner, text);
      const preview = plan.preview().text;
      require(!preview.includes('alice@example.test'), 'preview leaked source');
      const saved = await f.owner.commit(plan, {approved: true});
      require(saved.recordRevision === 2n && saved.previews[0].text === preview, 'wrong confirmed result');
      const stream = f.owner.bufferedRestore();
      const locking = f.owner.lock();
      await rejected(() => plan.preview()); await rejected(() => stream.feed(new Uint8Array()), 'owner_closed');
      await locking;
      const other = new FogVault(options); owners.add(other); await other.open(f.name); await other.unlock(password);
      const reopened = await FogVaultWorkspace.open({Workspace, vault: other, recordId: f.recordId, expectedNamespace: f.namespace, expectedHead: saved.head});
      require(reopened.restore(preview) === text, 'reopened restoration mismatch');
      require(reopened.recall('ALICE@example.test').findings.length === 1, 'reopened recall mismatch');
      const raw = await other.read(f.recordId);
      require(raw.metadata.length === 88 && same(raw.metadata.subarray(24, 40), f.recordId), 'binding metadata mismatch');
      clearRecord(raw); await reopened.close();
    });
    await record('literal_approval_and_foreign_plan_rejection_before_save', async () => {
      const f = await make(), second = await make(), plan = review(f.owner);
      const before = await f.vault.receipt();
      for (const approved of [false, 1, 'true', undefined]) await rejected(() => f.owner.commit(plan, {approved}), 'approval_required');
      await rejected(() => second.owner.commit(plan, {approved: true}), 'foreign_or_consumed_plan');
      require(same(before.head_digest, (await f.vault.receipt()).head_digest), 'rejected approval mutated storage');
      await f.owner.commit(plan, {approved: true}); await rejected(() => f.owner.commit(plan, {approved: true}), 'foreign_or_consumed_plan');
      await f.owner.close(); await second.owner.close();
    });
    await record('occupied_creation_rejects_exact_duplicate_request_without_overwrite', async () => {
      const vault = new FogVault(options); owners.add(vault);
      const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name); await vault.create(name, password);
      const parameters = {Workspace, vault, recordId: randomId(), namespace: randomId(), operationId: randomId(), requestId: randomId()};
      const owner = await FogVaultWorkspace.create(parameters), before = await vault.read(parameters.recordId), receiptBefore = await vault.receipt();
      await rejected(() => FogVaultWorkspace.create(parameters), 'occupied_destination');
      const after = await vault.read(parameters.recordId);
      require(before.revision === after.revision && same(before.payload, after.payload) && same(before.metadata, after.metadata), 'occupied creation changed record');
      require(same(receiptBefore.head_digest, (await vault.receipt()).head_digest), 'occupied replay changed vault head');
      clearRecord(before); clearRecord(after); await owner.close();
    });
    await record('complete_head_cas_conflict_closes_stale_owner', async () => {
      const f = await make(), competing = new FogVault(options); owners.add(competing);
      await competing.open(f.name); await competing.unlock(password);
      const other = await FogVaultWorkspace.open({Workspace, vault: competing, recordId: f.recordId});
      const stale = review(f.owner, 'Email first@example.test');
      const winning = await other.commit(review(other, 'Email second@example.test'), {approved: true});
      await rejected(() => f.owner.commit(stale, {approved: true}), 'Conflict');
      require(f.owner.closed, 'conflict retained Core authority');
      await rejected(() => f.owner.restore(winning.previews[0].text), 'owner_closed');
      await f.owner.lock(); await other.close();
    });
    await record('authenticated_record_and_namespace_substitution_rejected', async () => {
      const f = await make(), raw = await f.vault.read(f.recordId), otherId = randomId();
      await f.vault.save(otherId, raw.metadata, raw.payload, {expectedRevision: 0n});
      await rejected(() => FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: otherId}), 'binding_mismatch');
      await rejected(() => FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: f.recordId, expectedNamespace: randomId()}), 'binding_mismatch');
      clearRecord(raw); await f.owner.close();
    });
    await record('saved_core_commit_id_mismatch_rejected', async () => {
      const f = await make(), raw = await f.vault.read(f.recordId);
      raw.metadata[72] ^= 1;
      await f.vault.save(f.recordId, raw.metadata, raw.payload, {expectedRevision: raw.revision});
      await rejected(() => FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: f.recordId}), 'binding_mismatch');
      clearRecord(raw); await f.owner.close();
    });
    await record('async_open_expectations_and_creation_request_ids_are_snapshotted', async () => {
      const f = await make(), savedHead = f.owner.head();
      await f.owner.lock(); await f.vault.unlock(password);
      const wrongNamespace = randomId(), wrongHead = {revision: savedHead.revision, commitId: Array.from(randomId())};
      const barrier = receiptBarrier(f.vault);
      const pending = FogVaultWorkspace.open({Workspace, vault: barrier.vault, recordId: f.recordId,
        expectedNamespace: wrongNamespace, expectedHead: wrongHead});
      const failed = rejected(() => pending, 'binding_mismatch');
      await barrier.arrived; wrongNamespace.set(f.namespace); wrongHead.commitId.splice(0, 16, ...savedHead.commitId);
      barrier.release(); await failed;
      const namespace = f.namespace.slice(), expected = {revision: savedHead.revision, commitId: [...savedHead.commitId]};
      const positive = receiptBarrier(f.vault);
      const opening = FogVaultWorkspace.open({Workspace, vault: positive.vault, recordId: f.recordId,
        expectedNamespace: namespace, expectedHead: expected});
      await positive.arrived; namespace.fill(0); expected.revision = '18446744073709551615'; expected.commitId.fill(0);
      positive.release(); const opened = await opening;
      require(same(opened.namespace(), f.namespace), 'caller mutation changed trusted open expectations');
      await opened.close();

      const raw = new FogVault(options); owners.add(raw);
      const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name); await raw.create(name, password);
      const requestId = randomId(), expectedRequest = requestId.slice(), creation = receiptBarrier(raw);
      const creating = FogVaultWorkspace.create({Workspace, vault: creation.vault, recordId: randomId(), namespace: randomId(), requestId});
      await creation.arrived; requestId.fill(0); creation.release(); const created = await creating;
      require(same((await raw.receipt()).request_id, expectedRequest), 'caller mutation changed creation request identity');
      await created.close();
    });
    await record('lost_save_reply_reconciles_without_handoff_or_old_approval', async () => {
      const NativeWorker = globalThis.Worker;
      let f, intercept = false;
      globalThis.Worker = class ReplyLossWorker extends NativeWorker {
        constructor(...args) {
          super(...args);
          this.addEventListener('message', event => {
            if (intercept && event.data?.ok && event.data?.result?.saved) {
              event.stopImmediatePropagation(); void f.owner.lock();
            }
          });
        }
      };
      try {
        f = await make(); const plan = review(f.owner), requestId = randomId();
        intercept = true;
        const failure = await rejected(() => f.owner.commit(plan, {approved: true, requestId}), 'outcome_unknown');
        require(same(failure.requestId, requestId) && f.owner.closed, 'uncertain outcome lost request/closed state');
        await rejected(() => plan.preview(), 'owner_closed');
        intercept = false; await f.vault.unlock(password);
        const reconciled = await f.vault.reconcileLatest(requestId);
        require(same(reconciled.receipt.request_id, requestId), 'wrong reconciliation request');
        const reopened = await FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: f.recordId});
        require(reopened.recall('alice@example.test').findings.length === 1, 'committed target not reopened');
        await rejected(() => reopened.commit(plan, {approved: true}), 'foreign_or_consumed_plan');
        const fresh = review(reopened, 'Email alice@example.test');
        await reopened.commit(fresh, {approved: true}); await reopened.close();
      } finally { intercept = false; globalThis.Worker = NativeWorker; await f?.vault.close(); }
    });
    await record('late_confirmed_save_after_lock_never_activates', async () => {
      const raw = new FogVault(options); owners.add(raw);
      const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name); await raw.create(name, password);
      let intercept = false, submitted, release;
      const arrived = new Promise(resolve => { submitted = resolve; });
      const gate = new Promise(resolve => { release = resolve; });
      const transport = new Proxy(raw, {get(target, property) {
        if (property === 'save') return async (...args) => {
          const saved = await target.save(...args);
          if (intercept) { submitted(); await gate; }
          return saved;
        };
        const value = Reflect.get(target, property, target);
        return typeof value === 'function' ? value.bind(target) : value;
      }});
      const recordId = randomId(), owner = await FogVaultWorkspace.create({Workspace, vault: transport, recordId, namespace: randomId()});
      const plan = review(owner); intercept = true;
      const pending = owner.commit(plan, {approved: true});
      const failed = rejected(() => pending, 'owner_closed');
      await arrived; await owner.lock(); release(); await failed;
      require(owner.closed, 'late confirmation reopened owner');
      intercept = false; await raw.unlock(password);
      const reopened = await FogVaultWorkspace.open({Workspace, vault: raw, recordId});
      require(reopened.recall('alice@example.test').findings.length === 1, 'persisted late target missing');
      await reopened.close();
    });
    await record('pending_conversation_survives_reopen_and_requires_observed_send', async () => {
      const f = await make(), text = 'Email alice@example.test', logicalId = randomId(), surface = 'synthetic-no-network';
      const plan = f.owner.prepareConversationSend([{input: {text, scannerFindings: findings(text), userSelections: []},
        conversation: {id: logicalId, surface, siteId: null, siteIdByteBudget: 64}}], {id: randomId(), occurredAtMs: 1});
      const intents = plan.sendIntents(), saved = await f.owner.commit(plan, {approved: true});
      require(intents.length === 1, 'missing send intent');
      await rejected(() => f.owner.restoreInConversation(saved.previews[0].text, logicalId));
      await f.owner.lock(); await f.vault.unlock(password);
      const reopened = await FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: f.recordId});
      require(reopened.sendIntentStatus(intents[0]) === 'pending', 'uncertain send changed during reopen');
      await rejected(() => reopened.restoreInConversation(saved.previews[0].text, logicalId));
      const observed = reopened.prepareObservedSend(intents[0], logicalId, surface, 'synthetic-confirmed-chat', {id: randomId(), occurredAtMs: 2});
      await reopened.commit(observed, {approved: true});
      require(reopened.restoreInConversation(saved.previews[0].text, logicalId) === text, 'confirmed scoped restore mismatch');
      await reopened.close();
    });
    await record('authenticated_bundle_recovery_preserves_aliases_and_refuses_occupied_destination', async () => {
      const f = await make(), text = 'Email alice@example.test';
      const saved = await f.owner.commit(review(f.owner, text), {approved: true});
      const source = await f.vault.read(f.recordId);
      const target = new FogVault(options); owners.add(target);
      const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name); await target.create(name, password);
      const recordId = randomId(), parameters = {Workspace, vault: target, recordId, bundle: source.payload,
        expectedNamespace: f.namespace, expectedHead: saved.head, operationId: randomId(), requestId: randomId(), approved: true};
      const before = await target.receipt();
      await rejected(() => FogVaultWorkspace.importAuthenticatedBundle({...parameters, approved: false}), 'approval_required');
      await rejected(() => FogVaultWorkspace.importAuthenticatedBundle({...parameters, expectedNamespace: randomId()}), 'binding_mismatch');
      require(same(before.head_digest, (await target.receipt()).head_digest), 'rejected source mutated destination');
      const imported = await FogVaultWorkspace.importAuthenticatedBundle(parameters);
      require(imported.restore(saved.previews[0].text) === text, 'recovery lost alias');
      require(imported.head().revision !== saved.head.revision, 'recovery did not create a fresh Core head');
      const installed = await target.read(recordId);
      const view = new DataView(installed.metadata.buffer, installed.metadata.byteOffset, installed.metadata.byteLength);
      require(view.getBigUint64(40, true).toString() === saved.head.revision && view.getBigUint64(64, true).toString() === imported.head().revision, 'recovery binding lost source/candidate heads');
      clearRecord(installed);
      await rejected(() => FogVaultWorkspace.importAuthenticatedBundle(parameters), 'occupied_destination');
      require(imported.restore(saved.previews[0].text) === text, 'occupied import damaged existing destination');
      clearRecord(source); await imported.close(); await f.owner.close();
    });
    await record('authenticated_import_rejects_unrelated_occupied_vault_without_mutation', async () => {
      const source = await make(), destination = await make(), recordId = randomId();
      const sourceRecord = await source.vault.read(source.recordId), before = await destination.vault.read(destination.recordId);
      const prior = await destination.vault.receipt();
      await rejected(() => FogVaultWorkspace.importAuthenticatedBundle({Workspace, vault: destination.vault, recordId,
        bundle: sourceRecord.payload, expectedNamespace: source.namespace, expectedHead: source.owner.head(), approved: true}), 'occupied_destination');
      const after = await destination.vault.read(destination.recordId);
      require(before.revision === after.revision && same(before.metadata, after.metadata) && same(before.payload, after.payload), 'recovery changed unrelated destination record');
      require(same(prior.head_digest, (await destination.vault.receipt()).head_digest), 'recovery advanced occupied vault');
      await rejected(() => destination.vault.read(recordId), 'NotFound');
      clearRecord(sourceRecord); clearRecord(before); clearRecord(after); await source.owner.close(); await destination.owner.close();
    });
    await record('whole_bundle_growth_samples_and_payload_admission', async () => {
      for (const count of [1, 100, 1000]) {
        const f = await make();
        const values = Array.from({length: count}, (_, index) => `synthetic${index}@example.test`);
        const text = values.join(' '), items = []; let offset = 0;
        for (const value of values) {
          items.push({entityType: 'EMAIL', matchedText: value, range: {unit: 'utf8_bytes', start: offset, end: offset + value.length}});
          offset += value.length + 1;
        }
        let started = performance.now();
        const plan = f.owner.prepareReview([{text, scannerFindings: items, userSelections: []}], randomId());
        const prepareMs = performance.now() - started;
        started = performance.now(); const saved = await f.owner.commit(plan, {approved: true}); const saveMs = performance.now() - started;
        const record = await f.vault.read(f.recordId), bundleBytes = record.payload.length; clearRecord(record);
        await f.owner.lock(); started = performance.now(); await f.vault.unlock(password); const unlockMs = performance.now() - started;
        started = performance.now(); const reopened = await FogVaultWorkspace.open({Workspace, vault: f.vault, recordId: f.recordId}); const reopenMs = performance.now() - started;
        started = performance.now(); require(reopened.restore(saved.previews[0].text) === text, 'growth restore mismatch'); const restoreMs = performance.now() - started;
        measurements.push({mappings: count, source_bytes: new TextEncoder().encode(text).length, bundle_bytes: bundleBytes,
          prepare_ms: prepareMs, confirmed_save_activate_ms: saveMs, password_unlock_ms: unlockMs,
          authenticated_open_ms: reopenMs, restore_ms: restoreMs,
          advisory_js_heap_bytes: performance.memory?.usedJSHeapSize ?? null});
        await reopened.close();
      }
      // Admission rejection is actual SDK behavior; it must never become a successful handoff.
      const limited = new FogVault({...options, limits: {...options.limits, payload: 1}}); owners.add(limited);
      const name = 'datafog-SYNTHETIC-joint-' + crypto.randomUUID(); names.push(name); await limited.create(name, password);
      await rejected(() => FogVaultWorkspace.create({Workspace, vault: limited, recordId: randomId(), namespace: randomId()}), 'InvalidInput');
      require((await limited.receipt()).generation === 0n, 'admission rejection mutated vault'); await limited.close();
    });
  } catch (error) { failure = {code: error.code || error.name, message: error.message}; }
  finally {
    for (const owner of owners) await owner.close();
    for (const name of names) await new Promise((resolve, reject) => {
      const request = indexedDB.deleteDatabase(name); request.onsuccess = resolve; request.onerror = () => reject(request.error);
    });
  }
  return {schema: 'datafog-fogvault-browser-cases/v1', pass: !failure, executed: true, userAgent: navigator.userAgent, cases, measurements,
    measurement_scope: 'Single synthetic run; JS heap excludes Worker/WASM/allocator/foreign copies and is not a peak-memory or device qualification.', failure};
})();
