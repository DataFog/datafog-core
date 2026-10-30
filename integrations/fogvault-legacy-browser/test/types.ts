import {LegacyBackupImport} from '@datafog/core-fogvault-legacy-browser';
import {Workspace} from '@datafog/wasm';
import type {FogVault} from '@datafog/fogvault-browser';
async function example(vault:FogVault,archive:Uint8Array){
 const session=await LegacyBackupImport.open({Workspace,archive,credential:'synthetic'});
 const metadata=session.preview();const revision:string=metadata.head.revision;
 // @ts-expect-error no plaintext bundle export
 session.bundle;
 // @ts-expect-error only literal true approves
 await session.commit({vault,recordId:new Uint8Array(16),operationId:new Uint8Array(16),requestId:new Uint8Array(16),approved:false});
 await session.close();return revision;
}
void example;
