import {readFile} from "node:fs/promises";
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
const sha=value=>createHash("sha256").update(value).digest("hex");
/** Actual packed-SDK/IndexedDB boundary proof; records and credentials are synthetic. */
export async function verifyOperationCapacity({launch,close,status,check}) {
  const manifest=JSON.parse(await readFile(new URL("../target/operation-capacity/manifest.json",import.meta.url),"utf8"));
  for(const fixture of Object.values(manifest.fixtures)) {
    const bytes=await readFile(new URL(`../target/operation-capacity/${fixture.file}`,import.meta.url));
    check(sha(bytes)===fixture.sha256,"capacity fixture digest matches generated receipt");
  }
  const password="synthetic-capacity-password-only";
  const result={scope:"approved count/reserve boundary only; native operation APIs and final conversation format remain separate",passed:false,
    fixtureManifest:manifest,phases:[],memoryObservations:[]};
  let page;
  async function sample(label) {
    const session=await page.context().browser().newBrowserCDPSession();
    try {
      const {processInfo}=await session.send("SystemInfo.getProcessInfo");
      const pids=processInfo.map(p=>p.id).filter(p=>Number.isSafeInteger(p)&&p>0);
      const rss=execFileSync("ps",["-o","rss=","-p",pids.join(",")],{encoding:"utf8"}).trim().split(/\s+/).reduce((sum,value)=>sum+Number(value)*1024,0);
      const renderer=await page.context().newCDPSession(page);
      try {const heap=await renderer.send("Runtime.getHeapUsage");result.memoryObservations.push({label,processRssSumBytes:rss,jsHeapUsedBytes:heap.usedSize,
        note:"Point sample after full history inspection; RSS sums isolated browser processes and may double-count shared pages. It is not peak physical heap or a lower-hardware SLA."});}
      finally {await renderer.detach();}
    } finally {await session.detach();}
  }
  async function seed(name) {
    page=await launch(`capacity-${name}`);
    const created=await page.evaluate(async({name,password})=>{
      const api=await import("./vendor/package/index.js");await api.init();
      const {createVault}=await import("./vault.js"),{saveRecord}=await import("./storage.js");
      const bytes=new Uint8Array(await fetch(`./capacity-fixtures/${name}.cbor`).then(r=>r.arrayBuffer()));
      const workspace=api.Workspace.openBundle(bytes);bytes.fill(0);
      try {
        const created=await createVault(password,workspace.namespace(),workspace.exportBundle(),workspace.head());
        await saveRecord({...created,creating:true,authorized:()=>true});return {recoveryCode:created.recoveryCode};
      } finally {workspace.close();}
    },{name,password});
    await page.reload();await status(page,"ready");
    await page.locator("#password").fill(password);await page.locator("#unlock").click();await status(page,"unlocked");
    return created.recoveryCode;
  }
  async function inspect(expectedPrefix,credential=password,method="password") {
    return page.evaluate(async({expectedPrefix,credential,method,marker})=>{
      const api=await import("./vendor/package/index.js");await api.init();
      const {readRecords}=await import("./storage.js"),{openVault}=await import("./vault.js");
      const saved=await readRecords(),opened=await openVault(credential,saved.vault,saved.workspace,undefined,method);
      try {
        const workspace=api.Workspace.openBundle(opened.bundle);
        try {
          const operations=workspace.operations();
          const prefix=new TextEncoder().encode(JSON.stringify(operations.slice(0,expectedPrefix)));
          let prefixSha;try {prefixSha=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",prefix)));}finally{prefix.fill(0);}
          const exported=workspace.exportBundle();let bundleSha;
          try {bundleSha=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",exported)));}finally{exported.fill(0);}
          const unchanged=workspace.prepare("ordinary",[],crypto.getRandomValues(new Uint8Array(16)));
          const noOp=JSON.stringify(unchanged.heads().expectedHead)===JSON.stringify(unchanged.heads().candidateHead);unchanged.discard();
          return {counts:workspace.counts(),coverage:workspace.historyCoverage(),head:workspace.head(),prefixSha,bundleSha,
            first:operations[0].id,last:operations.at(-1).id,lastKind:operations.at(-1).kind,
            restored:workspace.restore(marker),noOp,profile:saved.vault.profile};
        } finally {workspace.close();}
      } finally {opened.bundle.fill(0);}
    },{expectedPrefix,credential,method,marker:manifest.marker});
  }
  async function blockedWrite(before,expectedPrefix,credential=password,method="password") {
    await page.locator("#source").fill("Kate");await page.locator("#entity-type").fill("PERSON");
    await page.locator("#source").evaluate(element=>element.setSelectionRange(0,4));
    await page.locator("#mark").click();await status(page,"selection_added");
    await page.locator("#prepare").click();await status(page,"resource_limit");
    check(await page.locator("#sink").textContent()==="","history exhaustion releases no text");
    const after=await inspect(expectedPrefix,credential,method);
    check(JSON.stringify(before.bundleSha)===JSON.stringify(after.bundleSha),"failed ordinary write preserves every saved byte");
    return after;
  }
  async function download() {
    const downloading=page.waitForEvent("download");
    await page.locator("#backup-export-password").fill(password);await page.locator("#backup-export").click();
    await status(page,"backup_exported");const file=await downloading;return readFile(await file.path());
  }
  async function importBackup(label,bytes,method,credential) {
    page=await launch(label);await page.locator("#backup-file").setInputFiles({name:"capacity.dfbackup",mimeType:"application/octet-stream",buffer:bytes});
    await page.locator("#backup-method").selectOption(method);await page.locator("#backup-credential").fill(credential);
    await page.locator("#backup-review").click();
  }
  for(const name of ["ordinary","last-recovery","hard"]) {
    const fixture=manifest.fixtures[name],recoveryCode=await seed(name);
    const original=await inspect(fixture.records);
    check(original.counts.operations===fixture.records&&original.restored==="Jane"&&original.noOp,"capacity fixture opens read-only with aliases/no-ops intact");
    check(original.coverage.recordedHistoryOnly&&original.coverage.preUpgradeHistory==="not_applicable","full retained history has explicit recorded-only coverage");
    check(JSON.stringify(original.first)===JSON.stringify(fixture.firstId)&&JSON.stringify(original.last)===JSON.stringify(fixture.lastId),"all boundary fixture endpoints retained");
    await blockedWrite(original,fixture.records);
    const backup=await download();const afterExport=await inspect(fixture.records);
    check(JSON.stringify(afterExport.bundleSha)===JSON.stringify(original.bundleSha),"export at exhaustion never writes activity/history");
    await sample(`${name}: full history view plus blocked write/export`);
    result.phases.push({name,records:fixture.records,ordinaryWrite:"resource_limit",readRestoreExport:true,noOp:original.noOp});
    await close();
    if(name==="hard") {
      await importBackup("capacity-hard-import",backup,"password",password);await status(page,"resource_limit");
      check(await page.locator("#backup-approve").isDisabled()&&await page.locator("#sink").textContent()==="","true hard ceiling rejects fresh-head recovery without release");
      check(await page.evaluate(async()=>!(await(await import("./storage.js")).readRecords()).vault),"failed exhausted recovery writes no destination vault");
      await close();backup.fill(0);continue;
    }
    for(const method of ["password","recovery"]) {
      const credential=method==="password"?password:recoveryCode,label=`capacity-${name}-${method}`;
      await importBackup(label,backup,method,credential);await status(page,"backup_preview_ready");
      await page.locator("#backup-approve").click();await status(page,"backup_restored_locked");await close();
      page=await launch(label);
      const restored=await inspect(fixture.records,credential,method);
      check(restored.counts.operations===fixture.records+1&&restored.profile===2&&restored.restored==="Jane","both strong backup credentials consume exactly one remaining recovery slot");
      check(restored.counts.preparedInputs===original.counts.preparedInputs&&restored.counts.transformedSpans===original.counts.transformedSpans,"recovery invents no input/replacement totals");
      check(JSON.stringify(restored.prefixSha)===JSON.stringify(original.prefixSha),"every old record survives recovery unchanged, not just the endpoints");
      check(restored.lastKind==="recovery"&&JSON.stringify(restored.head.commitId)!==JSON.stringify(original.head.commitId),"recovery has a fresh saved head and its real content-free row");
      await page.locator("#unlock-method").selectOption(method);await page.locator("#password").fill(credential);await page.locator("#unlock").click();await status(page,"unlocked");
      await blockedWrite(restored,fixture.records,credential,method);
      await sample(`${name}: ${method} recovered restart`);
      result.phases.push({name:`${name}-${method}`,records:restored.counts.operations,retainedPrefix:true,profile:2,reopened:true});
      await close();
    }
    backup.fill(0);
  }
  result.passed=true;console.log("PASS 99,744 ordinary /100,000 hard history, finite recovery slots, full-prefix retention, encrypted read/export and both backup credentials");
  return result;
}
