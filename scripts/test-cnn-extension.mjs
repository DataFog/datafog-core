// Qualify the installed embedded CNN through a real offline MV3 extension.
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {cp, mkdir, readFile, writeFile} from "node:fs/promises";
import {createHash} from "node:crypto";
import {createRequire} from "node:module";
import path from "node:path";
import {fileURLToPath} from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const {chromium} = createRequire(path.join(root,"bindings/wasm/package.json"))("playwright");
const output = path.join(root,"target/dfv/runs",`cnn-browser-${Date.now()}`);
const extension = path.join(output,"extension");
await mkdir(extension,{recursive:true});
const packed = JSON.parse(execFileSync("npm",["pack","--json","--pack-destination",output],{cwd:path.join(root,"bindings/wasm"),encoding:"utf8"}))[0];
await writeFile(path.join(output,"package.json"),JSON.stringify({private:true,type:"module"}));
execFileSync("npm",["install","--ignore-scripts","--no-audit","--no-fund",path.join(output,packed.filename)],{cwd:output,stdio:"inherit"});
await writeFile(path.join(output,"typing.mts"), `import { CnnSession } from "@datafog/wasm/cnn";
const session = await CnnSession.open({cnnEnabled:true});
const generation: number = session.generation;
await session.setEnabled(false);
const result = await session.scan("text", {entities:["PERSON","ADDRESS","EMAIL"]});
const model: string | null = result.model;
session.close();
`);
execFileSync(process.execPath,[path.join(root,"bindings/wasm/node_modules/typescript/bin/tsc"),"--noEmit","--module","nodenext","--target","es2022",path.join(output,"typing.mts")],{cwd:output,stdio:"inherit"});
const installed = path.join(output,"node_modules/@datafog/wasm");
await cp(installed,path.join(extension,"sdk"),{recursive:true});
await cp(installed,path.join(extension,"corrupt-sdk"),{recursive:true});
const wasmFile = path.join(extension,"corrupt-sdk/dist/datafog_wasm_bg.wasm");
const wasm = await readFile(wasmFile);
const weights = await readFile(path.join(root,"crates/core/src/cnn/assets/bundle.bin"));
const position = wasm.indexOf(weights.subarray(0,64));
assert(position>=0,"compiled WASM must embed the actual supplied model bytes");
wasm[position] ^= 1;
await writeFile(wasmFile,wasm);
await writeFile(path.join(extension,"manifest.json"),JSON.stringify({manifest_version:3,name:"DataFog embedded CNN qualification",version:"0.0.0",background:{service_worker:"background.js"},content_security_policy:{extension_pages:"script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; connect-src 'self'; worker-src 'self'"}}));
await writeFile(path.join(extension,"background.js"),"chrome.runtime.onInstalled.addListener(() => {});\n");
await writeFile(path.join(extension,"proof.html"),'<script type="module" src="proof.js"></script>');
await writeFile(path.join(extension,"proof.js"),'import {CnnSession} from "./sdk/cnn/index.js"; import {CnnSession as CorruptSession} from "./corrupt-sdk/cnn/index.js"; window.proof={CnnSession,CorruptSession};');
const fixtures=JSON.parse(await readFile(path.join(root,"crates/core/src/cnn/reference.json"),"utf8")).fixtures;
const report={passed:false,tests:[],commit:execFileSync("git",["rev-parse","HEAD"],{cwd:root,encoding:"utf8"}).trim(),packageSha256:createHash("sha256").update(await readFile(path.join(output,packed.filename))).digest("hex"),modelSha256:createHash("sha256").update(weights).digest("hex"),externalRequests:[],timings:{}};
let context;
function passed(name){report.tests.push(name);console.log(`PASS ${name}`);}
async function launch(){
 context=await chromium.launchPersistentContext(path.join(output,"profile"),{channel:"chromium",headless:process.env.DATAFOG_HEADED!=="1",args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
 await context.setOffline(true);
 context.on("request",request=>{if(/^https?:/.test(request.url()))report.externalRequests.push(request.url());});
 let worker=context.serviceWorkers()[0];if(!worker)worker=await context.waitForEvent("serviceworker");
 report.browser=context.browser().version();
 const page=await context.newPage();await page.goto(`chrome-extension://${new URL(worker.url()).hostname}/proof.html`);await page.waitForFunction(()=>window.proof);return page;
}
try{
 let page=await launch();
 report.timings.coldOpenMs=await page.evaluate(async()=>{const start=performance.now();window.session=await window.proof.CnnSession.open();return performance.now()-start;});
 assert.equal(await page.evaluate(()=>session.status),"ready");
 assert.equal(await page.evaluate(()=>session.cnnEnabled),true);
 assert.equal(await page.evaluate(()=>session.describe().backend),"embedded_cnn");
 passed("installed default-on CNN loads offline under real MV3 CSP with embedded weights");
 for(const fixture of fixtures){
  const start=performance.now();const result=await page.evaluate(text=>session.scanModel(text),fixture.text);report.timings.scanMs??=[];report.timings.scanMs.push(performance.now()-start);
  assert.deepEqual(result.findings.map(f=>({label:f.entityType,start:f.byteRange.start,end:f.byteRange.end})),fixture.spans);
  for(const f of result.findings){assert.equal(Buffer.from(fixture.text).subarray(f.byteRange.start,f.byteRange.end).toString(),f.matchedText);assert.equal([...fixture.text].slice(f.codepointRange.start,f.codepointRange.end).join(""),f.matchedText);assert.equal(fixture.text.slice(f.utf16Range.start,f.utf16Range.end),f.matchedText);}
 }
 passed("serialized quantized-reference predictions match WASM including Unicode and window seams");
 const text=fixtures[0].text;
 const rules=await page.evaluate(()=>session.scan("john@example.com",{entities:["EMAIL"]}));assert.equal(rules.findings[0].entityType,"EMAIL");
 assert.equal(await page.evaluate(async()=>{try{await session.scanModel("\ud800");}catch(e){return e.reason;}}),"invalid_unicode");
 assert.equal(await page.evaluate(async()=>{try{await session.scanModel("x".repeat(65537));}catch(e){return e.code;}}),"model_resource_exhausted");
 passed("rule routing and input limits preserve explicit errors");
 const disabled=await page.evaluate(async text=>{const pending=session.scanModel(text);const settled=pending.then(()=>"unexpected",e=>e.code);const before=session.generation;await session.setEnabled(false);return {settled:await settled,before,after:session.generation,status:session.status,descriptor:session.describe(),result:await session.scan(text)};},text);
 assert.equal(disabled.settled,"model_request_cancelled");assert.equal(disabled.after,disabled.before+1);assert.equal(disabled.status,"disabled");assert.equal(disabled.descriptor,undefined);assert(!disabled.result.findings.some(f=>f.entityType==="PERSON"));
 const selected = await page.evaluate(async () => {
   const result=await session.scan("John Smith john@example.com", {entities:["PERSON","ADDRESS","EMAIL"]});
   const onlyModel=await session.scan("John Smith",{entities:["PERSON","ADDRESS"]});
   let invalid;try{await session.scan("text",{entities:[]});}catch(e){invalid=e.code;}
   return {entities:result.findings.map(f=>f.entityType),onlyModel:onlyModel.findings,invalid};
 });
 assert.deepEqual(selected.entities,["EMAIL"]);assert.deepEqual(selected.onlyModel,[]);assert.equal(selected.invalid,"invalid_configuration");
 await page.evaluate(()=>session.setEnabled(true));assert((await page.evaluate(text=>session.scanModel(text),text)).findings.some(f=>f.entityType==="PERSON"));
 passed("toggle cancels old inference, changes generation and restores default CNN on re-enable");
 const corrupt=await page.evaluate(async()=>{const bad=await proof.CorruptSession.open({cnnEnabled:false});let failure;try{await bad.setEnabled(true);}catch(e){failure=e.code;}const failedStatus=bad.status;await bad.setEnabled(false);const recoveredStatus=bad.status;bad.close();return {failure,failedStatus,recoveredStatus};});
 assert.deepEqual(corrupt,{failure:"model_load_failed",failedStatus:"error",recoveredStatus:"disabled"});
 passed("disabled startup skips weight decoding; corrupt bundled weights fail enabled load and recovery is explicit");
 const close=await page.evaluate(async text=>{const pending=session.scanModel(text).then(()=>"unexpected",e=>e.code);session.close();session.close();let after;try{await session.scanModel(text);}catch(e){after=e.code;}return {pending:await pending,after,status:session.status};},text);
 assert.deepEqual(close,{pending:"model_session_closed",after:"model_session_closed",status:"closed"});
 passed("close revokes pending inference immediately and is idempotent");
 await context.close();context=undefined;page=await launch();
 const reopened=await page.evaluate(async text=>{const s=await proof.CnnSession.open();const r=await s.scanModel(text);s.close();return r;},text);
 assert(reopened.findings.some(f=>f.matchedText==="John Smith"));passed("fresh browser process reloads CNN from packaged assets offline");
 assert.deepEqual(report.externalRequests,[]);report.passed=true;
}finally{
 await context?.close();await writeFile(path.join(output,"report.json"),JSON.stringify(report,null,2));console.log(`Report: ${path.join(output,"report.json")}`);
}
