import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {createReadStream} from "node:fs";
import {cp, mkdir, readFile, readdir, stat, writeFile} from "node:fs/promises";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {build} from "esbuild";
import {MODEL} from "../../bindings/wasm/models/model.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const output = path.join(here, "output/playwright", new Date().toISOString().replaceAll(/[:.]/g, "-"));
const consumer = path.join(output, "consumer");
const dist = path.join(here, "dist");
const source = path.resolve(process.env.DATAFOG_BROWSER_MODEL_DIR || path.join(root, "spike/gliner2-browser/models"));
async function digest(file) {
  const hash = createHash("sha256");
  for await (const part of createReadStream(file)) hash.update(part);
  return hash.digest("hex");
}
for (const [name, expected] of Object.entries(MODEL.files)) {
  const file = path.join(source, name);
  if ((await stat(file)).size !== expected.bytes || await digest(file) !== expected.sha256) {
    throw new Error(`Pinned model asset verification failed: ${name}`);
  }
}
execFileSync("npm", ["run", "build", "--prefix", here], {stdio: "inherit"});
await mkdir(consumer, {recursive: true});
await writeFile(path.join(consumer, "package.json"), JSON.stringify({private: true, type: "module"}));
const packed = JSON.parse(execFileSync("npm", ["pack", "--json", "--pack-destination", consumer], {
  cwd: path.join(root, "bindings/wasm"), encoding: "utf8",
}))[0];
const tarball = path.join(consumer, packed.filename);
execFileSync("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", tarball], {cwd: consumer, stdio: "inherit"});
const modules = path.join(consumer, "node_modules");
const sdk = path.join(modules, "@datafog/wasm");
const ort = path.join(modules, "onnxruntime-web");
const bundle = await build({
  entryPoints: [path.join(here, "src/model-page.js")],
  outdir: dist, bundle: true, splitting: true, format: "esm", platform: "browser",
  target: "chrome120", chunkNames: "model-[name]-[hash]", nodePaths: [modules],
  metafile: true, legalComments: "eof",
});
await cp(path.join(sdk, "dist"), path.join(dist, "dist"), {recursive: true});
await mkdir(path.join(dist, "ort"), {recursive: true});
const runtimeFiles = ["ort-wasm-simd-threaded.mjs", "ort-wasm-simd-threaded.wasm"];
for (const name of runtimeFiles) await cp(path.join(ort, "dist", name), path.join(dist, "ort", name));
for (const name of ["LICENSE", "ThirdPartyNotices.txt"]) {
  try { await cp(path.join(ort, name), path.join(dist, "ort", name)); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
}
for (const name of Object.keys(MODEL.files)) {
  const destination = path.join(dist, "models", name);
  await mkdir(path.dirname(destination), {recursive: true});
  await cp(path.join(source, name), destination);
}
await mkdir(path.join(dist, "bad-model"), {recursive: true});
await cp(path.join(source, "tokenizer.json"), path.join(dist, "bad-model/tokenizer.json"));
const corrupt = await readFile(path.join(source, "tokenizer_config.json"));
corrupt[0] ^= 1;
await writeFile(path.join(dist, "bad-model/tokenizer_config.json"), corrupt);
await cp(path.join(here, "src/model-page.html"), path.join(dist, "model-page.html"));
await writeFile(path.join(dist, "model-background.js"), 'chrome.action.onClicked.addListener(() => chrome.tabs.create({url: chrome.runtime.getURL("model-page.html")}));\n');
const manifest = JSON.parse(await readFile(path.join(dist, "manifest.json"), "utf8"));
manifest.name = "DataFog synthetic MV3 model proof";
manifest.background.service_worker = "model-background.js";
await writeFile(path.join(dist, "manifest.json"), JSON.stringify(manifest, null, 2));
const assets = {};
for (const name of [...Object.keys(MODEL.files).map(name => `models/${name}`), ...runtimeFiles.map(name => `ort/${name}`), "dist/datafog_wasm_bg.wasm"]) {
  const file = path.join(dist, name);
  assets[name] = {bytes: (await stat(file)).size, sha256: await digest(file)};
}
const versions = {};
for (const name of ["@datafog/wasm", "onnxruntime-web", "@huggingface/tokenizers"]) {
  versions[name] = JSON.parse(await readFile(path.join(modules, name, "package.json"), "utf8")).version;
}
async function files(directory, relative = "") {
  const names = [];
  for (const entry of await readdir(directory, {withFileTypes: true})) {
    const name = path.posix.join(relative, entry.name);
    if (entry.isDirectory()) names.push(...await files(path.join(directory, entry.name), name));
    else names.push(name);
  }
  return names;
}
const extensionFiles = {};
for (const name of await files(dist)) {
  extensionFiles[name] = assets[name] || {bytes: (await stat(path.join(dist, name))).size, sha256: await digest(path.join(dist, name))};
}
const report = {
  sourceCommit: execFileSync("git", ["rev-parse", "HEAD"], {cwd: root, encoding: "utf8"}).trim(),
  sourceDirty: Boolean(execFileSync("git", ["status", "--porcelain"], {cwd: root, encoding: "utf8"}).trim()),
  model: MODEL, versions, manifest, assets, output, dist,
  extensionFiles, extensionBytes: Object.values(extensionFiles).reduce((sum, file) => sum + file.bytes, 0),
  tarball: {file: tarball, bytes: (await stat(tarball)).size, sha256: await digest(tarball)},
  bundler: {name: "esbuild", version: JSON.parse(await readFile(path.join(here, "node_modules/esbuild/package.json"), "utf8")).version, outputs: Object.keys(bundle.metafile.outputs)},
};
await writeFile(path.join(dist, "model-artifacts.json"), JSON.stringify(report, null, 2));
await writeFile(path.join(output, "artifacts.json"), JSON.stringify(report, null, 2));
console.log(`Built candidate-tarball MV3 model proof: ${output}`);
