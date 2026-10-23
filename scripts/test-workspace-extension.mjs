import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const harness = path.join(root, "examples/workspace-extension");
const { chromium } = createRequire(path.join(harness, "package.json"))("playwright");
const fixture = JSON.parse(await readFile(path.join(root, "fixtures/workspace/monthly.json"), "utf8"));
const counterFixtures = JSON.parse(await readFile(path.join(root, "fixtures/workspace/counters.json"), "utf8"));
const kdfFixture = JSON.parse(await readFile(path.join(root, "fixtures/workspace/argon2id.json"), "utf8"));
execFileSync("npm", ["run", "build", "--prefix", harness], { stdio: "inherit" });
const temporary = await mkdtemp(path.join(os.tmpdir(), "datafog-workspace-"));
let context;
let origin;
const password = "synthetic-test-password-only";
const observed = { externalRequests: 0, consoleMessages: 0, pageErrors: 0 };
function check(condition, label) { if (!condition) throw new Error(label); }
async function launch(profile = "happy") {
  context = await chromium.launchPersistentContext(path.join(temporary, profile), {
    channel: "chromium", headless: process.env.DATAFOG_HEADED !== "1",
    args: ["--enable-automation", `--disable-extensions-except=${path.join(harness, "dist")}`, `--load-extension=${path.join(harness, "dist")}`],
  });
  context.on("request", request => { if (/^https?:/i.test(request.url())) observed.externalRequests++; });
  context.on("console", () => { observed.consoleMessages++; });
  context.on("page", page => page.on("pageerror", () => { observed.pageErrors++; }));
  let worker = context.serviceWorkers()[0];
  if (!worker) worker = await context.waitForEvent("serviceworker");
  origin = `chrome-extension://${new URL(worker.url()).hostname}`;
  const page = await context.newPage();
  await page.goto(`${origin}/index.html`);
  await page.locator("#status").filter({ hasText: "ready" }).waitFor();
  return page;
}
async function status(page, text) {
  await page.waitForFunction(expected => document.querySelector("#status").textContent === expected, text);
}
async function create(page) {
  await page.locator("#password").fill(password); await page.locator("#create").click(); await status(page, "unlocked");
}
async function unlock(page) {
  await page.locator("#password").fill(password); await page.locator("#unlock").click(); await status(page, "unlocked");
}
async function fresh(profile) {
  await context?.close(); context = undefined;
  const page = await launch(profile); await create(page); return page;
}
async function control(page, command, value) {
  return page.evaluate(async args => (await import("./ui.js")).testCommand(...args), [command, value]);
}
async function paused(page, name) {
  await page.waitForFunction(async name => (await (await import("./ui.js")).testCommand("state")).paused.includes(name), name);
}
async function restored(page, text, expected) {
  await page.locator("#restore-input").fill(text); await page.locator("#restore").click(); await status(page, expected);
}
async function noRelease(page, label) { check(await page.locator("#sink").textContent() === "", label); }
async function prepare(page, source = fixture.january, type = fixture.type) {
  await page.locator("#source").fill(source);
  await page.locator("#entity-type").fill(type);
  const start = source.indexOf(fixture.name);
  await page.locator("#source").evaluate((element, range) => element.setSelectionRange(range.start, range.end), { start, end: start + fixture.name.length });
  await page.locator("#mark").click(); await status(page, "selection_added");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  return page.locator("#preview").textContent();
}

