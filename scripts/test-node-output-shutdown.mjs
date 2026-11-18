import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const args = process.argv.slice(2);
const child = args[0] === "--child";
if (child) args.shift();
const options = {};
for (let index = 0; index < args.length; index += 2) {
  assert(["--module", "--runs", "--iterations", "--output"].includes(args[index]));
  assert(args[index + 1] && !options[args[index]], "invalid or duplicate argument");
  options[args[index]] = args[index + 1];
}
assert(options["--module"], "--module requires an installed package entry point");
const modulePath = path.resolve(options["--module"]);
const iterations = Number(options["--iterations"] ?? 20_000);
const runs = Number(options["--runs"] ?? 10);
assert(Number.isSafeInteger(iterations) && iterations > 0);
assert(Number.isSafeInteger(runs) && runs > 0);

if (child) {
  const api = await import(pathToFileURL(modulePath).href);
  const workspace = api.Workspace.create(
    new Uint8Array(16).fill(1),
    new Uint8Array(16).fill(2),
  );
  try {
    for (let index = 0; index < iterations; index++) {
      const namespace = workspace.namespace();
      assert(namespace instanceof Uint8Array && namespace.length === 16);
      assert(namespace.every(value => value === 1));
    }
  } finally {
    workspace.close();
  }
  console.log("Public Node output burst completed; exiting naturally.");
} else {
  const results = [];
  for (let index = 0; index < runs; index++) {
    const result = spawnSync(
      process.execPath,
      [fileURLToPath(import.meta.url), "--child", "--module", modulePath, "--iterations", String(iterations)],
      { encoding: "utf8", maxBuffer: 1024 * 1024, timeout: 60_000 },
    );
    const completed = result.stdout?.includes("Public Node output burst completed; exiting naturally.") === true;
    results.push({
      index, pid: result.pid, exitCode: result.status, signal: result.signal,
      completed, stdout: result.stdout, stderr: result.stderr,
      error: result.error?.message ?? null,
      pass: !result.error && result.status === 0 && completed,
    });
  }
  const report = {
    schema: "datafog-node-output-shutdown/v1", modulePath,
    node: process.version, platform: process.platform, arch: process.arch,
    runs, iterations, pass: results.every(result => result.pass), results,
  };
  if (options["--output"]) writeFileSync(options["--output"], JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify({ pass: report.pass, runs, iterations, failed: results.filter(result => !result.pass).length }));
  if (!report.pass) {
    for (const result of results.filter(result => !result.pass)) console.error(JSON.stringify(result));
    process.exitCode = 1;
  }
}
