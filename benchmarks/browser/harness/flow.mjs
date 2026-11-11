import {init, Workspace} from './node_modules/@datafog/wasm/index.js';
import {CnnSession} from './node_modules/@datafog/wasm/cnn/index.js';
import {SQLiteVault, SQLiteWorkspace} from './node_modules/@datafog/core-sqlite-browser/index.mjs';
import {FogVault} from './node_modules/@datafog/fogvault-browser/dist/client.mjs';
import {FogVaultWorkspace} from './node_modules/@datafog/core-fogvault-browser/index.mjs';

const PASSWORD = 'Synthetic benchmark password only / 2026';
const FOGVAULT_OPTIONS = {
  limits: {payload: 16 * 1024 * 1024, metadata: 88, records: 64},
  storageProfile: 'arraybuffer', kdfProfiles: [{memory_kib: 65536, passes: 3}],
};
const encoder = new TextEncoder();
const now = () => performance.now();
const id16 = () => crypto.getRandomValues(new Uint8Array(16));
const hex = bytes => Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
const toPlain = value => JSON.parse(JSON.stringify(value, (_key, item) => typeof item === 'bigint' ? item.toString() : item));
const intervals = {};
const gates = {};
let bundleScope = 'initial';
const bundleSizes = [];
const observedMarkers = new Set();
let capturedCandidateBundle;
let cnnSession;
const requiredGateNames = [
  'exact_restore_after_browser_restart', 'no_handoff_before_confirmed_save_and_activation',
  'wrong_credentials_rejected', 'conflicting_save_rejected_without_overwrite',
  'lost_save_reply_recovers_without_duplicate_activation', 'lock_during_save_prevents_handoff',
  'stale_review_rejected', 'conversation_scope_enforced', 'backup_recovery_preserves_mappings_and_authority',
];
for (const name of requiredGateNames) gates[name] = {status: 'unsupported', detail: 'not exercised by the per-trial flow'};

function timed(name, startEvent, endEvent, run) {
  const started = now();
  return Promise.resolve().then(run).then(value => {
    intervals[name] = {durationMs: now() - started, clock: 'performance.now', startEvent, endEvent, status: 'measured'};
    return value;
  }, error => {
    intervals[name] = {durationMs: now() - started, clock: 'performance.now', startEvent, endEvent, status: 'failed', failureCode: error?.code || error?.name || 'error'};
    throw error;
  });
}

function observingWorkspace() {
  const wrapped = new WeakMap();
  const original = new WeakMap();
  const measureBytes = (kind, bytes) => {
    if (bytes instanceof Uint8Array) {
      bundleSizes.push({kind, phase: bundleScope, bytes: bytes.byteLength});
      if (kind === 'candidate_bundle' && bundleScope === 'trial') {
        capturedCandidateBundle?.fill(0);
        capturedCandidateBundle = bytes.slice();
      }
    }
    return bytes;
  };
  function observePlan(plan) {
    if (!plan || typeof plan !== 'object') return plan;
    if (wrapped.has(plan)) return wrapped.get(plan);
    const proxy = new Proxy(plan, {get(target, property) {
      const value = Reflect.get(target, property, target);
      if (property === 'exportCandidateBundle' && typeof value === 'function') return (...args) => measureBytes('candidate_bundle', value.apply(target, args));
      return typeof value === 'function' ? value.bind(target) : value;
    }});
    wrapped.set(plan, proxy); original.set(proxy, plan);
    return proxy;
  }
  function observeOwner(owner) {
    if (!owner || typeof owner !== 'object') return owner;
    return new Proxy(owner, {get(target, property) {
      const value = Reflect.get(target, property, target);
      if (property === 'exportBundle' && typeof value === 'function') return (...args) => measureBytes('workspace_bundle', value.apply(target, args));
      if (['prepareReview', 'prepareConversationSend', 'prepareObservedSend', 'prepareOutcome', 'prepare', 'prepareBatch'].includes(property) && typeof value === 'function') return (...args) => observePlan(value.apply(target, args));
      if (property === 'activateSaved' && typeof value === 'function') return (plan, ...args) => value.call(target, original.get(plan) || plan, ...args);
      return typeof value === 'function' ? value.bind(target) : value;
    }});
  }
  return {
    create: (...args) => observeOwner(Workspace.create(...args)),
    openBundle: (...args) => observeOwner(Workspace.openBundle(...args)),
    fromBundle: (...args) => observeOwner(Workspace.fromBundle(...args)),
  };
}

