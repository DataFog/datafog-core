// Run with UV_THREADPOOL_SIZE=1. Real model calls must progress while libuv is occupied.
import assert from 'node:assert/strict';
import {pbkdf2} from 'node:crypto';
import {promisify} from 'node:util';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const config=JSON.parse(readFileSync(process.argv[2],'utf8'));
const {ModelSession}=await import(pathToFileURL(process.argv[3]).href);
const session=await ModelSession.open(config);
// Fixed synthetic CPU work occupies the only libuv thread; it is not a product limit.
const blocker=promisify(pbkdf2)('synthetic-fixture','synthetic-salt',10_000_000,32,'sha256');
try {
  const first=await Promise.race([
    (async()=>{const result=await session.scan('John Smith lives at 123 Main Street.');
      assert(result.findings.length);
      await session.close();return 'model';})(),
    blocker.then(()=> 'libuv'),
  ]);
  assert.equal(first,'model','unrelated libuv work delayed model inference or cleanup');
} finally {await session.close();await blocker;}
console.log('Actual DataFog scan and cleanup progress independently of the saturated libuv pool.');
