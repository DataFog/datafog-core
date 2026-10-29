// NAPI transport capabilities are private; only checked JS wrappers are public.
import {readFile, writeFile} from "node:fs/promises";
const file = new URL("./index.d.ts", import.meta.url);
let source = await readFile(file, "utf8");
for (const name of ["NativeWorkspace", "NativeRestoreStream"]) {
  const rawClass = new RegExp(`^export declare class ${name} \\{\n[\\s\\S]*?^\\}\n`, "gm");
  if ([...source.matchAll(rawClass)].length !== 1) throw new Error(`native ${name} declaration generation changed`);
  source = source.replace(rawClass, "");
}
await writeFile(file, source);
