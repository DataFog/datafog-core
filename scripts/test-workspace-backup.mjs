import {createDecipheriv, hkdfSync} from "node:crypto";
import {waitForBrokerState} from "./workspace-test-wait.mjs";

const password = "synthetic-test-password-only";
// Independently unwrap with native Node crypto, without exporting browser CryptoKeys.
function freshWorkspaceKey(backup, recoveryCode, recovered) {
  const {payload, nonce, tag} = JSON.parse(backup), vault = payload.vault;
  const decode = value => Buffer.from(value, "base64url");
  const decrypt = (key, iv, aad, ciphertext) => {
    const cipher = createDecipheriv("aes-256-gcm", key, decode(iv));
    cipher.setAAD(aad);
    const bytes = decode(ciphertext); cipher.setAuthTag(bytes.subarray(-16));
    return Buffer.concat([cipher.update(bytes.subarray(0, -16)), cipher.final()]);
  };
  let wrapping, root, originalKey, newKey;
  const secret = decode(recoveryCode.slice(5));
  try {
    wrapping = Buffer.from(hkdfSync("sha256", secret, decode(vault.recovery.salt), Buffer.concat([Buffer.from("DFG2:RECOVERY:HKDF:"), decode(vault.vaultId), decode(vault.vaultKeyId)]), 32));
    root = decrypt(wrapping, vault.recovery.nonce, Buffer.concat([Buffer.from("DFG2:RC:"), decode(vault.vaultId), decode(vault.vaultKeyId), Buffer.from([1]), decode(vault.recovery.salt)]), vault.recovery.wrappedRoot);
    const unwrap = record => decrypt(root, record.keyNonce, Buffer.concat([Buffer.from("DFG1:WK:"), decode(record.vaultId), decode(record.workspaceId), decode(record.keyId)]), record.wrappedKey);
    originalKey = unwrap(payload.workspace); newKey = unwrap(recovered);
    const empty = decrypt(originalKey, nonce, Buffer.from(`DFGB${payload.version}:AUTH:${JSON.stringify(payload)}`), tag);
    return empty.length === 0 && !originalKey.equals(newKey);
  } finally { secret.fill(0); wrapping?.fill(0); root?.fill(0); originalKey?.fill(0); newKey?.fill(0); }
}
async function review(page, backup, credential = password, method = "password") {
  await page.locator("#backup-file").setInputFiles({name: "workspace.dfbackup", mimeType: "application/octet-stream", buffer: backup});
  await page.locator("#backup-method").selectOption(method);
  await page.locator("#backup-credential").fill(credential);
  await page.locator("#backup-review").click();
}
async function markName(page, name, status) {
  await page.locator("#source").fill(name);
  await page.locator("#entity-type").fill("PERSON");
  await page.locator("#source").evaluate(element => element.setSelectionRange(0, element.value.length));
  await page.locator("#mark").click(); await status(page, "selection_added");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  return page.locator("#preview").textContent();
}
async function fingerprint(page) {
  return page.evaluate(async () => {
    const {readRecords} = await import("./storage.js");
    const bytes = new TextEncoder().encode(JSON.stringify(await readRecords()));
    return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))).join(",");
  });
}
async function verifyCurrentFieldBackups({launch,close,unlock,status,check,fixture}) {
  let page=await launch("backup-current-field-source");
  const source=await page.evaluate(async({fixture,password})=>{
    const {init,Workspace}=await import("./vendor/package/index.js");await init();
    const {createVault}=await import("./vault.js");const {exportBackup}=await import("./backup.js");
    const bytes=Uint8Array.from(fixture.current.bundleHex.match(/../g),pair=>Number.parseInt(pair,16));
    const workspace=Workspace.openBundle(bytes);
    try {
      const created=await createVault(password,workspace.namespace(),bytes,workspace.head());
      return {bytes:Array.from(await exportBackup(created.vault,created.record,created.dataKey,password)),recoveryCode:created.recoveryCode};
    } finally{workspace.close();bytes.fill(0);}
  },{fixture,password});
  const backup=Buffer.from(source.bytes);
  check(![password,source.recoveryCode,...fixture.current.aliases.map(alias=>alias.value),...fixture.current.policy.choices.map(choice=>choice.value)].some(value=>backup.includes(Buffer.from(value))),"B2 current-state backup excludes plaintext values and credentials");
  await close();
  for(const [method,credential]of [["password",password],["recovery",source.recoveryCode]]) {
    const profile=`backup-current-fields-${method}`;page=await launch(profile);
    await review(page,backup,credential,method);await status(page,"backup_preview_ready");
    await page.locator("#backup-approve").click();await status(page,"backup_restored_locked");
    await close();page=await launch(profile);await unlock(page);
    const proof=await page.evaluate(async({fixture,password})=>{
      const {init,Workspace}=await import("./vendor/package/index.js");await init();
      const {readRecords}=await import("./storage.js");const {openVault,toBase64url}=await import("./vault.js");
      const {vault,workspace:record}=await readRecords();const opened=await openVault(password,vault,record);let workspace;
      try {
        workspace=Workspace.openBundle(opened.bundle);const recovered=workspace.exportBundle();
        const original=Uint8Array.from(fixture.current.bundleHex.match(/../g),pair=>Number.parseInt(pair,16));
        // Inspect actual CBOR value spans; recovery may add fields or widen the head.
        // This test decoder supports the definite primitive/container forms in canonical workspace fixtures.
        function fields(bytes) {
          let position=0;
          function item() {
            const start=position,initial=bytes[position++],major=initial>>5,info=initial&31;
            let value=info;
            if(info>=24&&info<=27) {
              const width=2**(info-24);value=0;
              for(let index=0;index<width;index++) value=value*256+bytes[position++];
            } else if(info>=28) throw new Error("unexpected indefinite fixture item");
            if(major===2||major===3) position+=value;
            else if(major===4) for(let index=0;index<value;index++) item();
            else if(major===5) for(let index=0;index<value*2;index++) item();
            else if(![0,1,7].includes(major)) throw new Error("unexpected fixture type");
            if(position>bytes.length) throw new Error("truncated fixture");
            return {major,value,start,end:position};
          }
          const initial=bytes[position++];
          if(initial>>5!==5||(initial&31)>=24) throw new Error("unexpected fixture map");
          const values=new Map();
          for(let index=0;index<(initial&31);index++) {
            const key=item();if(key.major!==0||values.has(key.value)) throw new Error("unexpected fixture key");
            const value=item();values.set(key.value,bytes.slice(value.start,value.end));
          }
          if(position!==bytes.length) throw new Error("trailing fixture data");
          return values;
        }
        const originalFields=fields(original),recoveredFields=fields(recovered);
        const preserved=[1,3,4,5,6,7].every(key=>{
          const before=originalFields.get(key),after=recoveredFields.get(key);
          return before&&after&&before.length===after.length&&after.every((value,index)=>value===before[index]);
        });
        const aliases=fixture.current.aliases.every(alias=>workspace.restore(alias.marker)===alias.value);
        const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==="object"?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
        const policy=workspace.policy();
        const sourceWorkspace=Workspace.openBundle(original);
        let expectedVersion;
        try {
          const staged=sourceWorkspace.prepareRecovery(new Uint8Array(16).fill(49));
          try {expectedVersion=staged.snapshotFormat().candidateVersion;}finally{staged.discard();}
        } finally{sourceWorkspace.close();}
        const settings=policy.snapshotVersion===expectedVersion && ["defaultRule","rules","choices"].every(key=>JSON.stringify(canonical(policy[key]))===JSON.stringify(canonical(fixture.current.policy[key])));
        const oldHead=fixture.current.head,newHead=workspace.head();
        const freshHead=BigInt(newHead.revision)===BigInt(oldHead.revision)+1n && newHead.commitId.join(",")!==oldHead.commitId.join(",");
        const envelope=Object.fromEntries(["vaultId","workspaceId","keyId","keyNonce","wrappedKey","nonce"].map(key=>[key,toBase64url(record[key])]));
        const recall=workspace.recall("👋 i\u0307\tJANE");
        return {preserved,aliases,settings,freshHead,envelope,recall:recall.findings.length===1 && recall.findings[0].finding.matchedText==="i\u0307\tJANE"};
      } finally{workspace?.close();opened.bundle.fill(0);}
    },{fixture,password});
    check(proof.preserved && proof.aliases && proof.settings && proof.freshHead && proof.recall,"B2 supported field bytes, Unicode identities, settings and fresh recovery head survive restart");
    check(freshWorkspaceKey(backup,source.recoveryCode,proof.envelope),"B2 independent native crypto confirms fresh destination workspace key");
    await close();
  }
  page=await launch("backup-current-field-rejections");const baseline=await fingerprint(page);
  for(const candidate of fixture.rejections.filter(candidate=>candidate.encryptedImport)) {
    const file=await page.evaluate(async({fixture,candidate,password})=>{
      const {init,Workspace}=await import("./vendor/package/index.js");await init();
      const {createVault}=await import("./vault.js");const {exportBackup}=await import("./backup.js");
      const original=Uint8Array.from(fixture.current.bundleHex.match(/../g),pair=>Number.parseInt(pair,16));
      const invalid=Uint8Array.from(candidate.bundleHex.match(/../g),pair=>Number.parseInt(pair,16));
      const workspace=Workspace.openBundle(original);
      try {const created=await createVault(password,workspace.namespace(),invalid,workspace.head());return Array.from(await exportBackup(created.vault,created.record,created.dataKey,password));}
      finally{workspace.close();original.fill(0);invalid.fill(0);}
    },{fixture,candidate,password});
    await review(page,Buffer.from(file));await status(page,candidate.code);
    check(await fingerprint(page)===baseline && await page.locator("#backup-approve").isDisabled() && await page.locator("#backup-preview").textContent()==="","B2 authenticated invalid current field writes nothing and exposes no usable preview");
  }
  await close();
}