try {
  let page = await launch();
  await create(page);
  const january = await prepare(page);
  check(january.startsWith("2026-01-06 | [[DFP1:") && january.endsWith(" | -40.00 USD"), "A1 preview preserves non-selected text");
  check(await page.locator("#sink").textContent() === "", "A2 preview does not release");
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === january, "A1 approved sink matches preview");
  await context.close(); context = undefined;
  page = await launch();
  await page.locator("#restore-input").fill(january);
  await page.locator("#restore").click(); await status(page, "locked");
  await page.locator("#password").fill(password); await page.locator("#unlock").click(); await status(page, "unlocked");
  const february = await prepare(page, fixture.february);
  const marker = january.match(/\[\[DFP1:.*?\]\]/)?.[0];
  check(Boolean(marker) && february.includes(marker), "A1 marker is byte-identical after process restart");
  await page.locator("#approve").click(); await status(page, "saved");
  await page.locator("#restore-input").fill(january);
  await page.locator("#restore").click(); await status(page, "restored");
  check(await page.locator("#restored").textContent() === fixture.january, "A1 exact restoration");
  console.log("PASS A1 encrypted save, browser restart, alias reuse and restoration");

  page = await fresh("batch");
  await prepare(page);
  await page.locator("#add-input").click(); await status(page, "input_added");
  await prepare(page, fixture.february);
  await page.locator("#add-input").click(); await status(page, "input_added");
  await page.locator("#source").fill("[[DFP1:literal source collision");
  await page.locator("#add-input").click(); await status(page, "input_added");
  await page.locator("#prepare").click(); await status(page, "invalid_marker");
  check((await page.locator("#batch-error").textContent()).includes("Input 3"), "B1 rejected input is identified without source text");
  check((await control(page, "state")).head.revision === "0", "B1 rejected batch saves no mappings");
  await noRelease(page, "B1 rejected batch releases nothing");
  check(await page.locator("#approve").isDisabled(), "B1 rejection leaves no approval");
  await page.getByRole("button", {name: "Remove input 3", exact: true}).click(); await status(page, "input_removed");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const batchPreview = await page.locator("#preview").textContent();
  const batchMarkers = batchPreview.match(/\[\[DFP1:.*?\]\]/g);
  check(batchMarkers?.length === 2 && batchMarkers[0] === batchMarkers[1], "B1 inputs share one exact pseudonym");
  await page.getByRole("button", {name: "Remove input 2", exact: true}).click(); await status(page, "input_removed");
  check(await page.locator("#approve").isDisabled() && await page.locator("#preview").textContent() === "", "B2 membership edits invalidate preview and approval");
  await noRelease(page, "B2 membership edits do not release an old plan");
  await prepare(page, fixture.february);
  const approvedBatch = await page.locator("#preview").textContent();
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === approvedBatch, "B1 only the entire approved batch is released");
  check((await control(page, "state")).head.revision === "1", "B1 batch has one committed head");
  check(await page.locator("#source").inputValue() === "" && await page.locator("#inputs").textContent() === "", "B3 completed batch drops temporary inputs");
  await context.close(); context = undefined;
  page = await launch("batch"); await unlock(page);
  const nextMonth = await prepare(page);
  const approvedMarker = approvedBatch.match(/\[\[DFP1:.*?\]\]/)[0];
  check(nextMonth.includes(approvedMarker), "B1 restart preserves batch aliases");
  await restored(page, approvedMarker, "restored");
  check(await page.locator("#restored").textContent() === fixture.name, "B1 batch alias restores after restart");
  await page.locator("#add-input").click(); await status(page, "input_added");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  check(await page.locator("#inputs").textContent() === "", "B3 cancelled batch drops temporary inputs");
  console.log("PASS B1–B3 atomic batch, explicit removal/reapproval and temporary input cleanup");

  page = await fresh("approval");
  await prepare(page);
  await page.locator("#cancel").click(); await status(page, "cancelled");
  check((await control(page, "state")).head.revision === "0", "A2 cancelled preview does not commit");
  await noRelease(page, "A2 cancelled preview does not release");
  await prepare(page);
  await page.locator("#source").fill(fixture.january + " ");
  check(await page.locator("#approve").isDisabled(), "A2 edit invalidates approval control");
  check(await page.locator("#preview").textContent() === "", "A2 edit clears old preview");
  check(!(await control(page, "state")).pending, "A2 edit discards pending plan");
  await noRelease(page, "A2 edited preview does not release");
  console.log("PASS A2 preview cancellation and edit invalidate approval");

  page = await fresh("storage-abort");
  const aborted = await prepare(page);
  await control(page, "configure", { abortSave: true });
  await page.locator("#approve").click(); await status(page, "storage_failure");
  check((await control(page, "state")).head.revision === "0", "A4 aborted save preserves old head");
  await noRelease(page, "A4 aborted save never releases");
  await page.locator("#lock").click(); await status(page, "locked"); await unlock(page);
  await restored(page, aborted, "invalid_marker");
  check(await page.locator("#restored").textContent() === "", "A4 aborted mappings are absent after reopen");
  await control(page, "configure", { abortSave: false });
  await prepare(page); await page.locator("#approve").click(); await status(page, "saved");
  console.log("PASS A4 definite IndexedDB abort preserves saved state and permits explicit retry");

  page = await fresh("uncertain-save");
  const uncertain = await prepare(page);
  await control(page, "configure", { loseAck: true });
  await page.locator("#approve").click(); await status(page, "save_uncertain");
  check((await control(page, "state")).head.revision === "1", "A4 uncertain committed head is resolved");
  await noRelease(page, "A4 uncertain save never releases");
  check(await page.locator("#approve").isDisabled(), "A4 uncertain outcome requires new review");
  await control(page, "configure", { loseAck: false });
  check(await prepare(page) === uncertain, "A4 fresh review preserves committed marker");
  await page.locator("#approve").click(); await status(page, "saved");
  console.log("PASS A4 lost acknowledgment preserves mappings without automatic release");

  page = await fresh("commit-interruption");
  const interrupted = await prepare(page);
  await control(page, "configure", { pause: "after-save" });
  await page.locator("#approve").click(); await paused(page, "after-save");
  await noRelease(page, "A4 interruption checkpoint precedes release");
  const isolatedBrowser = context.browser();
  const cdp = await isolatedBrowser.newBrowserCDPSession();
  const commandLine = await cdp.send("Browser.getBrowserCommandLine");
  check(commandLine.arguments.includes(`--user-data-dir=${path.join(temporary, "commit-interruption")}`), "A4 kill is restricted to the isolated test profile");
  const info = await cdp.send("SystemInfo.getProcessInfo");
  const pid = info.processInfo.find(process => process.type === "browser")?.id;
  check(Number.isSafeInteger(pid) && pid > 1, "A4 isolated browser process identity");
  const closed = new Promise(resolve => isolatedBrowser.once("disconnected", resolve));
  process.kill(pid, "SIGKILL");
  let timer;
  try { await Promise.race([closed, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("A4 isolated browser did not disconnect")), 15000); })]); }
  finally { clearTimeout(timer); }
  context = undefined;
  page = await launch("commit-interruption"); await unlock(page);
  await restored(page, interrupted, "restored");
  check(await page.locator("#restored").textContent() === fixture.january, "A4 restart loads committed candidate");
  await noRelease(page, "A4 restart does not resume delivery");
  console.log("PASS A4 actual browser termination between persistence and activation");

  page = await fresh("competing-writers");
  const second = await context.newPage(); await second.goto(`${origin}/index.html`);
  await second.locator("#status").filter({ hasText: "ready" }).waitFor(); await unlock(second);
  const one = await prepare(page), two = await prepare(second);
  await control(page, "configure", { pause: "before-save" }); await control(second, "configure", { pause: "before-save" });
  await page.locator("#approve").click(); await second.locator("#approve").click();
  await paused(page, "before-save"); await paused(second, "before-save");
  await Promise.all([control(page, "release", "before-save"), control(second, "release", "before-save")]);
  for (const tab of [page, second]) await tab.waitForFunction(() => ["saved", "workspace_conflict"].includes(document.querySelector("#status").textContent));
  const firstWon = await page.locator("#status").textContent() === "saved";
  check((await second.locator("#status").textContent() === "saved") !== firstWon, "A5 one complete-head writer wins");
  const winner = firstWon ? page : second, loser = firstWon ? second : page;
  const winningText = firstWon ? one : two;
  await noRelease(loser, "A5 stale writer does not release");
  check(await winner.locator("#sink").textContent() === winningText, "A5 winner releases only its saved candidate");
  await unlock(loser); await restored(loser, winningText, "restored");
  check(await loser.locator("#restored").textContent() === fixture.january, "A5 loser can reopen winning state");
  console.log("PASS A5 real competing transactions reject stale complete heads");

  for (const action of ["lock", "cancel"]) {
    for (const checkpoint of ["before-save", "after-save"]) {
      page = await fresh(`${action}-${checkpoint}`);
      const text = await prepare(page);
      await control(page, "configure", { pause: checkpoint });
      await page.locator("#approve").click(); await paused(page, checkpoint);
      await page.locator(`#${action}`).click(); await status(page, action === "lock" ? "locked" : "cancelled");
      await noRelease(page, "A5 invalidated async result never releases");
      if (action === "lock") await unlock(page);
      await restored(page, text, checkpoint === "after-save" ? "restored" : "invalid_marker");
      if (checkpoint === "after-save") check(await page.locator("#restored").textContent() === fixture.january, "A5 commit survives late invalidation");
      else check(await page.locator("#restored").textContent() === "", "A5 pre-save invalidation commits nothing");
    }
  }
  console.log("PASS A5 lock and cancel before/after commit block late release");

  page = await fresh("restoration-errors");
  const valid = await prepare(page); await page.locator("#approve").click(); await status(page, "saved");
  const full = valid.match(/\[\[DFP1:.*?\]\]/)[0];
  for (const bad of [full.replace("DFP1", "DFP2"), full.replace(":PERSON:", ":OTHER:"), "[[DFP1:damaged"]) {
    await restored(page, `${full} ${bad}`, "invalid_marker");
    check(await page.locator("#restored").textContent() === "", "A6 no partial restored text");
  }
  console.log("PASS A6 strict atomic local restoration");

  page = await fresh("envelope-validation");
  const encrypted = await prepare(page); await page.locator("#approve").click(); await status(page, "saved");
  await page.locator("#lock").click(); await status(page, "locked");
  await page.locator("#password").fill("incorrect-synthetic-password"); await page.locator("#unlock").click(); await status(page, "invalid_credentials");
  await noRelease(page, "A8 incorrect credentials do not release");
  const tampering = await page.evaluate(async password => {
    const { readRecords, DATABASE, workspaceKey } = await import("./storage.js");
    const baseline = await readRecords();
    function privateLeaf(value) {
      if (value instanceof CryptoKey) return true;
      if (typeof value === "string") return value.includes(password) || value.includes("Jane Smith");
      if (value instanceof Uint8Array || value === null || typeof value !== "object") return false;
      return Object.values(value).some(privateLeaf);
    }
    if (privateLeaf(baseline)) throw new Error("A8 plaintext credential or mapping in persistent records");
    async function replace(records) {
      const db = await new Promise((resolve, reject) => {
        const req = indexedDB.open(DATABASE, 1); req.onsuccess = () => resolve(req.result); req.onerror = () => reject(new Error("A8 storage setup"));
      });
      try {
        await new Promise((resolve, reject) => {
          const tx = db.transaction("records", "readwrite"); const store = tx.objectStore("records");
          store.clear(); store.put(records.vault, "vault"); store.put(records.workspace, workspaceKey(records.vault.workspaceId));
          tx.oncomplete = resolve; tx.onabort = () => reject(new Error("A8 storage setup"));
        });
      } finally { db.close(); }
    }
    async function attempt(expected) {
      await new Promise((resolve, reject) => {
        const status = document.querySelector("#status");
        const timer = setTimeout(() => { observer.disconnect(); reject(new Error("A8 unlock acknowledgment")); }, 15000);
        const observer = new MutationObserver(() => {
          clearTimeout(timer); observer.disconnect();
          if (expected.includes(status.textContent)) resolve(); else reject(new Error("A8 unlock category"));
        });
        observer.observe(status, {childList: true});
        document.querySelector("#password").value = password;
        document.querySelector("#unlock").click();
      });
    }
    for (const mutate of [
      records => { records.vault.profile = 255; },
      records => { records.vault.salt = new Uint8Array(1); },
      records => { records.vault.wrappedRoot[0] ^= 1; },
      records => { records.workspace.ciphertext[0] ^= 1; },
      records => { records.workspace.keyId[0] ^= 1; },
      records => { records.workspace.head.revision = "9007199254740993"; },
      records => { records.workspace.head.commitId[0] ^= 1; },
      records => { delete records.workspace.head.commitId[0]; },
    ]) {
      const corrupt = structuredClone(baseline); mutate(corrupt); await replace(corrupt);
      await attempt(["invalid_credentials", "invalid_envelope"]);
      const persisted = await readRecords();
      if (persisted.workspace.ciphertext[0] !== corrupt.workspace.ciphertext[0] || persisted.vault.profile !== corrupt.vault.profile) throw new Error("A8 rejected unlock overwrote records");
    }
    await replace(baseline); await attempt(["unlocked"]);
    return true;
  }, password);
  check(tampering, "A8 envelope rejection");
  await restored(page, encrypted, "restored");
  check(await page.locator("#restored").textContent() === fixture.january, "A8 validation failures preserve saved mappings");
  console.log("PASS A8 credentials, KDF bounds and authenticated-envelope tampering");

  await context?.close(); context = undefined;
  const browserPage = await launch("wasm-contract");
  await browserPage.evaluate(async counters => {
    const api = await import("./vendor/package/index.js");
    const { verifyWorkspace } = await import("./workspace-conformance.mjs");
    await verifyWorkspace(api, counters);
  }, counterFixtures);
  console.log("PASS A3/A7/A8 installed WASM inputs, lifecycle, canonical CBOR and uint64 parity");

  const vector = await browserPage.evaluate(async fixture => {
    const worker = new Worker("./kdf-worker.js", { type: "module" });
    try {
      return await new Promise((resolve, reject) => {
        worker.onmessage = ({ data }) => {
          if (!(data.key instanceof Uint8Array)) { reject(new Error("KDF fixture failed")); return; }
          const expected = fixture.outputHex;
          const same = data.key.length === 32 && data.key.every((b, i) => b === Number.parseInt(expected.slice(i * 2, i * 2 + 2), 16));
          data.key.fill(0); resolve(same);
        };
        worker.onerror = () => reject(new Error("KDF fixture failed"));
        worker.postMessage({ password: fixture.password, salt: Uint8Array.from(fixture.saltHex.match(/../g), v => Number.parseInt(v, 16)) });
      });
    } finally { worker.terminate(); }
  }, kdfFixture);
  check(vector, "A9 browser Argon2id matches independent native profile fixture");
  const nonextractable = await browserPage.evaluate(async password => {
    const api = await import("./vendor/package/index.js");
    const {createVault} = await import("./vault.js");
    const namespace = crypto.getRandomValues(new Uint8Array(16));
    const workspace = api.Workspace.create(namespace, crypto.getRandomValues(new Uint8Array(16)));
    try {
      const vault = await createVault(password, namespace, workspace.exportBundle(), workspace.head());
      try {
        const exported = await crypto.subtle.exportKey("raw", vault.dataKey);
        new Uint8Array(exported).fill(0);
        return false;
      } catch (error) { return error.name === "InvalidAccessError" && !vault.dataKey.extractable; }
    } finally { workspace.close(); }
  }, password);
  check(nonextractable, "A9 runtime workspace key export is rejected");
  check(observed.externalRequests === 0, "A9 no external requests");
  check(observed.consoleMessages === 0, "A9 no automatic console diagnostics");
  check(observed.pageErrors === 0, "A9 no unhandled page errors");
  const artifacts = JSON.parse(await readFile(path.join(harness, "dist/artifacts.json"), "utf8"));
  await writeFile(path.join(harness, "dist/integration-result.json"), JSON.stringify({
    result: "passed", browser: context.browser().version(), platform: process.platform, arch: process.arch,
    execution: process.env.GITHUB_ACTIONS === "true" ? "github-actions" : "local",
    scope: "synthetic Chromium workflow; branded Chrome and branch-rule enforcement are separate qualifications",
    checks: ["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "B1", "B2", "B3"],
    externalRequests: observed.externalRequests, consoleMessages: observed.consoleMessages, pageErrors: observed.pageErrors,
    artifacts,
  }, null, 2));
  console.log(`PASS A9/A10 local crypto, privacy and reproducibility (Chromium ${context.browser().version()}, ${process.platform}/${process.arch}, ${Object.keys(artifacts.hashes).length} hashed assets)`);
} finally {
  await context?.close();
  await rm(temporary, { recursive: true, force: true });
}
