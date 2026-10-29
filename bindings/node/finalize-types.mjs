// NAPI emits its transport class alongside public declarations. The package
// exports only the checked Workspace wrapper, whose plans are opaque objects.
import {readFile, writeFile} from "node:fs/promises";
const file = new URL("./index.d.ts", import.meta.url);
const source = await readFile(file, "utf8");
const rawClass = /^export declare class NativeWorkspace \{\n[\s\S]*?^\}\n/gm;
if ([...source.matchAll(rawClass)].length !== 1) throw new Error("native workspace declaration generation changed");
await writeFile(file, source.replace(rawClass, ""));