function inputForCore(input, mode) {
  return {
    text: input.text,
    scannerFindings: mode === 'storage_workspace' ? input.findings.map(finding => ({
      entityType: finding.entityType,
      matchedText: finding.matchedText,
      range: {unit: 'utf8_bytes', start: finding.startByte, end: finding.endByte},
      detectorName: 'synthetic-benchmark-fixture', source: 'scanner', needsReview: true,
    })) : [],
    // In complete-extension mode the fixture's known synthetic values enter as
    // explicit review selections, while the packaged CNN runs independently and
    // contributes its own scanner findings to the same prepared review.
    userSelections: mode === 'complete_extension' ? input.findings.map(finding => ({
      entityType: finding.entityType, matchedText: finding.matchedText,
      range: {unit: 'utf8_bytes', start: finding.startByte, end: finding.endByte},
      detectorName: 'synthetic-benchmark-selection', source: 'user_selection', needsReview: true,
    })) : [],
  };
}

async function loadInputs(fixture, mode) {
  if (mode === 'storage_workspace') return {seedInputs: fixture.seedInputs.map(input => inputForCore(input, mode)), inputs: fixture.inputs.map(input => inputForCore(input, mode))};
  const started = now();
  cnnSession = await CnnSession.open({cnnEnabled: true});
  const inferenceSamples = [];
  const scanInputs = async source => {
    const output = [];
    for (const input of source) {
      const inferenceStarted = now();
      const result = await cnnSession.scan(input.text);
      inferenceSamples.push(now() - inferenceStarted);
      const selected = inputForCore(input, mode).userSelections;
      output.push({text: input.text, scannerFindings: result.findings.map(finding => ({
      entityType: finding.entityType,
      matchedText: finding.matchedText,
      range: {unit: 'utf8_bytes', ...finding.byteRange},
      detectorName: finding.detectorName,
      detectorVersion: finding.detectorVersion,
      source: 'scanner', needsReview: true,
      ...(finding.confidence == null ? {} : {confidence: finding.confidence}),
      })), userSelections: selected});
    }
    return output;
  };
  const seedInputs = await scanInputs(fixture.seedInputs);
  const inputs = await scanInputs(fixture.inputs);
  intervals.modelStartup = {durationMs: inferenceSamples.length ? now() - started - inferenceSamples.reduce((sum, value) => sum + value, 0) : now() - started,
    clock: 'performance.now', startEvent: 'CnnSession.open called', endEvent: 'CNN worker reports ready', status: 'measured'};
  intervals.modelInference = {durationMs: inferenceSamples.reduce((sum, value) => sum + value, 0), perInputMs: inferenceSamples,
    clock: 'performance.now', startEvent: 'first detector.scan called', endEvent: 'last detector.scan resolved', status: 'measured', process: 'dedicated Worker'};
  return {seedInputs, inputs};
}

async function createBackend(backend, route, create) {
  const ObservedWorkspace = observingWorkspace();
  if (backend === 'sqlite') {
    const vault = await (create ? SQLiteVault.create({id: route.id, password: PASSWORD}) : SQLiteVault.open({id: route.id, password: PASSWORD}));
    let owner;
    try {
      owner = await (create
        ? SQLiteWorkspace.create({Workspace: ObservedWorkspace, vault, recordId: route.recordId, namespace: route.namespace})
        : SQLiteWorkspace.open({Workspace: ObservedWorkspace, vault, recordId: route.recordId, expectedNamespace: route.namespace}));
      return {vault, owner};
    } catch (error) { await vault.close(); throw error; }
  }
  const vault = new FogVault(FOGVAULT_OPTIONS);
  if (create) await vault.create(route.id, PASSWORD);
  else { await vault.open(route.id); await vault.unlock(PASSWORD); }
  try {
    const owner = await (create
      ? FogVaultWorkspace.create({Workspace: ObservedWorkspace, vault, recordId: route.recordId, namespace: route.namespace})
      : FogVaultWorkspace.open({Workspace: ObservedWorkspace, vault, recordId: route.recordId, expectedNamespace: route.namespace}));
    return {vault, owner};
  } catch (error) { await vault.close(); throw error; }
}

async function closeBackend(opened) { if (opened?.owner && !opened.owner.closed) await opened.owner.close(); else if (opened?.vault && !opened.vault.closed) await opened.vault.close(); }

async function savePlan(owner, plan, label) {
  bundleScope = label;
  const before = plan.heads();
  const saved = await owner.commit(plan, {approved: true});
  if (!saved || !saved.previews || !saved.head) throw new Error('confirmed commit did not return an activated result');
  return {saved, before};
}

function addMarkers(previews) {
  for (const preview of previews || []) {
    const matches = String(preview.text).matchAll(/\[\[([A-Z][A-Z0-9]{0,31}_[0-9A-HJKMNP-TV-Z]{6})\]\]/g);
    for (const match of matches) observedMarkers.add(match[1]);
  }
}

