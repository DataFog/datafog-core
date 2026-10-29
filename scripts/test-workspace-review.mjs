/** Real MV3 edit/review/approval/encrypted-save proof; synthetic values only. */
export async function verifyReviewHost({fresh,status,close,launch,unlock,control,check}) {
  let page=await fresh("combined-review");
  const before=(await control(page,"state")).head;
  const source=page.locator("#source");
  await source.fill("abcdef");await page.locator("#entity-type").fill("PERSON");
  for(const range of [[0,4],[2,6]]) {
    await source.evaluate((element,[start,end])=>element.setSelectionRange(start,end),range);
    await page.locator("#mark").click();await status(page,"selection_added");
  }
  const snapshot=await page.evaluate(async()=>{
    const {workspaceReview}=await import("./ui.js");
    return workspaceReview({text:"abcdef",scannerFindings:[{entityType:"PERSON",matchedText:"abcd",range:{unit:"utf8_bytes",start:0,end:4}}],userSelections:[{entityType:"CUSTOM",matchedText:"cdef",range:{unit:"utf8_bytes",start:2,end:6}}]});
  });
  check(snapshot.conflicts.length>0 && !Object.hasOwn(snapshot,"text"),"D8 readonly worker review returns conflicts without handoff");
  await page.locator("#prepare").click();await status(page,"review_conflict");
  check(await page.locator("#approve").isDisabled() && await page.locator("#preview").textContent()==="" && await page.locator("#sink").textContent()==="","D8 overlap has no usable approval/preview/handoff");
  check(await page.locator("#source-issues").isVisible() && await page.locator("#source-issue-preview mark").count()>0,"D8 original ranged conflicts survive sanitized worker RPC and highlight source");
  check(JSON.stringify((await control(page,"state")).head)===JSON.stringify(before),"D8 conflict does not mutate durable head");
  await source.fill("abcdef corrected");
  check(await page.locator("#approve").isDisabled() && await page.locator("#source-issues").isHidden(),"D8 source edit clears conflicting review");
  await source.evaluate(element=>element.setSelectionRange(0,6));await page.locator("#mark").click();await status(page,"selection_added");
  await page.locator("#prepare").click();await status(page,"preview_ready");
  const preview=await page.locator("#preview").textContent();
  check(preview.startsWith("[[PERSON_") && preview.endsWith(" corrected") && !preview.includes("abcdef"),"D8 fresh review preserves complete protected union");
  await control(page,"configure",{abortSave:true});await page.locator("#approve").click();await status(page,"storage_failure");
  check(await page.locator("#sink").textContent()==="" && JSON.stringify((await control(page,"state")).head)===JSON.stringify(before),"D8 failed encrypted save cannot activate or hand off");
  await control(page,"configure",{abortSave:false});await page.locator("#approve").click();await status(page,"saved");
  check(await page.locator("#sink").textContent()===preview,"D8 approved saved review hands off exact preview");
  await close();page=await launch("combined-review");await unlock(page);
  await page.locator("#source").fill("ABCDEF");await page.locator("#prepare").click();await status(page,"preview_ready");
  const recalled=await page.locator("#preview").textContent();
  check(recalled===preview.replace(" corrected","") && await page.locator("#sink").textContent()==="","D8 reviewed host automatically recalls after encrypted reopen but requires approval");
  await page.locator("#approve").click();await status(page,"saved");
  check(await page.locator("#sink").textContent()===recalled,"D8 follow-up recall still goes through save/approval");
  await close();
}
