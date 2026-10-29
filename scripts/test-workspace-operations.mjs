import {readFile} from "node:fs/promises";
/** Packed-SDK MV3 persistence proof; every value and credential is synthetic. */
export async function verifyOperationPersistence({launch, close, status, check}) {
  const legacy=JSON.parse(await readFile(new URL("../fixtures/workspace/policy-v2.json",import.meta.url),"utf8"));
  let page=await launch("operation-policy-v2");
  const password="synthetic-operation-password-only";
  await page.locator("#password").fill(password);await page.locator("#create").click();await status(page,"unlocked");
  await page.evaluate(async({legacy,password})=>{
    const {init,Workspace}=await import("./vendor/package/index.js");await init();
    const {readRecords,saveRecord}=await import("./storage.js");
    const {openVault,encryptCandidate}=await import("./vault.js");
    const {vault,workspace:record}=await readRecords();const opened=await openVault(password,vault,record);
    const bytes=Uint8Array.from(legacy.bundleHex.match(/../g),value=>parseInt(value,16));bytes.set(record.workspaceId,5);
    const old=Workspace.openBundle(bytes);
    try {
      const candidate=await encryptCandidate(opened.dataKey,record,old.head(),bytes);
      await saveRecord({vault,record:candidate,expectedHead:record.head,expectedGeneration:vault.accessGeneration,authorized:()=>true});
    } finally {old.close();bytes.fill(0);opened.bundle.fill(0);}
  },{legacy,password});
  await page.locator("#lock").click();await status(page,"locked");
  await page.locator("#password").fill(password);await page.locator("#unlock").click();await status(page,"unlocked");
  const version=()=>page.evaluate(async()=>(await import("./ui.js")).workspacePolicy().then(value=>value.snapshotVersion));
  check(await version()===2,"operation proposal opens encrypted policy-v2 without a read upgrade");
  async function stageUpgrade(){
    await page.locator("#source").fill("New Synthetic Name");await page.locator("#entity-type").fill("PERSON");
    await page.locator("#source").evaluate(element=>element.setSelectionRange(0,element.value.length));
    await page.locator("#mark").click();await status(page,"selection_added");
    await page.locator("#prepare").click();await status(page,"preview_ready");
    check(await page.locator("#format-upgrade").isVisible()&&(await page.locator("#format-upgrade").textContent()).includes("0.5.0-beta.2"),"v2-to-v3 incompatibility notice is visible BEFORE save");
    check(await version()===2,"v3 preview does not mutate committed v2 format");
  }
  await stageUpgrade();await page.locator("#cancel").click();await status(page,"cancelled");
  check(await version()===2,"discarded encrypted upgrade preserves policy-v2");
  await stageUpgrade();await page.locator("#approve").click();await status(page,"saved");
  check(await version()===3,"approved encrypted save activates proposed v3");
  const upgraded=await page.evaluate(async({password,marker})=>{
    const {init,Workspace}=await import("./vendor/package/index.js");await init();const {readRecords}=await import("./storage.js");const {openVault}=await import("./vault.js");
    const {vault,workspace:record}=await readRecords();const opened=await openVault(password,vault,record);const workspace=Workspace.openBundle(opened.bundle);
    try{return{restored:workspace.restore(marker),choices:workspace.policy().choices,counts:workspace.counts()};}
    finally{workspace.close();opened.bundle.fill(0);}
  },{password,marker:legacy.marker});
  check(upgraded.restored==="Jane"&&upgraded.choices[0].value==="May"&&upgraded.counts.operations===1,"v2-to-v3 preserves aliases/choices and does not fabricate prior history");
  await close();
  page = await launch("operation-records");
  const proof = await page.evaluate(async () => {
    const {init, Workspace} = await import("./vendor/package/index.js"); await init();
    const {randomId, createVault, openVault, encryptCandidate} = await import("./vault.js");
    const {readRecords, saveRecord} = await import("./storage.js");
    const {exportBackup} = await import("./backup.js");
    const password = "synthetic-operation-password-only";
    const context = () => ({id: randomId(), occurredAtMs: Date.now()});
    const workspace = Workspace.createWithOperation(randomId(), context());
    const created = await createVault(password, workspace.namespace(), workspace.exportBundle(), workspace.head());
    await saveRecord({...created, creating: true, authorized: () => true});
    let record = created.record;
    const finding = {entityType: "PERSON", matchedText: "Jane", range: {unit: "utf8_bytes", start: 0, end: 4}};
    const change = workspace.prepareBatchWithOperation([{text: "Jane", findings: [finding]}, {text: "Jane", findings: [finding]}], context());
    const heads = change.heads(), preview = change.previews()[0].text;
    const candidate = await encryptCandidate(created.dataKey, record, heads.candidateHead, change.exportCandidateBundle());
    let aborted;
    try { await saveRecord({vault: created.vault, record: candidate, expectedHead: heads.expectedHead, expectedGeneration: created.vault.accessGeneration, authorized: () => true, abort: true}); }
    catch (error) { aborted = error.code; }
    if (aborted !== "storage_failure" || workspace.counts().operations !== 1 || (await readRecords()).workspace.head.revision !== "0") throw new Error("operation rollback");
    await saveRecord({vault: created.vault, record: candidate, expectedHead: heads.expectedHead, expectedGeneration: created.vault.accessGeneration, authorized: () => true});
    workspace.activateSaved(change, heads.expectedHead, heads.candidateHead); record = candidate;
    const cancelled = workspace.prepareWithOperation("ordinary", [], context());
    cancelled.exportCandidateBundle(); cancelled.discard();
    if (workspace.counts().operations !== 2 || workspace.counts().preparedInputs !== 2 || workspace.counts().transformedSpans !== 2) throw new Error("operation counts");
    const denied = workspace.prepareOutcome({kind: "blocked_input", issueCount: 2}, context());
    const deniedHeads = denied.heads();
    const deniedRecord = await encryptCandidate(created.dataKey, record, deniedHeads.candidateHead, denied.exportCandidateBundle());
    await saveRecord({vault: created.vault, record: deniedRecord, expectedHead: deniedHeads.expectedHead, expectedGeneration: created.vault.accessGeneration, authorized: () => true});
    workspace.activateSaved(denied, deniedHeads.expectedHead, deniedHeads.candidateHead); record = deniedRecord;
    const counts = workspace.counts(), operations = workspace.operations();
    if (JSON.stringify({counts, operations}).includes("Jane") || counts.reportedBlockedAttempts !== 1 || counts.confirmedHandoffs !== 0) throw new Error("content-free records");
    workspace.close();
    const opened = await openVault(password, created.vault, record);
    try {
      const reopened = Workspace.openBundle(opened.bundle);
      try {
        if (JSON.stringify(reopened.counts()) !== JSON.stringify(counts) || reopened.restore(preview) !== "Jane") throw new Error("encrypted reopen");
      } finally { reopened.close(); }
    } finally { opened.bundle.fill(0); }
    const backup = await exportBackup(created.vault, record, created.dataKey,password);
    try { return {backup: Array.from(backup), counts, preview}; }
    finally { backup.fill(0); }
  });
  await close(); page = await launch("operation-records-recovery");
  await page.locator("#backup-file").setInputFiles({name: "operations.dfbackup", mimeType: "application/octet-stream", buffer: Buffer.from(proof.backup)});
  await page.locator("#backup-credential").fill("synthetic-operation-password-only");
  await page.locator("#backup-review").click(); await status(page, "backup_preview_ready");
  await page.locator("#backup-approve").click(); await status(page, "backup_restored_locked");
  await close(); page = await launch("operation-records-recovery");
  const recovered = await page.evaluate(async preview => {
    const {init, Workspace} = await import("./vendor/package/index.js"); await init();
    const {readRecords} = await import("./storage.js"); const {openVault} = await import("./vault.js");
    const saved = await readRecords(), opened = await openVault("synthetic-operation-password-only", saved.vault, saved.workspace);
    try {
      const workspace = Workspace.openBundle(opened.bundle);
      try { return {counts: workspace.counts(), record: workspace.operations().at(-1), restored: workspace.restore(preview)}; }
      finally { workspace.close(); }
    } finally { opened.bundle.fill(0); }
  }, proof.preview);
  check(recovered.counts.operations === proof.counts.operations + 1 && recovered.counts.preparedInputs === 2 && recovered.counts.transformedSpans === 2 && recovered.counts.reportedBlockedAttempts === 1, "operation/count keys survive encrypted backup approval and process restart");
  check(recovered.record.kind === "recovery" && recovered.record.occurredAtMs === null && recovered.restored === "Jane", "legacy recovery time stays unknown and markers preserve their meaning");
  await close();
  console.log("PASS content-free operations/counts, real IndexedDB rollback, retry/discard, encrypted reopen and backup recovery");
}
