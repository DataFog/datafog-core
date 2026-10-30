import {NativeWorkspace,PreparedChange,type CreateOptions,type Committed} from '@datafog/core-fogvault-node';
const id=new Uint8Array(16);
const options:CreateOptions={tenant:'tenant',protectedControlPath:'control',vaultPath:'vault.sqlite',password:id,recordId:id,namespace:id,operationId:id,requestId:id,profile:'password_only'};
const workspace=NativeWorkspace.createNew(options);
const change=workspace.prepareReview('tenant',[{text:'x',scannerFindings:[],userSelections:[]}],id);
const committed:Committed=workspace.commit('tenant',change,true,id);
const revision:string=committed.head.revision;
NativeWorkspace.openExisting({tenant:options.tenant,protectedControlPath:options.protectedControlPath,vaultPath:options.vaultPath,password:id,recordId:id,namespace:id,expectedHead:committed.head});
void revision;
// @ts-expect-error approval is literal true, not a truthy number.
workspace.commit('tenant',change,1,id);
// @ts-expect-error PreparedChange is an opaque owner-created handle.
new PreparedChange();
// @ts-expect-error u64 heads are strings, never JavaScript numbers.
NativeWorkspace.openExisting({...options,expectedHead:{revision:1,commitId:id}});
// @ts-expect-error direct save/activation authority is not exposed.
workspace.activateSaved(change);
