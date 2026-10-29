// Separate native-host applicability: these are two SQLite hosts, never four SQL hosts.
import {copyFile,readFile,writeFile} from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";
import {isDeepStrictEqual} from "node:util";
const sha=value=>createHash("sha256").update(value).digest("hex");
const bad=()=>{throw new Error("native server parity contract failed");};
function same(actual,expected){if(!isDeepStrictEqual(actual,expected))bad();}
function acceptedError(value,fixture,sdk,kind){if(!value || !fixture.hostErrors[kind][sdk].includes(value.code) || value.message!==value.code)bad();}
function expectedState(value,expected){same(value,expected);}
export function validateNativeServerOutput(value,fixture,oracle,sdk){
 if(fixture?.schemaVersion!==1 || fixture.profile!=="native-server-cas-v1" || !isDeepStrictEqual(fixture.sdks,["python","node"]) || !Array.isArray(fixture.cases) || !fixture.cases.length || !["python","node"].includes(sdk))bad();
 if(value?.schemaVersion!==1 || value.profile!==fixture.profile || value.sdk!==sdk || !Array.isArray(value.cases) || value.cases.length!==fixture.cases.length)bad();
 for(let index=0;index<fixture.cases.length;index++){
  const expected=fixture.cases[index],actual=value.cases[index];
  if(actual.id!==expected.id || !Array.isArray(actual.steps) || actual.steps.length!==expected.steps.length)bad();
  actual.steps.forEach((step,index)=>{if(step.call!==expected.steps[index] || !Object.hasOwn(step,"value"))bad();});
  const step=name=>actual.steps.find(step=>step.call===name)?.value;
  const failed=(name,kind)=>{const result=step(name);if(result?.ok!==false)bad();acceptedError(result.error,fixture,sdk,kind);};
  const unchanged=name=>{const result=step(name);if(!/^[a-f0-9]{64}$/.test(result?.rowBeforeSha256??"") || result.rowBeforeSha256!==result.rowAfterSha256)bad();if(result.local)expectedState(result.local,oracle.seed);};
  expectedState(actual.before,oracle.seed);let after=oracle.seed;
  switch(expected.id){
   case "acknowledged-save":
    expectedState(step("open"),oracle.seed);if(step("save")?.ok!==true)bad();
    if(sdk==="node")same(step("save").acknowledgment,oracle.a.head);else same(step("save").acknowledgment,null);
    expectedState(step("activate"),oracle.a);expectedState(step("reopen"),oracle.a);after=oracle.a;break;
   case "full-parent-conflict":failed("wrong-revision","conflict");failed("wrong-commit-id","conflict");unchanged("unchanged");break;
   case "abort-rollback":failed("save-aborted","abort");unchanged("unchanged");break;
   case "lost-acknowledgment":
    failed("save-uncertain","lostAck");if(step("save-uncertain").saveCalls!==1)bad();expectedState(step("save-uncertain").local,oracle.seed);expectedState(step("reopen-committed"),oracle.a);after=oracle.a;break;
   case "process-changed-head-race":
   case "process-noop-race": {
    const noop=expected.id==="process-noop-race",ready=step("two-processes-ready"),outcomes=step("race");
    if(!Array.isArray(ready)||ready.length!==2||!Array.isArray(outcomes)||outcomes.length!==2)bad();
    ready.forEach(value=>{if(value.ready!==true)bad();same(value.parent,oracle.seed.head);});
    const winners=[];
    outcomes.forEach((outcome,index)=>{
     const candidate=noop?oracle.noop:oracle[index===0?"a":"b"];
     same(outcome.operationId,Array(16).fill(noop?5+index:fixture.candidateDefinitions[index===0?"a":"b"].operationByte));
     if(outcome.saveCalls!==1 || outcome.candidateBundleHex!==candidate.bundleHex)bad();
     if(outcome.ok===true){if(outcome.activated!==true)bad();same(outcome.head,candidate.head);same(outcome.acknowledgment,sdk==="node"?candidate.head:null);winners.push({outcome,candidate});}
     else{if(outcome.ok!==false||outcome.activated!==false)bad();acceptedError(outcome.error,fixture,sdk,"conflict");}
    });
    if(winners.length!==(noop?2:1))bad();after=noop?oracle.seed:winners[0].candidate;
    expectedState(step(noop?"reopen-same-head":"reopen-winner"),after);break;
   }
   case "process-crash-before-commit":
   case "process-crash-after-commit": {
    const before=expected.id==="process-crash-before-commit";
    if(step("child-at-boundary")?.boundary!==(before?"before_commit":"after_commit") || step("killed")?.signal!=="SIGKILL")bad();
    if(sdk==="python"?step("killed").code!==-9:step("killed").code!==null)bad();
    after=before?oracle.seed:oracle.a;expectedState(step(before?"reopen-original":"reopen-committed"),after);break;
   }
   case "authentication-and-isolation":acceptedError(step("tampered-ciphertext"),fixture,sdk,"authentication");acceptedError(step("tampered-head-metadata"),fixture,sdk,"authentication");acceptedError(step("wrong-key"),fixture,sdk,"authentication");acceptedError(step("other-tenant"),fixture,sdk,"otherTenant");unchanged("restored-row");break;
   default:bad();
  }
  expectedState(actual.after,after);
 }
 return value;
}
function stateOracle(fixture,result){
 const states={};
 const commands=fixture.oracle.cases[0].commands,steps=result.cases[0].steps;
 if(commands.length!==steps.length)bad();
 commands.forEach((command,index)=>{
  const step=steps[index];if(!step.ok)bad();
  if(["createWithOperation","openBundle"].includes(command.call))states[command.workspace]={...step.value};
  else if(["counts","operations","historyCoverage"].includes(command.call))states[command.workspace][command.call]=step.value;
 });
 if(!["seed","a","b","noop"].every(name=>states[name]?.bundleHex && states[name].counts && states[name].operations && states[name].historyCoverage))bad();
 return states;
}
export async function runNativeServerProfile({root,temporary,python,output,command,validateWorkspaceOutput}){
 const fixtureFile=path.join(root,"fixtures/workspace/native-server-contract-v1.json"),fixtureBytes=await readFile(fixtureFile),fixture=JSON.parse(fixtureBytes);
 if(fixture.profile!=="native-server-cas-v1" || !isDeepStrictEqual(fixture.sdks,["python","node"]))bad();
 const oracleFixtureFile=path.join(output,"native-server-oracle-fixture.json");await writeFile(oracleFixtureFile,JSON.stringify(fixture.oracle,null,2));
 const rawOracle=validateWorkspaceOutput(JSON.parse(command("cargo",["run","--quiet","-p","datafog-core","--example","workspace_contract","--",oracleFixtureFile])),fixture.oracle);
 const oracle=stateOracle(fixture,rawOracle),oracleFile=path.join(output,"native-server-oracle.json");await writeFile(oracleFile,JSON.stringify(oracle,null,2));
 await copyFile(fixtureFile,path.join(output,"native-server-fixture.json"));await writeFile(path.join(output,"native-server-core-reference.json"),JSON.stringify(rawOracle,null,2));
 command(python,["-m","pip","install","--disable-pip-version-check","--require-hashes","-r",path.join(root,"examples/native-python-host/requirements.txt")]);
 command(python,["-c","import pathlib,sys,datafog_core; assert pathlib.Path(datafog_core.__file__).resolve().is_relative_to(pathlib.Path(sys.prefix).resolve())"]);
 // Preserve and execute existing high-level real handoff/cancellation/stream suites.
 command(python,[path.join(root,"examples/native-python-host/test_host.py")]);
 const pythonSuite=JSON.parse(await readFile(path.join(root,"target/python-host-proof.json"),"utf8"));
 if(pythonSuite.result!=="passed" || pythonSuite.tests<fixture.minimumExistingPythonTests)bad();
 await writeFile(path.join(output,"native-server-python-existing-suite.json"),JSON.stringify(pythonSuite,null,2));
 const results={};
 results.python=JSON.parse(command(python,[path.join(root,"scripts/native-server-parity-python.py"),fixtureFile,oracleFile]));
 await writeFile(path.join(output,"python-native-server.json"),JSON.stringify(results.python,null,2));validateNativeServerOutput(results.python,fixture,oracle,"python");
 for(const file of ["native-server-parity-node.mjs","node-workspace-host.mjs","node-workspace-host-worker.mjs","node-workspace-host-crash.mjs","node-workspace-host-conformance.mjs"])await copyFile(path.join(root,"scripts",file),path.join(temporary,file));
 results.node=JSON.parse(command(process.execPath,["native-server-parity-node.mjs",fixtureFile,oracleFile],temporary));
 await writeFile(path.join(output,"node-native-server.json"),JSON.stringify(results.node,null,2));validateNativeServerOutput(results.node,fixture,oracle,"node");
 const suite=results.node.existingHostSuite;
 if(!suite || fixture.requiredExistingNodeChecks.some(name=>suite[name]!==true) || Object.values(suite).some(value=>value!==true&&!Number.isSafeInteger(value)) || suite.stateChangingRaceWinners!==1 || suite.stateChangingRaceLosers!==1 || suite.noOpReleases!==2)bad();
 const requirements=await readFile(path.join(root,"examples/native-python-host/requirements.txt"));
 return {profile:fixture.profile,sdks:["python","node"],oracleSdk:"rust",cases:fixture.cases.length,steps:fixture.cases.reduce((sum,value)=>sum+value.steps.length,0),fixtureSha256:sha(fixtureBytes),requirementsSha256:sha(requirements),existingSuites:{python:{tests:pythonSuite.tests,passed:true},node:{checks:Object.keys(suite).length,passed:true}},scope:"actual existing Python/Node encrypted SQLite hosts only; not four SQL hosts, provider/KMS/admission policy or conversation confirmation"};
}
