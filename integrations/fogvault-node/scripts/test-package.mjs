import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const result=spawnSync(process.execPath,[path.join(root,'test/run-installed.mjs'),'--output',path.join(root,'target/installed-report.json')],{stdio:'inherit'});
process.exit(result.status??1);