function findingInput(input) { return {text: input.text, scannerFindings: input.scannerFindings, userSelections: input.userSelections || []}; }

async function prepare(owner, inputs, fixture, mode, opId = id16()) {
  if (mode === 'storage-workspace') {
    return owner.prepareReview(inputs, opId);
  }
  // The packaged CNN and scanner are loaded in Worker before this path; the exact
  // detected spans stay attached to the review that Core validates and persists.
  return owner.prepareReview(inputs, opId);
}

async function prepareAndCommit(owner, inputs, fixture, mode, intervalPrefix, label) {
  const plan = await timed(`${intervalPrefix}Preparation`, 'prepareReview called', 'prepared review returned',
    () => prepare(owner, inputs, fixture, mode));
  const {saved} = await timed(`${intervalPrefix}ApprovalToSavedActivation`, 'explicit approved:true passed to commit', 'encrypted commit confirmed, Core activated and previews available',
    () => savePlan(owner, plan, label));
  return saved;
}

async function populateHistory(owner, fixture) {
  const history = fixture.expected?.history || {};
  const recipe = history.recipe;
  if (!recipe) return {status: 'complete', operationOutcomes: 0, conversations: 0, messagesPerConversation: 0, simulatedSends: 0};
  let ordinal = 0;
  bundleScope = 'history';
  const total = recipe.operationOutcomes?.count || 0;
  for (let index = 0; index < total; index++) {
    const plan = owner.prepareOutcome({kind: recipe.operationOutcomes.kind, issueCount: recipe.operationOutcomes.issueCount},
      {id: id16(), occurredAtMs: recipe.timestamps.baseMs + ordinal++ + recipe.timestamps.stepMs});
    await owner.commit(plan, {approved: true});
  }
  const conversationRecipe = recipe.conversationRecipe || {};
  const conversations = conversationRecipe.conversationCount ?? recipe.conversations ?? 0;
  const messagesPerConversation = conversationRecipe.messagesPerConversation ?? recipe.messagesPerConversation ?? 0;
  const textPattern = conversationRecipe.messageText || 'Synthetic history message <conversation-index>-<message-index>; no findings.';
  let messageOrdinal = 0;
  let actualMessages = 0;
  let contexts = 0;
  const observedContexts = [];
  for (let conversation = 0; conversation < conversations; conversation++) {
    const logicalId = id16();
    const surface = conversationRecipe.surface || 'synthetic-browser-benchmark';
    const siteId = `synthetic-conversation-${String(conversation).padStart(4, '0')}`;
    let boundSiteId = null;
    contexts++;
    for (let message = 0; message < messagesPerConversation; message++) {
      const sourceText = textPattern.replace('<conversation-index>', String(conversation)).replace('<message-index>', String(message));
      const eventTime = recipe.timestamps.baseMs + recipe.operationOutcomes.count + 1 + 2 * messageOrdinal;
      const plan = owner.prepareConversationSend([{input: {text: sourceText, scannerFindings: [], userSelections: []},
        conversation: {id: logicalId, surface, siteId: boundSiteId, siteIdByteBudget: conversationRecipe.siteIdByteBudget ?? 64}}], {id: id16(), occurredAtMs: eventTime});
      const intent = plan.sendIntents()[0];
      await owner.commit(plan, {approved: true});
      if (owner.sendIntentStatus(intent) !== 'pending') throw new Error('history send was not pending after save');
      const observation = owner.prepareObservedSend(intent, logicalId, surface, siteId,
        {id: id16(), occurredAtMs: eventTime + recipe.timestamps.stepMs});
      await owner.commit(observation, {approved: true});
      if (owner.sendIntentStatus(intent) !== 'observed_sent') throw new Error('history observed-send did not activate after save');
      boundSiteId = siteId;
      actualMessages++;
      messageOrdinal++;
    }
    const state = owner.conversation(logicalId);
    if (!state || state.pendingIntents !== 0) throw new Error('history conversation context has pending intents');
    observedContexts.push({conversationIndex: conversation, pendingIntents: state.pendingIntents, siteId: state.conversation?.siteId || state.siteId});
  }
  return {status: 'complete', operationOutcomes: total, conversations, messagesPerConversation,
    simulatedSends: actualMessages, conversationContexts: contexts, operationIdCount: ordinal + messageOrdinal * 2, observedContexts,
    expectedBeforeTrial: recipe.expectedBeforeTrial || null};
}

async function storageSize() {
  try { return await navigator.storage.estimate(); }
  catch (error) { return {unsupported: true, reason: error?.name || 'storage estimate unavailable'}; }
}

