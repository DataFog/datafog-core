import {loadNative} from './native-loader.mjs';
import {types} from 'node:util';
const {NativeOwner}=loadNative();
const owners=new WeakMap(), changes=new WeakMap(), streams=new WeakMap();
// Held values retain the owner, never their target. Cleanup only releases
// abandoned handles; it cannot save, activate or approve a candidate.
const abandoned=new FinalizationRegistry(({owner,handle,method})=>{
  const state=owners.get(owner);
  if(!state)return;
  try{state.raw.call(state.tenant,method,JSON.stringify(method==='discard'?handle:{handle}));}catch{/* Explicit lock/close may already have revoked it. */}
});
const internal=Symbol('Core FogVault owner construction');
const MAX=16*1024*1024;
export class DataFogVaultError extends Error {
  constructor(code, detail={}) {super(code);this.name='DataFogVaultError';this.code=code;Object.assign(this,detail);}
}
const fail=code=>{throw new DataFogVaultError(code);};
function plain(value,depth=0,budget={items:0}) {
  if(depth>32||++budget.items>300000)fail('resource_limit');
  if(value===null||typeof value==='boolean')return value;
  if(typeof value==='string'){if(!value.isWellFormed())fail('invalid_encoding');if(Buffer.byteLength(value)>MAX)fail('resource_limit');return value;}
  if(typeof value==='number'){if(!Number.isFinite(value)||(Number.isInteger(value)&&!Number.isSafeInteger(value)))fail('invalid_transport');return value;}
  if(typeof value!=='object'||types.isProxy(value))fail('invalid_transport');
  if(value instanceof Uint8Array){return Array.from(bytes(value));}
  if(value instanceof Uint16Array){safeTyped(value,Uint16Array.prototype);if(value.byteLength>MAX)fail('resource_limit');return Array.from(value);}
  const prototype=Object.getPrototypeOf(value);
  const array=Array.isArray(value);
  if(array?prototype!==Array.prototype:prototype!==Object.prototype&&prototype!==null)fail('invalid_transport');
  const descriptors=Object.getOwnPropertyDescriptors(value),out=array?[]:{};
  for(const key of Reflect.ownKeys(descriptors)){
    if(array&&key==='length')continue;
    if(typeof key!=='string'||key==='__proto__'||key==='constructor'||key==='prototype')fail('invalid_transport');
    const d=descriptors[key];if(!('value' in d)||!d.enumerable)fail('invalid_transport');
    if(array&&(!/^(0|[1-9][0-9]*)$/.test(key)||Number(key)>=value.length))fail('invalid_transport');
    out[key]=plain(d.value,depth+1,budget);
  }
  if(array&&Object.keys(out).length!==value.length)fail('invalid_transport');
  return out;
}
function serialized(value){const out=JSON.stringify(plain(value));if(Buffer.byteLength(out)>MAX)fail('resource_limit');return out;}
function error(e){if(e instanceof DataFogVaultError)return e;try{const d=JSON.parse(e.message);if(typeof d.code==='string')return new DataFogVaultError(d.code,d);}catch{}return new DataFogVaultError('native_error');}
function safeTyped(value,prototype){
  if(types.isProxy(value)||Object.getPrototypeOf(value)!==prototype)fail('invalid_transport');
  for(const key of ['buffer','byteLength','byteOffset','length','constructor',Symbol.iterator])if(Object.hasOwn(value,key))fail('invalid_transport');
  if(!(value.buffer instanceof ArrayBuffer)||types.isSharedArrayBuffer(value.buffer)||value.buffer.detached)fail('invalid_transport');
}
function bytes(value,length,maximum=MAX){
  if(!(value instanceof Uint8Array))fail('invalid_transport');
  safeTyped(value,Buffer.isBuffer(value)?Buffer.prototype:Uint8Array.prototype);
  if(length!==undefined&&value.length!==length)fail('invalid_transport');
  if(value.byteLength>maximum)fail('resource_limit');return Buffer.from(value);
}
function id(value){return Array.from(bytes(value,16));}
function tenant(value){if(typeof value!=='string'||!value.isWellFormed()||!value.length||Buffer.byteLength(value)>128||/\p{Cc}/u.test(value))fail('invalid_tenant');return value;}
function state(owner){const s=owners.get(owner);if(!s)fail('invalid_owner');return s;}
function call(owner,t,method,value=null){const s=state(owner);tenant(t);try{return JSON.parse(s.raw.call(t,method,serialized(value)));}catch(e){throw error(e);}}
function factory(options,mode,bundle,approved){
  if(!options||typeof options!=='object'||types.isProxy(options)||![Object.prototype,null].includes(Object.getPrototypeOf(options)))fail('invalid_configuration');
  const descriptors=Object.getOwnPropertyDescriptors(options),config={};let originalPassword;
  for(const key of Reflect.ownKeys(descriptors)){
    const d=descriptors[key];if(typeof key!=='string'||!('value' in d)||!d.enumerable||['__proto__','constructor','prototype'].includes(key))fail('invalid_transport');
    if(key==='password')originalPassword=d.value;else config[key]=d.value;
  }
  const data=plain(config);
  const creating=mode==='createNew'||mode==='importAuthenticatedBundle';
  const allowed=creating?['tenant','protectedControlPath','vaultPath','recordId','namespace','operationId','requestId','profile','maxPayloadBytes',...(mode==='importAuthenticatedBundle'?['sourceHead']:[])]:['tenant','protectedControlPath','vaultPath','recordId','namespace','expectedHead','profile','maxPayloadBytes'];
  if(Object.keys(data).some(k=>!allowed.includes(k)))fail('invalid_configuration');tenant(data.tenant);
  for(const key of ['recordId','namespace',...(creating?['operationId','requestId']:[])]){
    if(!Array.isArray(data[key])||data[key].length!==16||data[key].some(v=>!Number.isInteger(v)||v<0||v>255))fail('invalid_transport');
  }
  if('profile' in data&&typeof data.profile!=='string')fail('invalid_configuration');
  if('maxPayloadBytes' in data&&(!Number.isSafeInteger(data.maxPayloadBytes)||data.maxPayloadBytes<64||data.maxPayloadBytes>MAX))fail('invalid_configuration');
  if('expectedHead' in data&&(data.expectedHead===null||typeof data.expectedHead.revision!=='string'||!/^(0|[1-9][0-9]*)$/.test(data.expectedHead.revision)))fail('invalid_head');
  if(mode==='importAuthenticatedBundle'){if(approved!==true)fail('approval_required');data.approved=true;if(!data.sourceHead||typeof data.sourceHead.revision!=='string')fail('invalid_head');}
  const password=bytes(originalPassword,undefined,1024);let source;
  try{if(password.length>1024)fail('resource_limit');if(mode==='importAuthenticatedBundle')source=bytes(bundle);return new NativeWorkspace(internal,source?NativeOwner[mode](serialized(data),password,source):NativeOwner[mode](serialized(data),password),data.tenant);}catch(e){throw error(e);}finally{password.fill(0);source?.fill(0);}
}
export class NativeWorkspace {
  constructor(token,raw,tenant){if(token!==internal)fail('invalid_owner');owners.set(this,{raw,tenant});Object.freeze(this);}
  static createNew(options){return factory(options,'createNew');}
  static openExisting(options){return factory(options,'openExisting');}
  static importAuthenticatedBundle(options,bundle,approved){return factory(options,'importAuthenticatedBundle',bundle,approved);}
  static reconcileExisting(options,requestId){if(!options||typeof options!=='object'||types.isProxy(options))fail('invalid_configuration');const keys=Reflect.ownKeys(Object.getOwnPropertyDescriptors(options));if(keys.some(k=>!['tenant','protectedControlPath','vaultPath','recordId','namespace','profile','maxPayloadBytes'].includes(k)))fail('invalid_configuration');const data=plain(options);tenant(data.tenant);try{return JSON.parse(NativeOwner.reconcileExisting(serialized(data),bytes(requestId,16)));}catch(e){throw error(e);}}
  head(t){return call(this,t,'head');}
  counts(t){return call(this,t,'counts');}
  historyCoverage(t){return call(this,t,'historyCoverage');}
  policy(t){return call(this,t,'policy');}
  operationsPage(t,offset,limit){return call(this,t,'operationsPage',{offset,limit});}
  review(t,input){return call(this,t,'review',input);}
  prepareReview(t,inputs,operationId){return stage(this,t,'prepareReview',{inputs,operationId:id(operationId)});}
  prepareConversationSend(t,inputs,operation){return stage(this,t,'prepareConversationSend',{inputs,operation});}
  prepareObservedSend(t,intent,logicalId,surface,siteId,operation){return stage(this,t,'prepareObservedSend',{intent,logicalId:id(logicalId),surface,siteId,operation});}
  prepareDefinitelyNotSent(t,intent,logicalId,operation){return stage(this,t,'prepareDefinitelyNotSent',{intent,logicalId:id(logicalId),operation});}
  preparePolicyChanges(t,edits,operationId){return stage(this,t,'preparePolicyChanges',{edits,operationId:id(operationId)});}
  prepareRecovery(t,operationId){return stage(this,t,'prepareRecovery',id(operationId));}
  commit(t,change,approved,requestId){
    if(approved!==true)fail('approval_required');
    const c=changes.get(change);if(!c||c.owner!==this)fail('foreign_change');
    const result=call(this,t,'commit',{handle:c.handle,approved:true,requestId:id(requestId)});
    abandoned.unregister(change);changes.delete(change);return result;
  }
  reconcile(t,requestId){return call(this,t,'reconcile',id(requestId));}
  restore(t,text){return call(this,t,'restore',text);}
  restoreReport(t,text){return call(this,t,'restoreReport',text);}
  restoreInConversation(t,text,logicalId){return call(this,t,'restoreInConversation',{text,logicalId:id(logicalId)});}
  restoreReportInConversation(t,text,logicalId){return call(this,t,'restoreReportInConversation',{text,logicalId:id(logicalId)});}
  bufferedRestore(t,encoding='utf8',logicalId){return stream(this,t,false,encoding,logicalId);}
  progressiveRestore(t,encoding='utf8',logicalId){return stream(this,t,true,encoding,logicalId);}
  lock(t){return call(this,t,'lock');}
  close(t){return call(this,t,'close');}
}
function stage(owner,t,method,args){const {handle}=call(owner,t,method,args);return new PreparedChange(internal,owner,handle);}
export class PreparedChange {
  constructor(token,owner,handle){if(token!==internal)fail('foreign_change');changes.set(this,{owner,handle});abandoned.register(this,{owner,handle,method:'discard'},this);Object.freeze(this);}
  #call(t,method){const c=changes.get(this);if(!c)fail('inactive_plan');return call(c.owner,t,method,c.handle);}
  previews(t){return this.#call(t,'previews');}
  heads(t){return this.#call(t,'planHeads');}
  sendIntents(t){return this.#call(t,'sendIntents');}
  snapshotFormat(t){return this.#call(t,'snapshotFormat');}
  policyPreview(t){return this.#call(t,'policyPreview');}
  discard(t){this.#call(t,'discard');abandoned.unregister(this);changes.delete(this);}
}
function stream(owner,t,progressive,encoding,logicalId){const data={progressive,encoding};if(logicalId!==undefined)data.logicalId=id(logicalId);const{handle}=call(owner,t,'stream',data);return new RestoreStream(internal,owner,handle,encoding);}
export class RestoreStream {
  constructor(token,owner,handle,encoding){if(token!==internal)fail('inactive_stream');streams.set(this,{owner,handle,encoding});abandoned.register(this,{owner,handle,method:'streamClose'},this);Object.freeze(this);}
  #call(t,method,data={}){const s=streams.get(this);if(!s)fail('inactive_stream');return call(s.owner,t,method,{handle:s.handle,...data});}
  feed(t,value){
    const s=streams.get(this);if(!s)fail('inactive_stream');
    this.#call(t,'streamCheck');
    let copy;
    try{
      if(s.encoding==='utf8')copy=bytes(value);
      else{if(!(value instanceof Uint16Array))fail('invalid_transport');safeTyped(value,Uint16Array.prototype);if(value.byteLength>MAX)fail('resource_limit');copy=Buffer.alloc(value.byteLength);for(let i=0;i<value.length;i++)copy.writeUInt16LE(value[i],i*2);}
    }catch(e){try{this.#call(t,'streamPoison',{utf16:s.encoding==='utf16'});}catch(failure){throw failure;}throw error(e);}
    try{return JSON.parse(state(s.owner).raw.feed(t,s.handle,copy,s.encoding==='utf16'));}catch(e){throw error(e);}finally{copy.fill(0);}
  }
  finalize(t){return this.#call(t,'streamFinalize');}
  reset(t){return this.#call(t,'streamReset');}
  close(t){this.#call(t,'streamClose');abandoned.unregister(this);streams.delete(this);}
}
