import {readFile} from "node:fs/promises";

const password="synthetic-test-password-only";
async function policy(page) {return page.evaluate(async()=> (await import("./ui.js")).workspacePolicy());}
async function recall(page,text) {return page.evaluate(async text=> (await import("./ui.js")).workspaceRecall(text),text);}
async function stage(page,status,value,disposition="not_sensitive",kind="choice",type="PERSON") {
  await page.locator("#policy-kind").selectOption(kind);
  await page.locator("#policy-type").fill(type);
  await page.locator("#policy-value").fill(value);
  await page.locator(kind === "rule" ? "#policy-rule" : "#policy-choice").selectOption(disposition);
  await page.locator("#policy-prepare").click();await status(page,"policy_preview_ready");
}
async function mark(page,status,text) {
  await page.locator("#source").fill(text);await page.locator("#entity-type").fill("PERSON");
  await page.locator("#source").evaluate(element=>element.setSelectionRange(0,element.value.length));
  await page.locator("#mark").click();await status(page,"selection_added");
  await page.locator("#prepare").click();await status(page,"preview_ready");
  return page.locator("#preview").textContent();
}
export async function verifyPolicyHost({fresh,status,close,launch,unlock,restored,control,check,counters}) {
  let page=await fresh("policy-upgrade");
  const legacyRecoveryCode=await page.locator("#recovery-created").inputValue();
  await page.evaluate(async ({fixture,password})=>{
    const {readRecords,saveRecord}=await import("./storage.js");
    const {openVault,encryptCandidate}=await import("./vault.js");
    const {vault,workspace:record}=await readRecords();
    const opened=await openVault(password,vault,record);
    const legacy=Uint8Array.from(fixture.bundleHex.match(/../g),value=>parseInt(value,16));
    legacy.set(record.workspaceId,5);
    try {
      const head={revision:fixture.revision,commitId:Array(16).fill(3)};
      const encrypted=await encryptCandidate(opened.dataKey,record,head,legacy);
      await saveRecord({vault,record:encrypted,expectedHead:record.head,expectedGeneration:vault.accessGeneration,authorized:()=>true});
    } finally {opened.bundle.fill(0);legacy.fill(0);}
  },{fixture:counters.high,password});
  await page.locator("#lock").click();await status(page,"locked");await unlock(page);
  check((await policy(page)).snapshotVersion === 1,"D7 legacy encrypted vault opens without upgrade");
  await mark(page,status,"Jane");
  check(await page.locator("#format-upgrade").isHidden(),"D7 legacy alias/no-op preparation needs no upgrade warning");
  await page.locator("#cancel").click();await status(page,"cancelled");
  await page.locator("#backup-export-password").fill(password);
  const legacyDownload=page.waitForEvent("download");await page.locator("#backup-export").click();
  const legacyBackup=await readFile(await (await legacyDownload).path());await status(page,"backup_exported");
  await stage(page,status,"Jane");
  check(await page.locator("#format-upgrade").isVisible() && (await page.locator("#format-upgrade").textContent()).includes("0.5.0-beta.1"),"D7 old-reader warning appears BEFORE upgrade approval/save");
  await page.locator("#cancel").click();await status(page,"cancelled");
  check((await policy(page)).snapshotVersion === 1,"D7 discarded candidate does not upgrade encrypted vault");
  await stage(page,status,"Jane");
  await control(page,"configure",{abortSave:true});await page.locator("#approve").click();await status(page,"storage_failure");
  check((await policy(page)).snapshotVersion === 1,"D7 failed real save does not upgrade");
  await control(page,"configure",{abortSave:false});await page.locator("#approve").click();await status(page,"saved");
  check((await policy(page)).snapshotVersion === 3,"D7 approved real save upgrades");
  await close();page=await launch("policy-upgrade");await unlock(page);
  check((await policy(page)).snapshotVersion === 3,"D7 upgraded encrypted vault reopens");
  await restored(page,counters.high.marker,"restored");check(await page.locator("#restored").textContent() === "Jane","D7 upgrade keeps legacy aliases");
  await close();
  for(const [method,credential] of [["password",password],["recovery",legacyRecoveryCode]]) {
    page=await launch(`policy-legacy-backup-${method}`);
  await page.locator("#backup-file").setInputFiles({name:"legacy.dfbackup",mimeType:"application/octet-stream",buffer:legacyBackup});
  await page.locator("#backup-method").selectOption(method);
  await page.locator("#backup-credential").fill(credential);await page.locator("#backup-review").click();await status(page,"backup_preview_ready");
  check(await page.locator("#backup-format-upgrade").isVisible() && (await page.locator("#backup-format-upgrade").textContent()).includes("0.5.0-beta.1"),"D7 legacy recovery warns before its saved upgrade");
  await page.locator("#backup-approve").click();await status(page,"backup_restored_locked");
  await unlock(page);check((await policy(page)).snapshotVersion === 3,"D7 legacy recovery stages and saves proposed v3");
  await restored(page,counters.high.marker,"restored");check(await page.locator("#restored").textContent() === "Jane","D7 legacy backup upgrade preserves alias");
  await close();
  }
  page=await fresh("policy-source");
  const recoveryCode=await page.locator("#recovery-created").inputValue();
  const marker=await mark(page,status,"Jane Smith");
  await page.locator("#approve").click();await status(page,"saved");
  check((await recall(page,"JANE\u00a0SMITH")).findings.length === 1,"D6 baseline recall");
  await stage(page,status,"Jane Smith");
  check((await policy(page)).choices.length === 0,"D7 preview does not commit correction");
  const before=(await control(page,"state")).head;
  await control(page,"configure",{abortSave:true});
  await page.locator("#approve").click();await status(page,"storage_failure");
  check((await policy(page)).choices.length === 0 && JSON.stringify((await control(page,"state")).head) === JSON.stringify(before),"D7 failed IndexedDB save preserves committed policy/head");
  await control(page,"configure",{abortSave:false});
  await page.locator("#approve").click();await status(page,"saved");
  check(await page.locator("#sink").textContent() === "","D7 settings changes hand off no text");
  check((await policy(page)).choices[0].disposition === "not_sensitive" && (await recall(page,"JANE  SMITH")).findings.length === 0,"D6 saved correction affects Core recall");
  await restored(page,marker,"restored");check(await page.locator("#restored").textContent() === "Jane Smith","D7 permanent old alias still restores");
  await page.locator("#lock").click();await status(page,"locked");
  check(await page.locator("#policy-value").inputValue() === "","D7 lock clears local settings values");
  await unlock(page);check((await policy(page)).choices[0].value === "Jane Smith","D7 encrypted reopen preserves correction");
  check(await mark(page,status,"JANE  SMITH") === "JANE  SMITH","D6 legacy host prepare respects normalized correction");
  await page.locator("#cancel").click();await status(page,"cancelled");
  await stage(page,status,"", "keep","rule","PRODUCT_CODE");
  await page.locator("#approve").click();await status(page,"saved");
  await page.locator("#backup-export-password").fill(password);
  const downloading=page.waitForEvent("download");await page.locator("#backup-export").click();
  const download=await downloading;await status(page,"backup_exported");
  const backup=await readFile(await download.path());
  check(!["Jane Smith","not_sensitive",password,recoveryCode].some(value=>backup.includes(Buffer.from(value))),"D7 encrypted backup carries no plaintext policy values/credentials");
  await close();
  for(const [method,credential] of [["password",password],["recovery",recoveryCode]]) {
    page=await launch(`policy-restored-${method}`);
    await page.locator("#backup-file").setInputFiles({name:"policy.dfbackup",mimeType:"application/octet-stream",buffer:backup});
    await page.locator("#backup-method").selectOption(method);await page.locator("#backup-credential").fill(credential);
    await page.locator("#backup-review").click();await status(page,"backup_preview_ready");
    await page.locator("#backup-approve").click();await status(page,"backup_restored_locked");
    await close();page=await launch(`policy-restored-${method}`);await unlock(page);
    const settings=await policy(page);
    check(settings.choices[0].disposition === "not_sensitive" && settings.rules.some(row=>row.entityType === "PRODUCT_CODE" && row.rule === "keep"),"D7 all settings survive encrypted backup/recovery/restart");
    check((await recall(page,"Jane Smith")).findings.length === 0,"D6 recovery recall uses persisted correction");
    await restored(page,marker,"restored");check(await page.locator("#restored").textContent() === "Jane Smith","D7 backup preserves old alias");
    await stage(page,status,"JANE SMITH","remove");await page.locator("#approve").click();await status(page,"saved");
    check((await recall(page,"Jane Smith")).findings.length === 1,"D7 removal restores recall after encrypted recovery");
    check(await mark(page,status,"Jane Smith") === marker,"D7 removal reuses permanent alias");
    await page.locator("#cancel").click();await status(page,"cancelled");
    await close();
  }
  console.log("PASS D6/D7 explicit correction, real failed save, encrypted reopen, password/recovery backup and permanent aliases");
}