export async function verifyBackups({fresh, prepare, status, close, launch, unlock, restored, control, paused, check, backupFields}) {
  let page = await fresh("backup-source");
  check(await page.locator("#recovery-created").count() === 1, "G1 vault creation supplies an offline recovery code");
  const recoveryCode = await page.locator("#recovery-created").inputValue();
  check(/^DFR1\.[A-Za-z0-9_-]{43}$/.test(recoveryCode), "G1 versioned random recovery code");
  await page.locator("#lock").click(); await status(page, "locked");
  check(await page.locator("#recovery-created").inputValue() === "", "G1 lock clears the one-time recovery code display");
  await unlock(page);
  const output = await prepare(page);
  await page.locator("#approve").click(); await status(page, "saved");
  const unsaved = await markName(page, "Unsaved Synthetic", status);
  const beforeExport = await fingerprint(page);
  const workerStartFailures = await page.evaluate(async password => {
    const {readRecords} = await import("./storage.js");
    const {openVault} = await import("./vault.js");
    const {exportBackup} = await import("./backup.js");
    const {vault, workspace} = await readRecords();
    const opened = await openVault(password, vault, workspace);
    const OriginalWorker = globalThis.Worker;
    let attempts = 0;
    try {
      globalThis.Worker = class {
        constructor() { attempts++; throw new DOMException("synthetic worker startup denied", "SecurityError"); }
      };
      const aborted = new AbortController(); aborted.abort();
      const errors = [];
      for (const attempt of [
        () => exportBackup(vault, workspace, opened.dataKey, ""),
        () => exportBackup(vault, workspace, opened.dataKey, password, aborted.signal),
        () => exportBackup(vault, workspace, opened.dataKey, password),
        () => openVault(password, vault, workspace),
      ]) {
        try { await attempt(); errors.push("unexpected_success"); }
        catch (error) { errors.push(error.code); }
      }
      return {attempts, errors};
    } finally { globalThis.Worker = OriginalWorker; opened.bundle.fill(0); }
  }, password);
  check(workerStartFailures.attempts === 2 && workerStartFailures.errors.join() === "invalid_credentials,cancelled,kdf_unavailable,kdf_unavailable",
    "B3 worker construction failure is unavailable while invalid credentials and cancellation retain their codes");
  check(await fingerprint(page) === beforeExport, "B3 worker startup failure changes no encrypted records");
  await page.locator("#backup-export-password").fill("incorrect-synthetic-password");
  await page.locator("#backup-export").click(); await status(page, "invalid_credentials");
  check(await fingerprint(page) === beforeExport && await page.locator("#backup-export-password").inputValue() === "", "B3 failed export reauthentication changes no records and clears the password");
  await page.locator("#backup-export-password").fill(password);
  const downloadEvent = page.waitForEvent("download");
  await page.locator("#backup-export").click();
  const download = await downloadEvent;
  await status(page, "backup_exported");
  const {readFile} = await import("node:fs/promises");
  const backup = await readFile(await download.path());
  const exportProperties = await page.evaluate(async raw => {
    const {readRecords} = await import("./storage.js");
    const {toBase64url} = await import("./vault.js");
    const {vault} = await readRecords(), file = JSON.parse(raw);
    return {version: file.payload.version, profile: file.payload.vault.profile,
      excludesOld: ![vault.salt, vault.rootNonce, vault.wrappedRoot].some(bytes => raw.includes(toBase64url(bytes))),
      liveProfile: vault.profile};
  }, backup.toString());
  check(exportProperties.version === 2 && exportProperties.profile === 2 && exportProperties.excludesOld && exportProperties.liveProfile === 1, "B3 v2 export replaces the complete old password wrapper without changing the live vault");
  check(await page.locator("#backup-export-password").inputValue() === "", "B3 export clears reauthentication input");
  // Build a legacy v1 file independently from stored profile-1 ciphertext, as old clients did.
  const legacyBackup = Buffer.from(await page.evaluate(async ({raw, password}) => {
    const {readRecords} = await import("./storage.js");
    const {openVault, toBase64url} = await import("./vault.js");
    const {vault, workspace} = await readRecords(), file = JSON.parse(raw);
    const opened = await openVault(password, vault, workspace);
    try {
      file.payload.version = 1;
      for (const key of ["profile", "salt", "rootNonce", "wrappedRoot"]) file.payload.vault[key] = key === "profile" ? vault[key] : toBase64url(vault[key]);
      const nonce = crypto.getRandomValues(new Uint8Array(12));
      const tag = new Uint8Array(await crypto.subtle.encrypt({name: "AES-GCM", iv: nonce, tagLength: 128,
        additionalData: new TextEncoder().encode(`DFGB1:AUTH:${JSON.stringify(file.payload)}`)}, opened.dataKey, new Uint8Array()));
      return JSON.stringify({payload: file.payload, nonce: toBase64url(nonce), tag: toBase64url(tag)});
    } finally { opened.bundle.fill(0); }
  }, {raw: backup.toString(), password}));
  check(![password, recoveryCode, "Jane Smith", "Unsaved Synthetic", unsaved].some(value => backup.includes(Buffer.from(value))), "G1 file excludes plaintext credentials, originals and previews");
  const storedPrivate = await page.evaluate(async ({password, recoveryCode}) => {
    const {readRecords} = await import("./storage.js");
    const {fromBase64url} = await import("./vault.js");
    const prohibited = [password, recoveryCode, "Jane Smith", "Unsaved Synthetic"].map(value => new TextEncoder().encode(value));
    prohibited.push(fromBase64url(recoveryCode.slice(5), 32));
    function contains(value) {
      if (value instanceof CryptoKey) return true;
      if (typeof value === "string") return contains(new TextEncoder().encode(value));
      if (value instanceof Uint8Array) return prohibited.some(needle => value.some((_, offset) => offset + needle.length <= value.length && needle.every((byte, index) => value[offset + index] === byte)));
      return value && typeof value === "object" && Object.values(value).some(contains);
    }
    return contains(await readRecords());
  }, {password, recoveryCode});
  check(!storedPrivate, "G1 persistent records contain no plaintext secret, mapping or pending draft");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  check(await page.locator("#recovery-created").inputValue() === "", "G1 cancellation hides the one-time code");
  const exportAbort = await page.evaluate(async password => {
    const {readRecords} = await import("./storage.js");
    const {rewrapBackupVault} = await import("./vault.js");
    const {vault} = await readRecords();
    for (const immediate of [true, false]) {
      const controller = new AbortController();
      if (immediate) controller.abort();
      const attempt = rewrapBackupVault(vault, password, controller.signal);
      const timer = setTimeout(() => controller.abort(), 10);
      try { await attempt; return false; }
      catch (error) { if (error.code !== "cancelled") return false; }
      finally { clearTimeout(timer); }
    }
    return true;
  }, password);
  check(exportAbort && await fingerprint(page) === beforeExport, "B3 cancellation before/during real KDF rejects without mutating the live vault");
  for (const action of ["cancel", "lock"]) {
    let downloads = 0; const onDownload = () => { downloads++; };
    page.on("download", onDownload);
    await control(page, "configure", {pause: "after-export-wrap"});
    await page.locator("#backup-export-password").fill(password); await page.locator("#backup-export").click();
    await paused(page, "after-export-wrap");
    await page.locator(`#${action}`).click(); await status(page, action === "lock" ? "locked" : "cancelled");
    await waitForBrokerState(() => control(page, "state"), state => !state.busy, {label: "cancel backup export"});
    check(downloads === 0 && await page.locator("#backup-export-password").inputValue() === "", "B3 lock/cancel after rewrap downloads nothing and clears credentials");
    page.off("download", onDownload); await control(page, "configure", {pause: null});
    if (action === "lock") await unlock(page);
  }
  const later = await markName(page, "Bob Synthetic", status);
  await page.locator("#approve").click(); await status(page, "saved");
  const sourceState = await fingerprint(page);
  const pendingPreview = await prepare(page);
  await review(page, backup); await status(page, "workspace_conflict");
  check(await fingerprint(page) === sourceState && (await control(page, "state")).pending, "G2 conflicting import preserves encrypted storage and the active plan");
  check(await page.locator("#preview").textContent() === pendingPreview, "G2 conflicting import preserves the existing preview");
  await page.locator("#approve").click(); await status(page, "saved");
  await page.locator("#lock").click(); await status(page, "locked");
  await page.locator("#backup-export").click(); await status(page, "locked");
  await page.locator("#unlock-method").selectOption("recovery");
  await page.locator("#password").fill(recoveryCode); await page.locator("#unlock").click(); await status(page, "unlocked");
  const recoveryState = await fingerprint(page);
  await page.locator("#backup-export").click(); await status(page, "invalid_credentials");
  check(await fingerprint(page) === recoveryState, "B3 recovery unlock does not enable an export without the vault password");
  await restored(page, output, "restored");
  check(await page.locator("#recovery-created").inputValue() === "", "G1 unlocking does not reveal the recovery code again");
  await close();
  for (const [method, credential] of [["password", password], ["recovery", recoveryCode]]) {
    page = await launch(`backup-${method}`);
    await review(page, backup, credential, method); await status(page, "backup_preview_ready");
    check(await page.locator("#backup-credential").inputValue() === "" && await page.locator("#backup-file").inputValue() === "", "G1 review drops credential and file controls");
    await page.locator("#backup-approve").click(); await status(page, "backup_restored_locked");
    await restored(page, output, "locked");
    await close(); page = await launch(`backup-${method}`);
    await unlock(page);
    const recoveredRecall = await page.evaluate(async password => {
      const {readRecords} = await import("./storage.js");
      const {openVault} = await import("./vault.js");
      const {init, Workspace} = await import("./vendor/package/index.js"); await init();
      const {vault, workspace: record} = await readRecords();
      const opened = await openVault(password, vault, record);
      let workspace;
      try {
        workspace = Workspace.openBundle(opened.bundle);
        const text = "👋 JANE\tSMITH", before = workspace.exportBundle();
        const recall = workspace.recall(text);
        const plan = workspace.prepare(text, recall.findings.map(span => span.finding), new Uint8Array(16).fill(212));
        const result = {recall, restored: workspace.restore(plan.preview().text), unchanged: workspace.exportBundle().every((value, i) => value === before[i])};
        plan.discard(); before.fill(0); return result;
      } finally { workspace?.close(); opened.bundle.fill(0); }
    }, password);
    check(recoveredRecall.recall.findings.length === 1 && recoveredRecall.recall.findings[0].finding.source === "recall" && recoveredRecall.restored === "👋 Jane Smith" && recoveredRecall.unchanged,
      "D5 committed recall survives encrypted backup, fresh recovery keys and browser restart without changing mappings");
    check(await page.evaluate(async () => (await (await import("./storage.js")).readRecords()).vault.profile) === 2, "B3 restored v2 vault retains stronger KDF profile across restart");
    await restored(page, output, "restored");
    check((await page.locator("#restored").textContent()).includes("Jane Smith"), "G1 backup restores old full aliases");
    check(await prepare(page) === output, "G1 imported mapping is byte-identical");
    await restored(page, unsaved, "invalid_marker");
    await restored(page, later, "invalid_marker");
    const replacement = await markName(page, "Carol Synthetic", status);
    check(/^\[\[PERSON_[0-9A-HJKMNP-TV-Z]{6}\]\]$/.test(replacement) && replacement !== later, "G3 fresh issuance identity separates equal counters after recovery");
    await page.locator("#approve").click(); await status(page, "saved");
    await restored(page, `${output} ${later}`, "invalid_marker");
    check(await page.locator("#restored").textContent() === "", "G3 missing post-backup alias never restores as the new identity");
    const rekeyed = await page.evaluate(async () => {
      const {readRecords} = await import("./storage.js");
      const {toBase64url} = await import("./vault.js");
      const {workspace} = await readRecords();
      return {...Object.fromEntries(["vaultId", "workspaceId", "keyId", "keyNonce", "wrappedKey", "nonce"].map(key => [key, toBase64url(workspace[key])])), commitId: toBase64url(new Uint8Array(workspace.head.commitId))};
    });
    const original = JSON.parse(backup).payload.workspace;
    check(rekeyed.keyId !== original.keyId && rekeyed.wrappedKey !== original.wrappedKey && rekeyed.nonce !== original.nonce && rekeyed.commitId !== original.head.commitId, "G3 recovered storage has fresh keys, nonce and commit head");
    check(freshWorkspaceKey(backup, recoveryCode, rekeyed), "G3 native HKDF/AES-GCM verifies backup authentication and a different destination data key");
    await close();
  }
  console.log("PASS G1/G3 encrypted backup, both credentials, fresh storage and rollback-safe alias reuse");

  for (const [method, credential] of [["password", password], ["recovery", recoveryCode]]) {
    page = await launch(`legacy-backup-${method}`);
    await review(page, legacyBackup, credential, method); await status(page, "backup_preview_ready");
    await page.locator("#backup-approve").click(); await status(page, "backup_restored_locked");
    await close(); page = await launch(`legacy-backup-${method}`); await unlock(page);
    await restored(page, output, "restored");
    check((await page.locator("#restored").textContent()).includes("Jane Smith"), "B3 legacy v1 backup still restores with either credential after restart");
    await close();
  }
  page = await launch("v1-vault-compat");
  const v1VaultWorks = await page.evaluate(async password => {
    const {openVault, randomId} = await import("./vault.js");
    const {init, Workspace} = await import("./vendor/package/index.js");
    await init();
    const encoder = new TextEncoder();
    const random = length => crypto.getRandomValues(new Uint8Array(length));
    function concat(...arrays) {
      const result = new Uint8Array(arrays.reduce((n, a) => n + a.length, 0));
      let offset = 0; for (const array of arrays) { result.set(array, offset); offset += array.length; }
      return result;
    }
    async function importKey(raw) { return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]); }
    async function encrypt(key, iv, aad, plaintext) { return new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key, plaintext)); }
    function keyAAD(record) { return concat(encoder.encode("DFG1:WK:"), record.vaultId, record.workspaceId, record.keyId); }
    function revisionBytes(head) {
      const revision = BigInt(head.revision);
      const array = new Uint8Array(8); new DataView(array.buffer).setBigUint64(0, revision, false);
      return array;
    }
    function snapshotAAD(record) { return concat(encoder.encode("DFG1:SN:"), record.vaultId, record.workspaceId, record.keyId, revisionBytes(record.head), new Uint8Array(record.head.commitId)); }
    function passwordAAD(vault) { return concat(encoder.encode(`DFG${vault.version}:PW:`), vault.vaultId, vault.vaultKeyId, new Uint8Array([vault.profile]), vault.salt); }
    const namespace = randomId();
    const workspace = Workspace.create(namespace, randomId());
    try {
      const bundle = workspace.exportBundle(), head = workspace.head();
      const v1Vault = { version: 1, vaultId: randomId(), vaultKeyId: randomId(), workspaceId: namespace, profile: 1, salt: random(16), rootNonce: random(12), accessGeneration: randomId() };
      const root = random(32), data = random(32);
      const worker = new Worker("./kdf-worker.js", { type: "module" });
      const derived = await new Promise((resolve, reject) => {
        worker.onmessage = ({data}) => { worker.terminate(); resolve(data.key); };
        worker.onerror = () => { worker.terminate(); reject(new Error("KDF failed")); };
        worker.postMessage({password, salt: v1Vault.salt, profile: 1});
      });
      try {
        v1Vault.wrappedRoot = await encrypt(await importKey(derived), v1Vault.rootNonce, passwordAAD(v1Vault), root);
        const rootKey = await importKey(root);
        const record = {version: 1, vaultId: v1Vault.vaultId, workspaceId: v1Vault.workspaceId, keyId: randomId(), head, keyNonce: random(12), nonce: random(12)};
        record.wrappedKey = await encrypt(rootKey, record.keyNonce, keyAAD(record), data);
        const dataKey = await importKey(data);
        record.ciphertext = await encrypt(dataKey, record.nonce, snapshotAAD(record), bundle);
        const opened = await openVault(password, v1Vault, record);
        if (!opened.bundle || opened.bundle.length === 0) return {error: "openVault returned no bundle"};
        opened.bundle.fill(0);
        return {success: true};
      } finally { root.fill(0); data.fill(0); derived.fill(0); }
    } finally { workspace.close(); }
  }, password);
  check(v1VaultWorks.success === true, `G1 v1 vault unlocks via openVault with DFG1:PW: AAD${v1VaultWorks.error ? ` (${v1VaultWorks.error})` : ""}`);
  await close();

  page = await launch("backup-rejections");
  const empty = await fingerprint(page);
  const damaged = mutate => { const file = JSON.parse(backup); mutate(file); return Buffer.from(JSON.stringify(file)); };
  const binary = value => Buffer.from(value, "base64url");
  const flip = value => { const bytes = binary(value); bytes[0] ^= 1; return bytes.toString("base64url"); };
  for (const [bytes, credential, method, expected] of [
    [backup, "incorrect-synthetic-password", "password", "invalid_credentials"],
    [backup, `DFR1.${"A".repeat(43)}`, "recovery", "invalid_credentials"],
    [backup, recoveryCode + "=", "recovery", "invalid_credentials"],
    [backup.subarray(0, -1), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.version = 255; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.vault.profile = 1; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.version = 1; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.vault.memlimit = 1; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.vault.profile = 255; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.exportedAt = "2020-01-01T00:00:00.000Z"; }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.vault.recovery.wrappedRoot = flip(file.payload.vault.recovery.wrappedRoot); }), password, "password", "invalid_backup"],
    [damaged(file => { file.payload.vault.wrappedRoot = flip(file.payload.vault.wrappedRoot); }), recoveryCode, "recovery", "invalid_backup"],
    [damaged(file => { file.payload.workspace.ciphertext = flip(file.payload.workspace.ciphertext); }), password, "password", "invalid_credentials"],
    [damaged(file => { file.tag = flip(file.tag); }), password, "password", "invalid_backup"],
    [Buffer.from(backup.toString().replace('"version":1', '"version":1,"version":1')), password, "password", "invalid_backup"],
    [Buffer.alloc(24 * 1024 * 1024 + 1), password, "password", "resource_limit"],
  ]) {
    await review(page, bytes, credential, method); await status(page, expected);
    check(await fingerprint(page) === empty && await page.locator("#backup-approve").isDisabled() && await page.locator("#backup-preview").textContent() === "", "G2 rejected backup writes nothing and exposes no partial preview");
  }
  await review(page, backup); await status(page, "backup_preview_ready");
  await page.locator("#source").fill("Changed synthetic draft");
  check(await page.locator("#backup-approve").isDisabled() && await page.locator("#backup-preview").textContent() === "", "G4 an edit invalidates pending restore approval");
  check(await fingerprint(page) === empty, "G4 editing a pending restore commits nothing");
  await waitForBrokerState(() => control(page, "state"), state => !state.busy, {label: "backup edit cancellation"});
  check(!(await control(page, "state")).importPending, "G4 editing discards the staged import in the broker");
  for (const mismatch of ["bundle", "namespace", "head"]) {
    const authenticatedInvalid = await page.evaluate(async ({password, mismatch}) => {
      const {init, Workspace} = await import("./vendor/package/index.js");
      await init();
      const {createVault, randomId} = await import("./vault.js");
      const {exportBackup} = await import("./backup.js");
      const namespace = randomId(), workspace = Workspace.create(namespace, randomId());
      try {
        const bundle = workspace.exportBundle(), head = workspace.head();
        if (mismatch === "bundle") bundle[0] = 0;
        if (mismatch === "head") head.revision = "1";
        const created = await createVault(password, mismatch === "namespace" ? randomId() : namespace, bundle, head);
        return Array.from(await exportBackup(created.vault, created.record, created.dataKey, password));
      } finally { workspace.close(); }
    }, {password, mismatch});
    await review(page, Buffer.from(authenticatedInvalid));
    await status(page, mismatch === "bundle" ? "invalid_snapshot" : "invalid_backup");
    check(await fingerprint(page) === empty, "G2 authenticated invalid Core state cannot be imported");
  }
  await close();
  console.log("PASS G2 authenticated metadata/credential slots, malformed files and resource rejection");

  for (const checkpoint of ["after-import-open", "before-import-save", "after-import-save"]) {
    for (const action of ["cancel", "lock"]) {
      const profile = `backup-${action}-${checkpoint}`;
      page = await launch(profile);
      const baseline = await fingerprint(page);
      await control(page, "configure", {pause: checkpoint});
      await review(page, backup);
      if (checkpoint !== "after-import-open") {
        await status(page, "backup_preview_ready");
        await page.locator("#backup-approve").click();
      }
      await paused(page, checkpoint);
      await page.locator(`#${action}`).click(); await status(page, action === "lock" ? "locked" : "cancelled");
      check(!(await control(page, "state")).importPending && await page.locator("#backup-approve").isDisabled(), "G4 cancellation clears staged state and approval");
      check(await page.locator("#sink").textContent() === "", "G4 import cancellation never releases content");
      if (checkpoint === "after-import-save") {
        await close(); page = await launch(profile); await unlock(page);
        await restored(page, output, "restored");
      } else check(await fingerprint(page) === baseline, "G4 pre-commit cancellation changes no stored state");
      await close();
    }
  }
  page = await launch("backup-abort");
  const beforeAbort = await fingerprint(page);
  await review(page, backup); await status(page, "backup_preview_ready");
  await control(page, "configure", {abortSave: true});
  await page.locator("#backup-approve").click(); await status(page, "storage_failure");
  check(await fingerprint(page) === beforeAbort && !(await control(page, "state")).importPending, "G4 aborted restore rolls back vault, wrappers and workspace together");
  await control(page, "configure", {abortSave: false, loseAck: true});
  await review(page, backup); await status(page, "backup_preview_ready");
  await page.locator("#backup-approve").click(); await status(page, "save_uncertain");
  check((await control(page, "state")).locked && await page.locator("#backup-approve").isDisabled() && await page.locator("#sink").textContent() === "", "G4 lost acknowledgment remains locked with no resend or stale approval");
  await close(); page = await launch("backup-abort"); await unlock(page);
  await restored(page, output, "restored"); await close();

  page = await launch("backup-race");
  const second = await page.context().newPage(); await second.goto(page.url()); await status(second, "ready");
  for (const tab of [page, second]) {
    await review(tab, backup); await status(tab, "backup_preview_ready");
    await control(tab, "configure", {pause: "before-import-save"});
    await tab.locator("#backup-approve").click(); await paused(tab, "before-import-save");
  }
  await Promise.all([control(page, "release", "before-import-save"), control(second, "release", "before-import-save")]);
  for (const tab of [page, second]) await tab.waitForFunction(() => ["backup_restored_locked", "workspace_conflict"].includes(document.querySelector("#status").textContent));
  check((await page.locator("#status").textContent() === "backup_restored_locked") !== (await second.locator("#status").textContent() === "backup_restored_locked"), "G4 exactly one competing restore commits");
  for (const tab of [page, second]) {
    check((await control(tab, "state")).locked && await tab.locator("#sink").textContent() === "", "G4 competing imports grant no unlocked session or handoff approval");
  }
  await unlock(page); await restored(page, output, "restored"); await close();
  console.log("PASS G4 restore approval, cancellation, atomic abort/lost acknowledgment and competing imports");
  await verifyCurrentFieldBackups({launch,close,unlock,status,check,fixture:backupFields});
  console.log("PASS B2 current snapshot fields, both credentials, fresh keys/head, reserved merge slot and authenticated missing-field rejection");

}
