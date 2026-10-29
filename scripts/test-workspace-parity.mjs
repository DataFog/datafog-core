import {spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import {createServer} from "node:http";
import {mkdtemp,readFile,writeFile,mkdir,copyFile,rm,readdir} from "node:fs/promises";
import {createRequire} from "node:module";
import os from "node:os";
import path from "node:path";
import {fileURLToPath,pathToFileURL} from "node:url";
import {isDeepStrictEqual} from "node:util";
import {canonicalWorkspaceValue} from "./workspace-parity-runner.mjs";

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
const fixturePath=path.join(root,"fixtures/workspace/sdk-contract-v1.json");
const streamingFixturePath=path.join(root,"fixtures/workspace/streaming-contract-v1.json");
const arguments_=process.argv.slice(2);
const outputIndex=arguments_.indexOf("--output");
const output=path.resolve(outputIndex>=0?arguments_[outputIndex+1]:path.join(root,"target/workspace-parity",new Date().toISOString().replaceAll(/[:.]/g,"-")));
const sdkNames=["rust","python","node","wasm"];
const sha=bytes=>createHash("sha256").update(bytes).digest("hex");
function command(executable,args,cwd=root) {
  const result=spawnSync(executable,args,{cwd,encoding:"utf8",maxBuffer:64*1024*1024});
  if(result.error || result.status!==0)throw new Error("workspace parity child process failed");
  return result.stdout;
}
function parsed(value){try{return JSON.parse(value);}catch{throw new Error("workspace parity child output is malformed");}}
export function validateWorkspaceOutput(value,fixture) {
  if(fixture?.schemaVersion!==1 || !Array.isArray(fixture.cases) || fixture.cases.length===0)throw new Error("workspace parity fixture schema mismatch");
  if(value?.schemaVersion!==1 || value.profile!==fixture.profile || !Array.isArray(value.cases) || value.cases.length!==fixture.cases.length)throw new Error("workspace parity output schema mismatch");
  for(let i=0;i<fixture.cases.length;i++) {
    const expected=fixture.cases[i],actual=value.cases[i];
    if(actual?.id!==expected.id || !Array.isArray(actual.steps) || actual.steps.length!==expected.commands.length)throw new Error("workspace parity output case mismatch");
    for(let j=0;j<expected.commands.length;j++) {
      const call=expected.commands[j],step=actual.steps[j];
      if(step?.call!==call.call || typeof step.ok!=="boolean")throw new Error("workspace parity output step mismatch");
      if(call.expectCode) {
        if(step.ok!==false || step.error?.code!==call.expectCode || !Object.hasOwn(step.error,"inputIndex") || !Object.hasOwn(step.error,"findingIndex") || !Array.isArray(step.error.markerIssues) || !Array.isArray(step.error.reviewConflicts))throw new Error("workspace parity required error mismatch");
      } else if(step.ok!==true || !Object.hasOwn(step,"value"))throw new Error("workspace parity unexpected operation failure");
      if(step.ok && Array.isArray(step.value?.issues) && step.value.issues.length>0 && Object.hasOwn(step.value,"restoredMarkers") && step.value.text!==null)throw new Error("workspace parity partial restore reported as success");
    }
  }
  return canonicalWorkspaceValue(value);
}

function validCounts(value) {
  return value && ["bytes","codepoints","utf16","restoredMarkers"].every(key=>Number.isSafeInteger(value[key]) && value[key]>=0);
}
function validMarkerIssues(issues) {
  return Array.isArray(issues) && issues.every(issue=>["unknown","damaged","unbracketed"].includes(issue?.kind) && ["byteRange","codepointRange","utf16Range"].every(key=>Number.isSafeInteger(issue[key]?.start) && Number.isSafeInteger(issue[key]?.end) && issue[key].start>=0 && issue[key].end>=issue[key].start));
}
export function validateStreamingOutput(value,fixture) {
  const invalid=()=>{throw new Error("workspace streaming parity output schema mismatch");};
  if(fixture?.schemaVersion!==1 || fixture.profile!=="workspace-streaming-v1" || !Array.isArray(fixture.cases) || !fixture.cases.length) invalid();
  if(value?.schemaVersion!==1 || value.profile!==fixture.profile || !Array.isArray(value.cases) || value.cases.length!==fixture.cases.length) invalid();
  for(let i=0;i<fixture.cases.length;i++) {
    const expected=fixture.cases[i],actual=value.cases[i];
    if(actual?.id!==expected.id || !actual.modes || Object.keys(actual.modes).sort().join(",")!=="buffered,progressive") invalid();
    for(const mode of ["buffered","progressive"]) {
      const steps=actual.modes[mode]?.steps;
      if(!Array.isArray(steps) || steps.length!==expected.steps.length) invalid();
      for(let j=0;j<steps.length;j++) {
        const wanted=expected.steps[j],step=steps[j];
        if(step?.op!==wanted.op || typeof step.ok!=="boolean") invalid();
        if(mode==="progressive" && wanted.expectError && (step.ok || step.error?.code!==wanted.expectError.code)) invalid();
        if(mode==="progressive" && !wanted.expectError && !step.ok) invalid();
        if(step.ok) {
          if(!Object.hasOwn(step,"value")) invalid();
          if(mode==="progressive" && ["feed","feedUtf16","finalize"].includes(step.op)) {
            if(typeof step.value?.text!=="string" || !validCounts(step.value.delivered) || step.value.complete!==(step.op==="finalize")) invalid();
          } else if(mode==="buffered" && step.op==="finalize") {
            const report=step.value;
            if(!report || !(report.text===null || typeof report.text==="string") || !Number.isSafeInteger(report.restoredMarkers) || report.restoredMarkers<0 || !validMarkerIssues(report.issues) || (report.issues.length>0)!==(report.text===null)) invalid();
          } else if(step.value!==null) invalid();
        } else {
          const error=step.error;
          if(!error || typeof error.code!=="string" || Object.hasOwn(error,"text") || !["findingIndex","inputIndex","delivered","complete"].every(key=>Object.hasOwn(error,key)) || !validMarkerIssues(error.markerIssues) || !Array.isArray(error.reviewConflicts)) invalid();
          if(mode==="progressive" ? error.complete!==false || !validCounts(error.delivered) : error.complete!==null || error.delivered!==null) invalid();
        }
      }
    }
  }
  return canonicalWorkspaceValue(value);
}

// Retained wheels from earlier releases are valid artifacts, not current inputs.
// Require the exact repository pin rather than deleting them or guessing newest.
export function selectWorkspaceWheel(names,projectToml) {
  const project=/^\[project\]\s*\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(projectToml)?.[1];
  const versions=project ? [...project.matchAll(/^version[ \t]*=[ \t]*(["'])([A-Za-z0-9.!+_-]+)\1[ \t]*(?:#.*)?$/gm)] : [];
  const distributions=project ? [...project.matchAll(/^name[ \t]*=[ \t]*(["'])([A-Za-z0-9._-]+)\1[ \t]*(?:#.*)?$/gm)] : [];
  if(versions.length!==1 || distributions.length!==1) throw new Error("workspace parity Python project pin is invalid");
  const version=versions[0][2],distribution=distributions[0][2].replaceAll(/[-_.]+/g,"_");
  const candidates=names.filter(name=>typeof name==="string" && name.startsWith(`${distribution}-${version}-`) && name.endsWith(".whl"));
  if(candidates.length!==1) throw new Error("workspace parity requires exactly one wheel matching the current Python project pin");
  return {filename:candidates[0],version};
}

async function main() {
let temporary,server,browser;
const report={schemaVersion:1,pass:false,sdks:sdkNames,profile:null,errors:[],packages:{}};
try {
  await mkdir(output,{recursive:true});
  const fixtureBytes=await readFile(fixturePath),fixture=parsed(fixtureBytes.toString("utf8"));
  const streamingBytes=await readFile(streamingFixturePath),streamingFixture=parsed(streamingBytes.toString("utf8"));
  report.profile=fixture.profile;report.fixtureSha256=sha(fixtureBytes);report.streamingFixtureSha256=sha(streamingBytes);
  report.sourceCommit=command("git",["rev-parse","HEAD"]).trim();report.sourceDirty=command("git",["status","--porcelain"]).length>0;
  report.runtime={node:process.version,rust:command("rustc",["--version"]).trim()};
  await copyFile(fixturePath,path.join(output,"fixture.json"));
  await copyFile(streamingFixturePath,path.join(output,"streaming-fixture.json"));
  temporary=await mkdtemp(path.join(os.tmpdir(),"datafog-workspace-parity-"));
  const packages=path.join(output,"packages");await mkdir(packages,{recursive:true});
  await writeFile(path.join(temporary,"package.json"),JSON.stringify({private:true,type:"module"}));
  const tarballs=[];
  for(const sdk of ["node","wasm"]) {
    const packed=parsed(command("npm",["pack","--json","--pack-destination",packages],path.join(root,"bindings",sdk)));
    if(packed.length!==1 || typeof packed[0].filename!=="string")throw new Error("workspace parity package receipt malformed");
    const file=path.join(packages,packed[0].filename),bytes=await readFile(file);report.packages[sdk]={file,sha256:sha(bytes),bytes:bytes.length};tarballs.push(file);
  }
  command("npm",["install","--ignore-scripts","--no-audit","--no-fund",...tarballs],temporary);
  const projectBytes=await readFile(path.join(root,"bindings/python/pyproject.toml"));
  const selected=selectWorkspaceWheel(await readdir(path.join(root,"target/wheels")),projectBytes.toString("utf8"));
  const wheel=path.join(packages,selected.filename);await copyFile(path.join(root,"target/wheels",selected.filename),wheel);
  const wheelBytes=await readFile(wheel);report.packages.python={file:wheel,sha256:sha(wheelBytes),bytes:wheelBytes.length,version:selected.version,projectSha256:sha(projectBytes)};
  command("python3",["-m","venv",path.join(temporary,"venv")]);
  const python=path.join(temporary,"venv/bin/python");
  command(python,["-m","pip","install","--no-deps","--disable-pip-version-check",wheel]);
  report.runtime.python=command(python,["--version"]).trim();
  if(command(python,["-c","from importlib.metadata import version; print(version('datafog-core'))"]).trim()!==selected.version) throw new Error("workspace parity installed Python package version differs from its pin");
  const results={},streaming={};
  results.rust=validateWorkspaceOutput(parsed(command("cargo",["run","--quiet","-p","datafog-core","--example","workspace_contract","--",fixturePath])),fixture);
  streaming.rust=validateStreamingOutput(parsed(command("cargo",["run","--quiet","-p","datafog-core","--example","workspace_streaming_contract","--",streamingFixturePath])),streamingFixture);
  results.python=validateWorkspaceOutput(parsed(command(python,[path.join(root,"scripts/workspace-parity-python.py"),fixturePath])),fixture);
  streaming.python=validateStreamingOutput(parsed(command(python,[path.join(root,"scripts/workspace-parity-streaming-python.py"),streamingFixturePath])),streamingFixture);
  await copyFile(path.join(root,"scripts/workspace-parity-runner.mjs"),path.join(temporary,"runner.mjs"));
  await copyFile(path.join(root,"scripts/workspace-streaming-conformance.mjs"),path.join(temporary,"streaming-runner.mjs"));
  await copyFile(streamingFixturePath,path.join(temporary,"streaming-fixture.json"));
  await copyFile(fixturePath,path.join(temporary,"fixture.json"));
  await writeFile(path.join(temporary,"node.mjs"),'import fs from "node:fs";import * as api from "@datafog/node";import {runWorkspaceContract} from "./runner.mjs";process.stdout.write(JSON.stringify(runWorkspaceContract(api,JSON.parse(fs.readFileSync("fixture.json","utf8")))));');
  results.node=validateWorkspaceOutput(parsed(command(process.execPath,["node.mjs"],temporary)),fixture);
  await writeFile(path.join(temporary,"node-streaming.mjs"),'import fs from "node:fs";import * as api from "@datafog/node";import {runWorkspaceStreamingContract} from "./streaming-runner.mjs";process.stdout.write(JSON.stringify(await runWorkspaceStreamingContract(api,JSON.parse(fs.readFileSync("streaming-fixture.json","utf8")))));');
  streaming.node=validateStreamingOutput(parsed(command(process.execPath,["node-streaming.mjs"],temporary)),streamingFixture);
  await writeFile(path.join(temporary,"index.html"),"<!doctype html><title>Workspace SDK parity</title>");
  server=createServer(async(request,response)=>{
    try {
      const pathname=new URL(request.url,"http://localhost").pathname;
      const file=path.resolve(temporary,`.${pathname==="/"?"/index.html":pathname}`);
      if(!file.startsWith(temporary+path.sep)){response.writeHead(404).end();return;}
      const content=await readFile(file);const mime=file.endsWith(".wasm")?"application/wasm":file.endsWith(".js")||file.endsWith(".mjs")?"text/javascript":file.endsWith(".json")?"application/json":"text/html";
      response.writeHead(200,{"content-type":mime});response.end(content);
    } catch{response.writeHead(404).end();}
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const address=server.address();if(!address || typeof address==="string")throw new Error("workspace parity browser server unavailable");
  const origin=`http://127.0.0.1:${address.port}`;
  const {chromium}=createRequire(path.join(root,"bindings/wasm/package.json"))("playwright");
  browser=await chromium.launch({headless:true});const page=await browser.newPage();let browserErrors=0,externalRequests=0;
  page.on("pageerror",()=>{browserErrors++;});await page.route("**/*",route=>{if(!route.request().url().startsWith(origin+"/")){externalRequests++;return route.abort();}return route.continue();});
  await page.goto(origin);
  const wasm=await page.evaluate(async()=>{const api=await import("/node_modules/@datafog/wasm/index.js");await api.init();const {runWorkspaceContract}=await import("/runner.mjs");return runWorkspaceContract(api,await fetch("/fixture.json").then(response=>response.json()));});
  if(browserErrors || externalRequests)throw new Error("workspace parity browser runtime failed");
  report.runtime.chromium=browser.version();results.wasm=validateWorkspaceOutput(wasm,fixture);
  const wasmStreaming=await page.evaluate(async()=>{const api=await import("/node_modules/@datafog/wasm/index.js");const {runWorkspaceStreamingContract}=await import("/streaming-runner.mjs");return runWorkspaceStreamingContract(api,await fetch("/streaming-fixture.json").then(response=>response.json()));});
  if(browserErrors || externalRequests)throw new Error("workspace streaming parity browser runtime failed");
  streaming.wasm=validateStreamingOutput(wasmStreaming,streamingFixture);
  for(const sdk of sdkNames){await writeFile(path.join(output,`${sdk}.json`),JSON.stringify(results[sdk],null,2));if(!isDeepStrictEqual(results.rust,results[sdk]))report.errors.push({sdk,category:"workspace contract mismatch"});}
  for(const sdk of sdkNames){await writeFile(path.join(output,`${sdk}-streaming.json`),JSON.stringify(streaming[sdk],null,2));if(!isDeepStrictEqual(streaming.rust,streaming[sdk]))report.errors.push({sdk,category:"workspace streaming contract mismatch"});}
  report.profiles={foundation:{profile:fixture.profile,cases:fixture.cases.length,steps:fixture.cases.reduce((sum,item)=>sum+item.commands.length,0)},streaming:{profile:streamingFixture.profile,cases:streamingFixture.cases.length,modes:["buffered","progressive"],steps:2*streamingFixture.cases.reduce((sum,item)=>sum+item.steps.length,0)}};
  report.cases=report.profiles.foundation.cases+report.profiles.streaming.cases;report.steps=report.profiles.foundation.steps+report.profiles.streaming.steps;report.pass=report.errors.length===0;
} catch {
  report.errors.push({category:"workspace parity failed before complete four-SDK comparison"});
} finally {
  await browser?.close();if(server)await new Promise(resolve=>server.close(resolve));if(temporary)await rm(temporary,{recursive:true,force:true});
  await mkdir(output,{recursive:true});await writeFile(path.join(output,"workspace-parity.json"),JSON.stringify(report,null,2));
}
if(!report.pass){console.error("Workspace SDK parity failed; see the retained content-safe report.");process.exitCode=1;}else{console.log(`Workspace SDK parity passed: ${report.cases} cases, ${report.steps} steps, Rust/Python/Node/browser WASM.`);}

}
if(process.argv[1] && import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)await main();
