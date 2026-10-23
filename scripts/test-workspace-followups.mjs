/** Real browser approval and persistence checks for supplied follow-up findings. */
async function markQuestion(page, text, selected, status) {
  await page.locator("#source").fill(text); await page.locator("#entity-type").fill("PERSON");
  await page.locator("#source").evaluate((element, selected) => {
    const start = element.value.indexOf(selected); if (start < 0) throw new Error("synthetic follow-up selection missing");
    element.setSelectionRange(start, start + selected.length);
  }, selected);
  await page.locator("#mark").click(); await status(page, "selection_added");
}

export async function verifyFollowups({fresh, prepare, status, contextClose, launch, unlock, restored, control, check, foreignMarker}) {
  let page = await fresh("followups");
  const upload = await prepare(page), marker = upload.match(/\[\[DFP1:.*?\]\]/)[0];
  await page.locator("#approve").click(); await status(page, "saved");
  await contextClose(); page = await launch("followups");
  await page.locator("#source").fill(`Totals for ${marker}?`);
  await page.locator("#prepare").click(); await status(page, "locked");
  check(await page.locator("#approve").isDisabled() && await page.locator("#sink").textContent() === "", "F1 locked follow-ups release nothing");
  await unlock(page);
  check(await page.locator("#source").inputValue() === "", "F1 unlock requires reentering and reviewing the question");
  const question = `👋 é Compare ${marker} with John Smith, then ${marker}.`;
  await markQuestion(page, question, "John Smith", status);
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const preview = await page.locator("#preview").textContent(), markers = preview.match(/\[\[DFP1:.*?\]\]/g);
  const freshMarker = markers[1];
  check(markers.length === 3 && markers[0] === marker && markers[2] === marker && freshMarker.endsWith(":PERSON:2]]"), "F1 old aliases pass through and the new selected name gets one alias");
  check(preview === question.replace("John Smith", freshMarker) && !preview.includes("John Smith") && !preview.includes("Jane Smith"), "F1 Unicode question preserves markers and surrounding text without local restoration");
  check(await page.locator("#sink").textContent() === "" && (await control(page, "state")).head.revision === "1", "F1 preview neither releases nor commits the new alias");
  await restored(page, freshMarker, "invalid_marker");
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === preview && await page.locator("#source").inputValue() === "", "F1 approval releases exactly the reviewed question and clears its draft");
  await contextClose(); page = await launch("followups"); await unlock(page);
  await restored(page, preview, "restored");
  check(await page.locator("#restored").textContent() === question.replaceAll(marker, "Jane Smith"), "F1 old and new aliases restore after another browser restart");
  const aliasOnly = `Totals for ${marker} and ${freshMarker}?`;
  await page.locator("#source").fill(aliasOnly); await page.locator("#prepare").click(); await status(page, "preview_ready");
  check(await page.locator("#preview").textContent() === aliasOnly && await page.locator("#sink").textContent() === "", "F1 alias-only questions still require preview and approval");
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === aliasOnly && (await control(page, "state")).head.revision === "2", "F1 alias-only approval leaves the mapping revision unchanged");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  await page.locator("#source").fill(aliasOnly); await page.locator("#prepare").click(); await status(page, "preview_ready");
  await page.locator("#source").fill(aliasOnly + " In 2026.");
  check(await page.locator("#approve").isDisabled() && await page.locator("#preview").textContent() === "" && await page.locator("#sink").textContent() === "", "F2 editing a question invalidates its approval without release");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  await markQuestion(page, `Compare ${marker} with Alice Smith.`, "Alice Smith", status);
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const pendingMarker = (await page.locator("#preview").textContent()).match(/\[\[DFP1:.*?\]\]/g)[1];
  await page.locator("#cancel").click(); await status(page, "cancelled");
  for (const bad of [marker.replace("DFP1", "DFP2"), marker.slice(0, -1), foreignMarker, pendingMarker]) {
    await markQuestion(page, `Compare ${marker} and ${bad} with Alice Smith.`, "Alice Smith", status);
    await page.locator("#prepare").click(); await status(page, "invalid_marker");
    check(await page.locator("#approve").isDisabled() && await page.locator("#preview").textContent() === "" && await page.locator("#sink").textContent() === "", "F2 invalid markers block the complete question");
    check((await control(page, "state")).head.revision === "2" && (await page.locator("#batch-error").textContent()).includes("full pseudonym"), "F2 rejected markers preserve mappings and give content-free correction guidance");
    await page.locator("#cancel").click(); await status(page, "cancelled");
  }
  await markQuestion(page, `Totals for ${marker}?`, marker, status);
  await page.locator("#prepare").click(); await status(page, "invalid_finding");
  check(await page.locator("#approve").isDisabled() && (await control(page, "state")).head.revision === "2", "F2 selecting an existing marker cannot rewrite it");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  await restored(page, pendingMarker, "invalid_marker");
  await restored(page, freshMarker, "restored");
  check(await page.locator("#restored").textContent() === "John Smith", "F2 cancelled and rejected questions leave committed aliases usable");
  console.log("PASS F1/F2 protected follow-ups, unchanged aliases, approval, rejection and restart");
}
