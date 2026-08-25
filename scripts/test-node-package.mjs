import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const nodePackage = path.join(root, "bindings", "node");
const fixturesDirectory = path.join(root, "fixtures");
const temporaryDirectory = mkdtempSync(
  path.join(os.tmpdir(), "datafog-node-package-"),
);

function run(command, arguments_, cwd) {
  execFileSync(command, arguments_, { cwd, stdio: "inherit" });
}

function fixtureRecords(name) {
  return readFileSync(path.join(fixturesDirectory, name), "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function writeConsumerTest() {
  writeFileSync(
    path.join(temporaryDirectory, "test-installed.mjs"),
    `
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { scan } from "@datafog/node";

const fixturesDirectory = process.argv[2];

assert.throws(() => scan(123), TypeError);

for (const name of ["development.jsonl", "final.jsonl"]) {
  const records = readFileSync(path.join(fixturesDirectory, name), "utf8")
    .split("\\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));

  for (const record of records) {
    assert.deepEqual(scan(record.text), record.entities, \`\${name}: \${record.id}\`);
  }
}

console.log("Installed @datafog/node package matches both fixtures.");
`.trimStart(),
  );

  writeFileSync(
    path.join(temporaryDirectory, "type-smoke.ts"),
    `
import { scan, type Entity, type Label } from "@datafog/node";

const entities: Entity[] = scan("Email jane@example.com");
const label: Label = entities[0]?.label ?? "EMAIL";

void label;
`.trimStart(),
  );

  writeFileSync(
    path.join(temporaryDirectory, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          module: "NodeNext",
          moduleResolution: "NodeNext",
          noEmit: true,
          strict: true,
        },
      },
      null,
      2,
    ),
  );
}

let tarball;

try {
  run("npm", ["run", "build"], nodePackage);

  const packed = JSON.parse(
    execFileSync("npm", ["pack", "--json"], {
      cwd: nodePackage,
      encoding: "utf8",
    }),
  );
  tarball = path.join(nodePackage, packed[0].filename);

  const nodePackageJson = JSON.parse(
    readFileSync(path.join(nodePackage, "package.json"), "utf8"),
  );

  writeFileSync(
    path.join(temporaryDirectory, "package.json"),
    JSON.stringify(
      {
        name: "datafog-node-consumer-test",
        private: true,
        type: "module",
      },
      null,
      2,
    ),
  );

  writeConsumerTest();

  run(
    "npm",
    [
      "install",
      "--ignore-scripts",
      tarball,
      `typescript@${nodePackageJson.devDependencies.typescript}`,
    ],
    temporaryDirectory,
  );

  run(
    process.execPath,
    ["test-installed.mjs", fixturesDirectory],
    temporaryDirectory,
  );

  run(
    path.join(temporaryDirectory, "node_modules", ".bin", "tsc"),
    ["--project", "tsconfig.json"],
    temporaryDirectory,
  );
} finally {
  if (tarball) {
    rmSync(tarball, { force: true });
  }
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
