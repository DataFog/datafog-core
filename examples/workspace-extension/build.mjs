import { build } from "esbuild";
import { cp, mkdir, rm, readFile, writeFile, readdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const dist = path.join(here, "dist");
const wasm = path.join(root, "bindings/wasm");
execFileSync("npm", ["run", "build", "--prefix", wasm], { stdio: "inherit" });
await rm(dist, { recursive: true, force: true });
await mkdir(path.join(dist, "vendor"), { recursive: true });
const tarball = JSON.parse(execFileSync("npm", ["pack", "--json", "--pack-destination", dist], { cwd: wasm, encoding: "utf8" }))[0].filename;
execFileSync("tar", ["-xzf", path.join(dist, tarball), "-C", path.join(dist, "vendor")]);
await rm(path.join(dist, tarball));
await cp(path.join(here, "src"), dist, { recursive: true });
await cp(path.join(root, "scripts/workspace-policy-conformance.mjs"), path.join(dist, "workspace-policy-conformance.mjs"));
await cp(path.join(root, "scripts/workspace-conformance.mjs"), path.join(dist, "workspace-conformance.mjs"));
await cp(path.join(root,"scripts/workspace-review-conformance.mjs"),path.join(dist,"workspace-review-conformance.mjs"));
await cp(path.join(root,"scripts/workspace-streaming-conformance.mjs"),path.join(dist,"workspace-streaming-conformance.mjs"));
await cp(path.join(root,"fixtures/workspace/streaming-contract-v1.json"),path.join(dist,"workspace-streaming.json"));
await build({
  entryPoints: [path.join(here, "src/kdf-worker.js")],
  bundle: true, platform: "browser", format: "esm", target: "chrome120",
  outfile: path.join(dist, "kdf-worker.js"), legalComments: "eof",
});
const manifest = {
  manifest_version: 3, name: "DataFog synthetic workspace proof", version: "0.0.1",
  description: "Local synthetic-only workspace integration harness; not the production extension.",
  action: { default_title: "Open synthetic workspace" },
  background: { service_worker: "background.js", type: "module" },
  content_security_policy: { extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; connect-src 'self'; worker-src 'self'" },
};
await writeFile(path.join(dist, "manifest.json"), JSON.stringify(manifest, null, 2));
async function files(directory, relative = "") {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const next = path.posix.join(relative, entry.name);
    if (entry.isDirectory()) result.push(...await files(path.join(directory, entry.name), next));
    else result.push(next);
  }
  return result;
}
const names = (await files(dist)).sort();
const hashes = Object.fromEntries(await Promise.all(names.map(async file => [file, createHash("sha256").update(await readFile(path.join(dist, file))).digest("hex")])));
const versions = {};
for (const name of ["playwright", "esbuild", "libsodium-wrappers-sumo", "libsodium-sumo"]) {
  versions[name] = JSON.parse(await readFile(path.join(here, "node_modules", name, "package.json"), "utf8")).version;
}
const inputPaths = ["Cargo.lock", "bindings/wasm/package-lock.json", "examples/workspace-extension/package-lock.json", "scripts/test-workspace-extension.mjs", "scripts/test-workspace-followups.mjs", "scripts/test-workspace-backup.mjs", "scripts/test-workspace-policy.mjs", "scripts/test-workspace-review.mjs", "scripts/workspace-conformance.mjs", "scripts/workspace-review-conformance.mjs", "scripts/workspace-policy-conformance.mjs", "scripts/workspace-streaming-conformance.mjs", "fixtures/workspace/streaming-contract-v1.json", "fixtures/workspace/monthly.json", "fixtures/workspace/counters.json", "fixtures/workspace/argon2id.json", "fixtures/workspace/argon2id-backup.json"];
const inputHashes = Object.fromEntries(await Promise.all(inputPaths.map(async file => [file, createHash("sha256").update(await readFile(path.join(root, file))).digest("hex")])));
await writeFile(path.join(dist, "artifacts.json"), JSON.stringify({
  coreBaseline: "0.4.4", sourceCommit: execFileSync("git", ["rev-parse", "HEAD"], {cwd: root, encoding: "utf8"}).trim(),
  sourceDirty: execFileSync("git", ["status", "--porcelain"], {cwd: root, encoding: "utf8"}).length > 0,
  node: process.version, rust: execFileSync("rustc", ["--version"], {encoding: "utf8"}).trim(), versions, hashes, inputHashes,
}, null, 2));
console.log("Built local synthetic workspace harness.");
