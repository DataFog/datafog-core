import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
await build({entryPoints:[root+'kdf-worker-source.mjs'],bundle:true,platform:'browser',format:'esm',target:'es2022',outfile:root+'kdf-worker.mjs',legalComments:'eof'});
