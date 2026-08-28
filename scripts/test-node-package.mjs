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
import { DataFogError, scan, scanAndTransform, transform } from "@datafog/node";

const fixturesDirectory = process.argv[2];

assert.throws(() => scan(123), TypeError);

function legacyProjection(finding) {
  return {
    label: finding.entityType,
    text: finding.matchedText,
    start: finding.codepointRange.start,
    end: finding.codepointRange.end,
  };
}

function verifyContract(text, finding) {
  const bytes = Buffer.from(text, "utf8");
  assert.equal(
    bytes.subarray(finding.byteRange.start, finding.byteRange.end).toString("utf8"),
    finding.matchedText,
  );
  assert.equal(
    Array.from(text)
      .slice(finding.codepointRange.start, finding.codepointRange.end)
      .join(""),
    finding.matchedText,
  );
  assert.equal(finding.confidence, undefined);
  assert.ok(finding.detectorName.startsWith("datafog-core/"));
  assert.ok(finding.detectorVersion);
}

for (const name of ["development.jsonl", "final.jsonl"]) {
  const records = readFileSync(path.join(fixturesDirectory, name), "utf8")
    .split("\\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));

  for (const record of records) {
    const findings = scan(record.text);
    assert.deepEqual(
      findings.map(legacyProjection),
      record.entities,
      \`\${name}: \${record.id}\`,
    );
    findings.forEach((finding) => verifyContract(record.text, finding));
  }
}

const emojiFinding = scan("👋 jane@example.com")[0];
assert.deepEqual(emojiFinding.byteRange, { start: 5, end: 21 });
assert.deepEqual(emojiFinding.codepointRange, { start: 2, end: 18 });
assert.deepEqual(
  scan("Email jane@example.com", { locale: "en-US" }),
  scan("Email jane@example.com"),
);

const transformText = "👋 jane@example.com and jane@example.com";
const explicit = transform(transformText, scan(transformText), {
  default: { strategy: "redact" },
});
const convenience = scanAndTransform(transformText, {
  transform: { default: { strategy: "redact" } },
});
assert.deepEqual(explicit, convenience);
assert.equal(explicit.text, "👋 [EMAIL] and [EMAIL]");
assert.equal(explicit.transformations.length, 2);
assert.equal(explicit.transformations[0].replacement, "[EMAIL]");
assert.deepEqual(explicit.transformations[0].outputByteRange, { start: 5, end: 12 });
assert.deepEqual(explicit.transformations[0].outputCodepointRange, { start: 2, end: 9 });
assert.equal(
  scanAndTransform("Email jane@example.com", {
    transform: { default: { strategy: "mask" } },
  }).text,
  "Email ****************",
);
const partialMask = scanAndTransform("Email jane@example.com", {
  transform: {
    default: {
      strategy: "mask",
      character: "•",
      reveal: { direction: "last", count: 4 },
    },
  },
});
assert.equal(partialMask.text, "Email ••••••••••••.com");
assert.equal(partialMask.transformations[0].strategy, "mask");
assert.equal(partialMask.transformations[0].replacement, "••••••••••••.com");
assert.deepEqual(partialMask.transformations[0].outputByteRange, { start: 6, end: 46 });

assert.equal(
  scanAndTransform("Email jane@example.com", {
    transform: {
      default: {
        strategy: "mask",
        reveal: { direction: "first", count: 99 },
      },
    },
  }).text,
  "Email jane@example.com",
);

const removed = scanAndTransform("Email jane@example.com today", {
  transform: { default: { strategy: "remove" } },
});
assert.equal(removed.text, "Email  today");
assert.equal(removed.transformations[0].strategy, "remove");
assert.equal(removed.transformations[0].replacement, "");
assert.deepEqual(removed.transformations[0].outputCodepointRange, { start: 6, end: 6 });

for (const invalidConfig of [
  { strategy: "mask", character: "" },
  { strategy: "mask", character: "**" },
  { strategy: "mask", character: " " },
  { strategy: "mask", unexpected: true },
  { strategy: "remove", character: "*" },
  { strategy: "mask", reveal: { direction: "last", count: -1 } },
  { strategy: "mask", reveal: { direction: "middle", count: 4 } },
]) {
  assert.throws(
    () => scanAndTransform("Email jane@example.com", {
      transform: { default: invalidConfig },
    }),
    DataFogError,
  );
}
assert.throws(
  () => transform(
    transformText,
    [{ ...scan(transformText)[0], confidence: 2 }],
    { default: { strategy: "redact" } },
  ),
  (error) =>
    error instanceof DataFogError &&
    error.code === "invalid_finding" &&
    error.reason === "invalid_confidence" &&
    error.path === "/findings/0/confidence" &&
    error.findingIndex === 0,
);

const selected = scanAndTransform(
  "Email support@example.com or call (212) 555-0100",
  {
    scan: { locale: "en-US" },
    transform: {
      default: { strategy: "redact" },
      entities: ["EMAIL", "PHONE"],
      overrides: {
        PHONE: {
          strategy: "mask",
          reveal: { direction: "last", count: 4 },
        },
      },
      allow: {
        exact: { EMAIL: ["support@example.com"] },
        regex: {},
      },
    },
  },
);
assert.equal(selected.text, "Email support@example.com or call **********0100");
assert.equal(selected.transformations.length, 1);

assert.throws(
  () => transform(transformText, scan(transformText), {
    default: { strategy: "redact" },
    overides: {},
  }),
  (error) =>
    error instanceof DataFogError &&
    error.reason === "unknown_field" &&
    error.path === "/overides",
);

console.log("Installed @datafog/node package matches fixtures and transform contracts.");
`.trimStart(),
  );

  writeFileSync(
    path.join(temporaryDirectory, "type-smoke.ts"),
    `
import {
  scan,
  scanAndTransform,
  transform,
  type EntityType,
  type Finding,
  type MaskRevealConfig,
  type ScanAndTransformConfig,
  type TextRange,
  type TransformationConfig,
  type TransformResult,
} from "@datafog/node";

const findings: Finding[] = scan("Email jane@example.com");
const entityType: EntityType = findings[0]?.entityType ?? "CUSTOM_ENTITY";
const range: TextRange = findings[0]?.byteRange ?? { start: 0, end: 0 };
const explicit: TransformResult = transform(
  "Email jane@example.com",
  findings,
  { default: { strategy: "redact" } },
);
const convenience: TransformResult = scanAndTransform(
  "Email jane@example.com",
  { transform: { default: { strategy: "redact" } } },
);
const reveal: MaskRevealConfig = { direction: "last", count: 4 };
const maskConfig: TransformationConfig = {
  default: {
    strategy: "mask",
    character: "•",
    reveal,
  },
};
const combined: ScanAndTransformConfig = { transform: maskConfig };
const masked: TransformResult = scanAndTransform("Email jane@example.com", combined);

void entityType;
void range;
void explicit;
void convenience;
void masked;
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
