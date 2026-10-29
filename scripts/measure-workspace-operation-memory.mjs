import {cp,mkdtemp,readFile,rm,writeFile,mkdir} from "node:fs/promises";
import {execFileSync} from "node:child_process";
import {createRequire} from "node:module";
import {createHash} from "node:crypto";
import path from "node:path";
import os from "node:os";
import {fileURLToPath} from "node:url";

// Controlled synthetic measurement, not a production memory budget or a read limit.
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
const harness=path.join(root,"examples/workspace-extension");
const {chromium}=createRequire(path.join(harness,"package.json"))("playwright");
const temporary=await mkdtemp(path.join(os.tmpdir(),"datafog-history-memory-"));
const password="synthetic-capacity-password-only";
let context;
function check(condition,message){if(!condition)throw new Error(message);}
async function launch(profile){
  context=await chromium.launchPersistentContext(path.join(temporary,profile),{
    channel:"chromium",headless:true,args:["--enable-automation",`--disable-extensions-except=${path.join(harness,"dist")}`,`--load-extension=${path.join(harness,"dist")}`],
  });
  let worker=context.serviceWorkers()[0];if(!worker)worker=await context.waitForEvent("serviceworker");
  const page=await context.newPage();await page.goto(`chrome-extension://${new URL(worker.url()).hostname}/index.html`);
  await page.waitForFunction(()=>document.querySelector("#status").textContent==="ready");return page;
}
async function close(){await context?.close();context=undefined;}
async function sample(page,renderer,label){
  await renderer.send("HeapProfiler.collectGarbage");
  const browser=await context.browser().newBrowserCDPSession();
  try {
    const {processInfo}=await browser.send("SystemInfo.getProcessInfo");
    const pids=processInfo.map(p=>p.id).filter(p=>Number.isSafeInteger(p)&&p>0);
    const rss=new Map(execFileSync("ps",["-o","pid=,rss=","-p",pids.join(",")],{encoding:"utf8"}).trim().split("\n").map(line=>{const [id,size]=line.trim().split(/\s+/).map(Number);return[id,size*1024];}));
    const processes=processInfo.map(process=>({type:process.type,rssBytes:rss.get(process.id)??null}));
    const heap=await renderer.send("Runtime.getHeapUsage");
    return {label,processCount:processes.length,processes,processRssSumBytes:processes.reduce((sum,p)=>sum+(p.rssBytes??0),0),
      jsHeapUsedBytes:heap.usedSize,jsHeapTotalBytes:heap.totalSize,
      wasmLinearMemoryBytes:await page.evaluate(()=>globalThis.historyProbe.memory.buffer.byteLength)};
  } finally{await browser.detach();}
}
try {
  const artifacts=JSON.parse(await readFile(path.join(harness,"dist/artifacts.json"),"utf8"));
  const fixtureManifest=JSON.parse(await readFile(path.join(root,"target/operation-capacity/manifest.json"),"utf8"));
  let page=await launch("seed");
  const encryptedIdentity=await page.evaluate(async password=>{
    const api=await import("./vendor/package/index.js");await api.init();
    const {createVault}=await import("./vault.js"),{saveRecord,readRecords}=await import("./storage.js");
    const bundle=new Uint8Array(await fetch("./capacity-fixtures/ordinary.cbor").then(r=>r.arrayBuffer()));
    const workspace=api.Workspace.openBundle(bundle);bundle.fill(0);
    try {
      const created=await createVault(password,workspace.namespace(),workspace.exportBundle(),workspace.head());
      await saveRecord({...created,creating:true,authorized:()=>true});
      const bytes=new TextEncoder().encode(JSON.stringify(await readRecords()));
      try{return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)));}finally{bytes.fill(0);}
    } finally{workspace.close();}
  },password);
  await close();
  const measurements=[];
  // Clone only after clean browser shutdown: exact same encrypted bytes/salt/KDF in each fresh process.
  for(let repetition=0;repetition<2;repetition++)for(const mode of ["none","full","paged"]){
    const profile=`${mode}-${repetition}`;await cp(path.join(temporary,"seed"),path.join(temporary,profile),{recursive:true});
    page=await launch(profile);
    const opened=await page.evaluate(async password=>{
      const api=await import("./vendor/package/index.js");await api.init();
      const wasm=await(await import("./vendor/package/dist/datafog_wasm.js")).default();
      const {readRecords}=await import("./storage.js"),{openVault}=await import("./vault.js");
      const saved=await readRecords();const identityBytes=new TextEncoder().encode(JSON.stringify(saved));
      let encryptedIdentity;try{encryptedIdentity=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",identityBytes)));}finally{identityBytes.fill(0);}
      const opened=await openVault(password,saved.vault,saved.workspace);
      try {
        const workspace=api.Workspace.openBundle(opened.bundle);
        const bytes=workspace.exportBundle();let digest;
        try{digest=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)));}finally{bytes.fill(0);}
        globalThis.historyProbe={workspace,memory:wasm.memory,retained:null};
        return {encryptedIdentity,digest,count:workspace.counts().operations,profile:saved.vault.profile};
      } finally{opened.bundle.fill(0);}
    },password);
    check(JSON.stringify(opened.encryptedIdentity)===JSON.stringify(encryptedIdentity),"every process opens identical encrypted vault/KDF");
    check(opened.count===99744&&opened.profile===1,"same full ordinary-boundary state and live KDF");
    const renderer=await context.newCDPSession(page);
    try {
      const before=await sample(page,renderer,"opened-before-history-read");
      const read=await page.evaluate(mode=>{
        const probe=globalThis.historyProbe,workspace=probe.workspace;
        let count=0,preparedInputs=0,transformedSpans=0,checksum=2166136261;
        function consume(records){for(const row of records){
          count++;preparedInputs+=row.preparedInputs;transformedSpans+=row.transformedSpans;
          for(const character of JSON.stringify(row)){checksum=Math.imul(checksum^character.charCodeAt(0),16777619)>>>0;}
        }}
        const started=performance.now();
        if(mode==="full"){probe.retained=workspace.operations();consume(probe.retained);}
        else if(mode==="paged"){
          let offset=0;
          do{const page=workspace.operationsPage(offset,1000);probe.retained=page.records;consume(page.records);offset=page.nextOffset;}while(offset!==null);
        } else probe.retained=workspace.counts();
        return {count,preparedInputs,transformedSpans,checksum,elapsedMs:performance.now()-started};
      },mode);
      if(mode!=="none")check(read.count===99744,"full/paged traversal reads every record, including final partial page");
      const retained=await sample(page,renderer,"history-result-retained-after-gc");
      const final=await page.evaluate(async()=>{
        const probe=globalThis.historyProbe;probe.retained=null;
        const bytes=probe.workspace.exportBundle();try{return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)));}finally{bytes.fill(0);}
      });
      check(JSON.stringify(final)===JSON.stringify(opened.digest),"all read modes leave snapshot byte-identical");
      const released=await sample(page,renderer,"result-released-after-gc");
      measurements.push({repetition,mode,opened:{records:opened.count,profile:opened.profile},read,before,retained,released,unchanged:true});
      console.log(`PASS controlled ${mode} repetition${repetition}: ${retained.jsHeapUsedBytes} JS bytes, ${retained.wasmLinearMemoryBytes} WASM bytes`);
      await page.evaluate(()=>{globalThis.historyProbe.workspace.close();delete globalThis.historyProbe;});
    } finally{await renderer.detach();await close();}
  }
  const reads=measurements.filter(value=>value.mode!=="none");
  check(reads.every(value=>["count","preparedInputs","transformedSpans","checksum"].every(key=>value.read[key]===reads[0].read[key])),"every complete/paged traversal returns identical ordered records and numeric totals");
  const output=process.argv[2]??path.join(root,"target/operation-capacity/controlled-browser-memory.json");await mkdir(path.dirname(output),{recursive:true});
  await writeFile(output,JSON.stringify({scope:"same encrypted snapshot/profile1 KDF, fresh cloned browser process for each mode, two repetitions; no new read limit",sourceCommit:artifacts.sourceCommit,sourceDirty:artifacts.sourceDirty,
    browser:chromium.executablePath(),platform:process.platform,arch:process.arch,fixtureSha256:fixtureManifest.fixtures.ordinary.sha256,
    encryptedIdentitySha256:createHash("sha256").update(Buffer.from(encryptedIdentity)).digest("hex"),pageSize:1000,measurements,
    caveats:["RSS sums may double-count shared pages; process topology and per-process samples are reported, not unique physical memory.","WASM linear memory is a nonshrinking allocation high-water mark. Explicit GC applies to JavaScript only.","Single machine instrumented observations are not a lower-hardware requirement or latency SLA.","Per-row JSON checksum traversal is common to full/paged modes; no-read mode intentionally avoids all record materialization."],passed:true},null,2));
}finally{await close();await rm(temporary,{recursive:true,force:true});}