function checkStaleCoreReview() {
  const workspace = Workspace.create(id16(), id16());
  let firstCandidate, staleCandidate, firstBytes, staleBytes;
  try {
    const first = workspace.prepareReview([{text: 'Synthetic stale review baseline', scannerFindings: [], userSelections: []}], id16());
    const stale = workspace.prepareReview([{text: 'Synthetic competing review', scannerFindings: [], userSelections: []}], id16());
    const firstHeads = first.heads(), staleHeads = stale.heads();
    firstBytes = first.exportCandidateBundle();
    firstCandidate = Workspace.openBundle(firstBytes);
    workspace.activateSaved(first, firstHeads.expectedHead, firstHeads.candidateHead);
    const committedHead = workspace.head();
    staleBytes = stale.exportCandidateBundle();
    staleCandidate = Workspace.openBundle(staleBytes);
    let failure;
    try { workspace.activateSaved(stale, staleHeads.expectedHead, staleHeads.candidateHead); }
    catch (error) { failure = error; }
    const sameHead = JSON.stringify(committedHead) === JSON.stringify(workspace.head());
    if (!failure || failure.code !== 'workspace_conflict' || !sameHead) throw new Error('stale prepared review activation was not rejected without changing the current Core head');
    return {status: 'pass', detail: 'Core rejected activation of a second prepared review from an obsolete parent head'};
  } finally {
    firstCandidate?.close(); staleCandidate?.close();
    firstBytes?.fill(0); staleBytes?.fill(0);
    workspace.close();
  }
}

