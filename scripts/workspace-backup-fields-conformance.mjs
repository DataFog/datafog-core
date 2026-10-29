// Shared synthetic current-state decoder proof; Core owns schema and validation.
export async function verifyWorkspaceBackupFields(api,fixture) {
  if(api.init)await api.init();
  const check=(value,label)=>{if(!value)throw new Error(label);};
  const bytes=hex=>Uint8Array.from(hex.match(/../g),pair=>Number.parseInt(pair,16));
  const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==="object"?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
  const equal=(left,right)=>JSON.stringify(canonical(left))===JSON.stringify(canonical(right));
  check(fixture.schemaVersion===1,"unsupported backup field fixture");
  const original=bytes(fixture.current.bundleHex),workspace=api.Workspace.openBundle(original);
  try {
    check(equal([...workspace.exportBundle()],[...original]),"current canonical field fixture is not byte-exact");
    check(equal(workspace.head(),fixture.current.head),"current full head changed");
    const policy=workspace.policy();
    for(const key of ["snapshotVersion","defaultRule","rules","choices"])check(equal(policy[key],fixture.current.policy[key]),"current saved policy field changed");
    for(const alias of fixture.current.aliases)check(workspace.restore(alias.marker)===alias.value,"current saved identity or alias changed");
    for(const candidate of fixture.rejections) {
      let rejected;
      try{api.Workspace.openBundle(bytes(candidate.bundleHex)).close();}catch(error){rejected=error;}
      check(rejected instanceof api.DataFogError && rejected.code===candidate.code && rejected.message===candidate.code,"missing/malformed current snapshot was not rejected by Core");
      check(!rejected.markerIssues?.length && !rejected.reviewConflicts?.length,"snapshot error exposed unrelated source diagnostics");
    }
    check(equal([...workspace.exportBundle()],[...original]),"decoder rejection mutated current state");
    return {profile:"workspace-current-backup-fields-v1",snapshotVersion:2,rejections:fixture.rejections.length,aliases:fixture.current.aliases.length};
  } finally{workspace.close();original.fill(0);}
}
