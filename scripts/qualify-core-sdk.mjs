#!/usr/bin/env node
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdir, readFile, readdir, writeFile} from "node:fs/promises";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {MODEL} from "../bindings/wasm/models/model.mjs";
import {validateCoreSdkQualification} from "./core-sdk-qualification.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "target/core-sdk-qualification", new Date().toISOString().replaceAll(/[:.]/g, "-"));
await mkdir(output, {recursive: true});
// Include working-tree sources, including new files, because local release
// candidates can be deliberately uncommitted. Generated outputs are excluded.
async function sourceHashes() {
  const files = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".nvmrc"];
  async function visit(directory) {
    for (const entry of await readdir(path.join(root, directory), {withFileTypes: true})) {
      if (["node_modules", "target", "dist", "output", "__pycache__"].includes(entry.name)) continue;
      const next = path.posix.join(directory, entry.name);
      if (entry.isDirectory()) await visit(next);
      else if (entry.isFile()) files.push(next);
    }
  }
  for (const directory of ["crates", "bindings/wasm", "examples/workspace-extension", "scripts", "fixtures"]) await visit(directory);
  const result = {};
  for (const file of files.sort()) result[file] = createHash("sha256").update(await readFile(path.join(root, file))).digest("hex");
  return result;
}
const sources = await sourceHashes();
await writeFile(path.join(output, "source-inputs.json"), JSON.stringify(sources, null, 2) + "\n");
execFileSync(process.execPath, ["scripts/qualify-browser-detection.mjs"], {cwd: root, stdio: "inherit",
  env: {...process.env, DATAFOG_CORE_SDK_QUALIFICATION: "1", DATAFOG_QUALIFICATION_OUTPUT: output}});
assert.deepEqual(await sourceHashes(), sources, "Sources changed during qualification; rerun against a frozen candidate");
const receipt = JSON.parse(await readFile(path.join(output, "receipt.json")));
const corpus = JSON.parse(await readFile(path.join(root, "fixtures/browser-detection/qualification-v1.json")));
const sdkPackage = JSON.parse(await readFile(path.join(root, "bindings/wasm/package.json")));
assert.equal(receipt.versions["@datafog/wasm"], sdkPackage.version, "Qualification installed a different SDK version");
const result = validateCoreSdkQualification(receipt, corpus, MODEL);
result.sdkVersion = sdkPackage.version;
result.sourceInputsSha256 = createHash("sha256").update(await readFile(path.join(output, "source-inputs.json"))).digest("hex");
result.receiptSha256 = createHash("sha256").update(await readFile(path.join(output, "receipt.json"))).digest("hex");
await writeFile(path.join(output, "core-sdk-gate.json"), JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify({pass: result.passed, executed: true, report: path.join(output, "core-sdk-gate.json")}));