globalThis.runBenchmark = async (fixture, run) => {
  const trialId = run.trialId;
  const backend = run.backend;
  const mode = fixture.scenario.mode;
  const route = {id: backend === 'sqlite' ? hex(id16()) : `datafog-synthetic-${hex(id16())}`,
    recordId: Array.from(id16()), namespace: Array.from(id16())};
  const resourceBefore = await storageSize();
  const instrumentation = {longTasks: [], measureUserAgentSpecificMemory: {status: 'unavailable', reason: 'Chrome has not exposed the API to this extension origin'}};
  try {
    await timed('sdkInitialization', 'WASM init() called', 'WASM init() resolved', () => init());
    gates.stale_review_rejected = checkStaleCoreReview();
    if (typeof PerformanceObserver !== 'undefined') {
      try { new PerformanceObserver(entries => instrumentation.longTasks.push(...entries.getEntries().map(entry => ({durationMs: entry.duration, startTimeMs: entry.startTime, attribution: entry.attribution?.map(item => ({name: item.name, entryType: item.entryType})) || []})))).observe({type: 'longtask', buffered: true}); instrumentation.longTasksStatus = 'measured'; }
      catch (error) { instrumentation.longTasksStatus = 'unavailable'; instrumentation.longTasksReason = error?.name || 'longtask observer unavailable'; }
    }
    const workload = await loadInputs(fixture, mode);
    const populatedInputs = workload.inputs;
    const created = await timed('vaultCreate', 'backend create called', 'new encrypted vault created', async () => {
      if (backend === 'sqlite') return {vault: await SQLiteVault.create({id: route.id, password: PASSWORD})};
      const vault = new FogVault(FOGVAULT_OPTIONS); await vault.create(route.id, PASSWORD); return {vault};
    });
    const initial = await timed('workspaceCreate', 'Core Workspace create called', 'empty complete bundle confirmed saved and activated', async () => {
      if (backend === 'sqlite') return {vault: created.vault, owner: await SQLiteWorkspace.create({Workspace: observingWorkspace(), vault: created.vault,
        recordId: new Uint8Array(route.recordId), namespace: new Uint8Array(route.namespace)})};
      return {vault: created.vault, owner: await FogVaultWorkspace.create({Workspace: observingWorkspace(), vault: created.vault,
        recordId: new Uint8Array(route.recordId), namespace: new Uint8Array(route.namespace)})};
    });
    await closeBackend(initial);

    let wrongCredentialRejected = false;
    try {
      if (backend === 'sqlite') await SQLiteVault.open({id: route.id, password: 'Definitely wrong synthetic password'});
      else { const wrong = new FogVault(FOGVAULT_OPTIONS); await wrong.open(route.id); try { await wrong.unlock('Definitely wrong synthetic password'); } finally { await wrong.close(); } }
    } catch { wrongCredentialRejected = true; }
    gates.wrong_credentials_rejected = {status: wrongCredentialRejected ? 'pass' : 'fail', detail: 'wrong synthetic password rejected before any Workspace was opened'};

    let opened;
    if (backend === 'sqlite') {
      const vault = await timed('initialVaultPasswordUnlock', 'SQLiteVault.open called with the correct synthetic password', 'authenticated vault opened',
        () => SQLiteVault.open({id: route.id, password: PASSWORD}));
      opened = await timed('initialWorkspaceOpen', 'SQLiteWorkspace.open called for empty existing workspace', 'empty encrypted bundle decrypted and Core Workspace reconstructed',
        () => SQLiteWorkspace.open({Workspace: observingWorkspace(), vault, recordId: new Uint8Array(route.recordId), expectedNamespace: new Uint8Array(route.namespace)}).then(owner => ({vault, owner})));
    } else {
      const vault = new FogVault(FOGVAULT_OPTIONS);
      await timed('vaultContainerOpen', 'FogVault.open called', 'vault container opened', () => vault.open(route.id));
      await timed('initialVaultPasswordUnlock', 'FogVault.unlock called with correct synthetic password', 'vault unlocked', () => vault.unlock(PASSWORD));
      opened = await timed('initialWorkspaceOpen', 'FogVaultWorkspace.open called for empty existing workspace', 'empty encrypted bundle decrypted and Core Workspace reconstructed',
        () => FogVaultWorkspace.open({Workspace: observingWorkspace(), vault, recordId: new Uint8Array(route.recordId), expectedNamespace: new Uint8Array(route.namespace)}).then(owner => ({vault, owner})));
    }
    let owner = opened.owner;
    let history;
    if (workload.seedInputs.length) {
      const seed = workload.seedInputs;
      const seedSaved = await prepareAndCommit(owner, seed, fixture, mode, 'seed', 'seed');
      addMarkers(seedSaved.previews);
    }
    history = await timed('historyPopulation', 'history recipe started', 'all history events durably saved and activated', () => populateHistory(owner, fixture));
    await closeBackend(opened);
    if (backend === 'sqlite') {
      const vault = await timed('vaultPasswordUnlock', 'SQLiteVault.open called with correct password for populated workspace', 'vault credential accepted',
        () => SQLiteVault.open({id: route.id, password: PASSWORD}));
      opened = await timed('existingWorkspaceReopen', 'SQLiteWorkspace.open called for populated existing workspace', 'complete encrypted bundle decrypted and Core Workspace reconstructed',
        () => SQLiteWorkspace.open({Workspace: observingWorkspace(), vault, recordId: new Uint8Array(route.recordId), expectedNamespace: new Uint8Array(route.namespace)}).then(owner => ({vault, owner})));
    } else {
      const vault = new FogVault(FOGVAULT_OPTIONS);
      await timed('existingVaultContainerOpen', 'FogVault.open called for populated workspace', 'vault container opened', () => vault.open(route.id));
      await timed('vaultPasswordUnlock', 'FogVault.unlock called with correct password for populated workspace', 'vault credential accepted', () => vault.unlock(PASSWORD));
      opened = await timed('existingWorkspaceReopen', 'FogVaultWorkspace.open called for populated existing workspace', 'complete encrypted bundle decrypted and Core Workspace reconstructed',
        () => FogVaultWorkspace.open({Workspace: observingWorkspace(), vault, recordId: new Uint8Array(route.recordId), expectedNamespace: new Uint8Array(route.namespace)}).then(owner => ({vault, owner})));
    }
    owner = opened.owner;
    const countsBeforeTrial = toPlain(owner.counts());

    const plan = await timed('trialPreparation', 'trial review preparation called', 'trial review prepared', async () => owner.prepareReview(populatedInputs, id16()));
    const saved = await timed('approvalToSavedActivation', 'explicit approved:true passed to commit', 'encrypted save confirmed, Core activated, and handoff preview returned',
      async () => {
        bundleScope = 'trial';
        const result = await owner.commit(plan, {approved: true});
        if (!result?.previews || result.previews.length !== populatedInputs.length) throw new Error('confirmed save did not expose the complete handoff batch');
        addMarkers(result.previews);
        return result;
      });
    const restoration = [];
    for (let index = 0; index < saved.previews.length; index++) {
      const restored = owner.restore(saved.previews[index].text);
      const exact = restored === populatedInputs[index].text;
      restoration.push({inputIndex: index, exact});
      if (!exact) throw new Error(`exact restoration failed for input ${index}`);
    }
    gates.no_handoff_before_confirmed_save_and_activation = {status: 'pass', detail: 'flow reads commit previews only after awaiting the encrypted adapter commit/activation promise'};

    const logicalId = id16(), surface = 'synthetic-browser-benchmark';
    const first = populatedInputs[0];
    const conversationPlan = owner.prepareConversationSend([{input: findingInput(first),
      conversation: {id: logicalId, surface, siteId: null, siteIdByteBudget: 64}}], {id: id16(), occurredAtMs: 1700000001000});
    const sendIntent = conversationPlan.sendIntents()[0];
    const conversationSaved = await timed('observedSendPersistence', 'conversation review commit called', 'initial message and send intent encrypted-save confirmed', async () => {
      const result = await owner.commit(conversationPlan, {approved: true});
      addMarkers(result.previews);
      if (owner.sendIntentStatus(sendIntent) !== 'pending') throw new Error('save implicitly observed a send');
      const observed = owner.prepareObservedSend(sendIntent, logicalId, surface, 'synthetic-confirmed-chat', {id: id16(), occurredAtMs: 1700000001001});
      await owner.commit(observed, {approved: true});
      if (owner.sendIntentStatus(sendIntent) !== 'observed_sent') throw new Error('observed-send was not activated after save');
      return result;
    });
    const observedHead = owner.head();
    const scopedText = conversationSaved.previews[0].text;
    const warmRestored = await timed('warmScopedRestore', 'restoreInConversation called with saved observed-send marker', 'scoped restore returned',
      async () => owner.restoreInConversation(scopedText, logicalId));
    if (warmRestored !== first.text) throw new Error('warm scoped restore differs from source');
    gates.conversation_scope_enforced = {status: 'pass', detail: 'scoped restore succeeds only after a separately saved observed-send record'};
    gates.conflicting_save_rejected_without_overwrite = {status: 'unsupported', detail: 'not exercised per trial; pinned adapters have separate conflict conformance suites'};
    gates.lost_save_reply_recovers_without_duplicate_activation = {status: 'unsupported', detail: 'not exercised per trial; pinned adapters have separate lost-reply conformance suites'};
    gates.lock_during_save_prevents_handoff = {status: 'unsupported', detail: 'not exercised per trial; pinned adapters have separate lock/fault conformance suites'};
    if (gates.stale_review_rejected?.status !== 'pass') gates.stale_review_rejected = {status: 'unsupported', detail: 'Core stale-review qualification was not completed'};

    let backupExport;
    if (typeof owner.exportBackup === 'function') {
      backupExport = await timed('backupExport', 'adapter exportBackup called', 'authenticated encrypted backup returned', async () => owner.exportBackup({password: PASSWORD}));
      gates.backupExport = {status: backupExport instanceof Uint8Array ? 'pass' : 'fail'};
      await timed('backupRecovery', 'approved encrypted backup import called into a fresh empty vault', 'Core recovery activated and mappings/authority verified', async () => {
        const recoveryRoute = {id: hex(id16()), recordId: id16()};
        const recoveryVault = await SQLiteVault.create({id: recoveryRoute.id, password: PASSWORD});
        const recovered = await SQLiteWorkspace.importBackup({Workspace: observingWorkspace(), vault: recoveryVault,
          recordId: recoveryRoute.recordId, backup: backupExport, password: PASSWORD, approved: true, operationId: id16(), requestId: id16()});
        try {
          for (let index = 0; index < saved.previews.length; index++) if (recovered.restore(saved.previews[index].text) !== populatedInputs[index].text) throw new Error('backup recovery changed mappings');
          if (recovered.sendIntentStatus(sendIntent) !== 'observed_sent' || recovered.restoreInConversation(scopedText, logicalId) !== first.text) throw new Error('backup recovery changed send authority');
        } finally { await recovered.close(); }
      });
      gates.backup_recovery_preserves_mappings_and_authority = {status: 'pass', detail: 'fresh SQLite vault restored all trial mappings and observed-send scope'};
    } else {
      intervals.backupExport = {status: 'unsupported', reason: 'published Core-owned FogVault adapter does not expose a backup export API'};
      intervals.backupRecovery = {status: 'unsupported', reason: 'no authenticated backup bytes can be exported by the published Core-owned FogVault adapter'};
      if (capturedCandidateBundle instanceof Uint8Array) {
        await timed('authenticatedBundleRecovery', 'approved authenticated Core bundle imported into a fresh FogVault vault', 'Core recovery activated and mappings/authority verified', async () => {
          const recoveryVault = new FogVault(FOGVAULT_OPTIONS);
          const recoveryId = `datafog-synthetic-recovery-${hex(id16())}`;
          await recoveryVault.create(recoveryId, PASSWORD);
          const recovered = await FogVaultWorkspace.importAuthenticatedBundle({Workspace: observingWorkspace(), vault: recoveryVault,
            recordId: id16(), bundle: capturedCandidateBundle, expectedNamespace: new Uint8Array(route.namespace), expectedHead: observedHead,
            approved: true, operationId: id16(), requestId: id16()});
          try {
            for (let index = 0; index < saved.previews.length; index++) if (recovered.restore(saved.previews[index].text) !== populatedInputs[index].text) throw new Error('bundle recovery changed mappings');
            if (recovered.sendIntentStatus(sendIntent) !== 'observed_sent' || recovered.restoreInConversation(scopedText, logicalId) !== first.text) throw new Error('bundle recovery changed send authority');
          } finally { await recovered.close(); }
        });
        gates.backup_recovery_preserves_mappings_and_authority = {status: 'pass', detail: 'Core authenticated bundle recovery preserved mappings and observed-send authority; standalone encrypted backup export is unavailable for FogVault'};
      } else {
        intervals.authenticatedBundleRecovery = {status: 'unavailable', reason: 'Core candidate bundle could not be captured'};
        gates.backup_recovery_preserves_mappings_and_authority = {status: 'unsupported', detail: 'no authenticated bundle was available to exercise recovery'};
      }
    }
    const counts = toPlain(owner.counts());
    const head = toPlain(owner.head());
    const storageAfter = await storageSize();
    const measuredBundleBytes = bundleSizes.filter(sample => sample.kind === 'candidate_bundle' && sample.phase === 'trial').at(-1)?.bytes;
    const sendIntentReceipt = {preparationId: Array.from(sendIntent.preparationId), inputIndex: sendIntent.inputIndex};
    localStorage.setItem(`benchmark:${trialId}`, JSON.stringify({route, markerTexts: saved.previews.map(item => item.text), originals: populatedInputs.map(item => item.text),
      scopedText, scopedOriginal: first.text, logicalId: Array.from(logicalId), sendIntent: sendIntentReceipt, head}));
    await closeBackend(opened);
    if (cnnSession) { cnnSession.close(); cnnSession = undefined; }
    capturedCandidateBundle?.fill(0); capturedCandidateBundle = undefined;
    const expected = fixture.expected || {};
    gates.mapping_target_admitted = {status: observedMarkers.size === expected.finalMappingCount ? 'pass' : 'fail', detail: {expected: expected.finalMappingCount, uniqueCommittedMarkersObserved: observedMarkers.size}};
    const expectedHistory = fixture.expected?.history?.recipe?.expectedBeforeTrial;
    const historyPass = !expectedHistory || (countsBeforeTrial.operations === expectedHistory.operations &&
      countsBeforeTrial.confirmedHandoffs === expectedHistory.confirmedSends &&
      history.conversationContexts === expectedHistory.conversationContexts &&
      history.simulatedSends === expectedHistory.confirmedSends && observedMarkers.size === expectedHistory.activeMappings);
    gates.history_recipe = {status: historyPass ? 'pass' : 'fail',
      detail: {expectedBeforeTrial: expectedHistory, actualBeforeTrialCounts: countsBeforeTrial, history, uniqueCommittedMarkersBeforeTrial: observedMarkers.size}};
    const resourceDelta = Number.isFinite(resourceBefore.usage) && Number.isFinite(storageAfter.usage) ? storageAfter.usage - resourceBefore.usage : undefined;
    const result = {
      scenarioId: fixture.scenario.scenarioId, scenarioVersion: fixture.scenario.scenarioVersion, backend, trialId,
      profileId: run.profileId, runKind: run.runKind, admission: {status: 'admitted'},
      correctness: {gates}, timings: intervals,
      measurements: {counts, head, history, storageEstimateBefore: toPlain(resourceBefore), storageEstimateAfter: toPlain(storageAfter), storageEstimateDeltaBytes: resourceDelta,
        processMemory: {status: 'collected-by-host', method: 'ps -o rss= for Chrome main process', process: 'browser'},
        browserLongTasks: {status: instrumentation.longTasksStatus || 'unavailable', entries: instrumentation.longTasks},
        mainThread: {status: instrumentation.longTasksStatus || 'unavailable', source: 'PerformanceObserver longtask entries in extension page'},
        workerMemory: {status: 'unavailable', reason: 'Chrome does not expose isolated worker heap attribution to this harness'},
        wasmMemory: {status: 'unavailable', reason: 'WASM heap cannot be separated from extension page JavaScript heap'},
        storageBytesWritten: {status: 'unavailable', reason: 'browser APIs do not expose reliable per-operation disk write bytes'},
        serializedWorkspaceBundleBytes: measuredBundleBytes ?? null,
        serializedWorkspaceBundleStatus: measuredBundleBytes === undefined ? 'unavailable' : 'measured by observing exact Core candidate bundle bytes passed to adapter',
        encryptedArtifactBytes: {status: 'unavailable', reason: 'published adapter APIs return decrypted records; they do not expose sealed on-disk payload size'},
        bundleSamples: bundleSizes,
        restoration, backupExportBytes: backupExport?.byteLength ?? null,
        instrumentationOverhead: {status: 'not separately calibrated', note: 'named intervals use performance.now; size and long-task observers add bounded work outside most intervals'}},
      backendConfiguration: backend === 'sqlite' ? {adapter: '@datafog/core-sqlite-browser@1.1.0', sqlite3MultipleCiphers: 'published vendor assets; exact lock recorded with package hashes', durability: 'shipped adapter default'}
        : {adapter: '@datafog/core-fogvault-browser@1.1.0', fogvaultOptions: FOGVAULT_OPTIONS, durability: 'FogVault published default'},
      exclusionReason: Object.values(gates).some(gate => gate.status === 'fail') ? 'one or more correctness gates failed' : null,
    };
    globalThis.benchmarkDone = result;
    return result;
  } catch (error) {
    if (cnnSession) { try { cnnSession.close(); } catch {} cnnSession = undefined; }
    const code = error?.code || error?.name || 'error';
    if (code === 'resource_limit') globalThis.benchmarkDone = {scenarioId: fixture.scenario.scenarioId, backend, trialId, admission: {status: 'rejected', reason: code}, correctness: {gates}, timings: intervals, failure: {code, message: error.message}, exclusionReason: 'resource admission rejected'};
    else globalThis.benchmarkDone = {scenarioId: fixture.scenario.scenarioId, backend, trialId, admission: {status: 'unknown'}, correctness: {gates}, timings: intervals, failure: {code, message: error.message}, exclusionReason: 'trial failed before all correctness gates completed'};
    throw error;
  }
};

globalThis.runBenchmarkRestart = async (trialId, backend) => {
  const saved = JSON.parse(localStorage.getItem(`benchmark:${trialId}`));
  if (!saved) throw new Error('restart route was not persisted in isolated extension profile');
  const route = saved.route, recordId = new Uint8Array(route.recordId), namespace = new Uint8Array(route.namespace);
  const ObservedWorkspace = observingWorkspace();
  await timed('postRestartSdkInitialization', 'WASM init() called after new Chrome process start', 'WASM init() resolved', () => init());
  let opened;
  if (backend === 'sqlite') {
    const vault = await timed('postRestartPasswordUnlock', 'SQLiteVault.open called after Chrome process restart', 'vault credential accepted', () => SQLiteVault.open({id: route.id, password: PASSWORD}));
    opened = await timed('postRestartReopen', 'adapter workspace open called after restart', 'encrypted bundle decrypted and Core Workspace reconstructed',
      () => SQLiteWorkspace.open({Workspace: ObservedWorkspace, vault, recordId, expectedNamespace: namespace}).then(owner => ({vault, owner})));
  } else {
    const vault = new FogVault(FOGVAULT_OPTIONS);
    await timed('postRestartVaultOpen', 'FogVault.open called after Chrome process restart', 'vault container opened', () => vault.open(route.id));
    await timed('postRestartPasswordUnlock', 'FogVault.unlock called after Chrome process restart', 'credential accepted', () => vault.unlock(PASSWORD));
    opened = await timed('postRestartReopen', 'adapter workspace open called after restart', 'encrypted bundle decrypted and Core Workspace reconstructed',
      () => FogVaultWorkspace.open({Workspace: ObservedWorkspace, vault, recordId, expectedNamespace: namespace}).then(owner => ({vault, owner})));
  }
  const data = JSON.parse(localStorage.getItem(`benchmark:${trialId}`));
  const logicalId = new Uint8Array(data.logicalId);
  const restored = await timed('postRestartScopedRestore', 'restoreInConversation called after full Chrome process restart and explicit credential unlock', 'scoped restore returned',
    () => opened.owner.restoreInConversation(data.scopedText, logicalId));
  const exact = restored === data.scopedOriginal;
  if (!exact) throw new Error('post-restart scoped restore differs from source');
  const pending = opened.owner.sendIntentStatus(data.sendIntent);
  if (pending !== 'observed_sent') throw new Error('saved observed-send authority did not survive browser restart');
  const result = {trialId, backend, timings: intervals, correctness: {gates: {exact_restore_after_browser_restart: {status: 'pass', detail: 'exact post-restart scoped restoration succeeded'}, observedSendAuthorityAfterRestart: {status: 'pass'}}},
    counts: toPlain(opened.owner.counts()), storageEstimate: toPlain(await storageSize())};
  await closeBackend(opened);
  globalThis.benchmarkRestartDone = result;
  return result;
};

globalThis.benchmarkReady = true;
