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
import { DataFogError, PrivacyManager, scan, scanAndTransform, transform, scanStructured, discoverFields, transformStructured, scanAndTransformStructured } from "@datafog/node";

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
  assert.equal(
    text.slice(finding.utf16Range.start, finding.utf16Range.end),
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

const structuredRecords = readFileSync(path.join(fixturesDirectory, "structured.jsonl"), "utf8").trim().split("\\n").map(JSON.parse);

function pointerValue(data, pointer) {
  return pointer.slice(1).split("/").reduce((value, key) => value[key.replaceAll("~1", "/").replaceAll("~0", "~")], data);
}
for (const record of structuredRecords) {
  const result = scanStructured(record.data, record.config);
  const mappings = result.mappings.map(m => ({path:m.path, entity_type:m.entityType, source:m.source, rule:m.rule}));
  if (JSON.stringify(mappings) !== JSON.stringify(record.mappings)) throw new Error("structured mappings: " + record.id);
  if (JSON.stringify(discoverFields(record.data, record.config)) !== JSON.stringify(result.mappings)) throw new Error("discovery mismatch");
  const actual = result.findings.map(({path, finding}) => {
    verifyContract(pointerValue(record.data, path), finding);
    return {path, ...legacyProjection(finding)};
  });
  if (JSON.stringify(actual) !== JSON.stringify(record.findings)) throw new Error("structured findings: " + record.id);
}
const cycle = {}; cycle.self = cycle;
for (const input of [null, "secret-value", {n:NaN}, {n:Infinity}, {n:2 ** 53}, {n:1n}, {n:undefined}, {n:new Date()}, {n:new Map()}, {n:[,]}, cycle]) {
  let failed = false;
  try { scanStructured(input); } catch (error) {
    failed = error instanceof DataFogError && error.code === "invalid_configuration" && error.path === "/data" && !error.message.includes("secret-value");
  }
  if (!failed) throw new Error("invalid structured input accepted");
}

const structuredTransformRecords = readFileSync(path.join(fixturesDirectory, "structured-transform.jsonl"), "utf8").trim().split("\\n").map(JSON.parse);

for (const record of structuredTransformRecords) {
  const result = scanAndTransformStructured(record.data, record.config);
  const explicit = transformStructured(record.data, scanStructured(record.data, record.config.scan).findings, record.config.transform);
  // Compare recursively without depending on object insertion order.
  const ordered = value => Array.isArray(value) ? value.map(ordered) : value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map(k => [k,ordered(value[k])])) : value;
  if (JSON.stringify(ordered(result.data)) !== JSON.stringify(ordered(record.expected_data))) throw new Error("structured transform: " + record.id);
  if (JSON.stringify(result) !== JSON.stringify(explicit)) throw new Error("structured explicit mismatch");
  for (const {path, transformation:t} of result.transformations) {
    const source = pointerValue(record.data, path);
    const output = pointerValue(result.data, path);
    if (output.slice(t.outputUtf16Range.start,t.outputUtf16Range.end) !== t.replacement) throw new Error("structured output range");
    if (!source.slice(t.sourceUtf16Range.start,t.sourceUtf16Range.end)) throw new Error("structured source range");
    if ("matchedText" in t) throw new Error("structured record echoes plaintext");
  }
}

const denseData = {a:"👋 may@example.test ".repeat(80), b:"中 é other@example.test ".repeat(80)};
const denseFindings = scanStructured(denseData).findings;
assert.equal(denseFindings.length, 160);
for (const {path, finding} of denseFindings) verifyContract(pointerValue(denseData, path), finding);
for (const strategy of [{strategy:"redact"}, {strategy:"remove"}, {strategy:"mask",character:"🔒"}]) {
  const config = {default:strategy};
  const result = transformStructured(denseData, [...denseFindings].reverse(), config);
  assert.deepEqual(result, scanAndTransformStructured(denseData, {transform:config}));
  assert.equal(result.transformations.length, 160);
  for (const {path, transformation:t} of result.transformations) {
    const source = pointerValue(denseData, path);
    const output = pointerValue(result.data, path);
    const expectedSource = path === "/a" ? "may@example.test" : "other@example.test";
    assert.equal(source.slice(t.sourceUtf16Range.start,t.sourceUtf16Range.end), expectedSource);
    assert.equal(Array.from(output).slice(t.outputCodepointRange.start,t.outputCodepointRange.end).join(""), t.replacement);
    assert.equal(output.slice(t.outputUtf16Range.start,t.outputUtf16Range.end), t.replacement);
    assert.equal(Buffer.from(output).subarray(t.outputByteRange.start,t.outputByteRange.end).toString(), t.replacement);
  }
}

const emojiFinding = scan("👋 jane@example.com")[0];
assert.deepEqual(emojiFinding.byteRange, { start: 5, end: 21 });
assert.deepEqual(emojiFinding.codepointRange, { start: 2, end: 18 });
assert.deepEqual(emojiFinding.utf16Range, { start: 3, end: 19 });
const { utf16Range: _derivedRange, ...preSliceEightFinding } = emojiFinding;
assert.equal(
  transform("👋 jane@example.com", [preSliceEightFinding], {
    default: { strategy: "redact" },
  }).text,
  "👋 [EMAIL]",
);
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
assert.deepEqual(explicit.transformations[0].sourceUtf16Range, { start: 3, end: 19 });
assert.deepEqual(explicit.transformations[0].outputUtf16Range, { start: 3, end: 10 });
for (const record of explicit.transformations) {
  assert.equal(
    transformText.slice(record.sourceUtf16Range.start, record.sourceUtf16Range.end),
    "jane@example.com",
  );
  assert.equal(
    explicit.text.slice(record.outputUtf16Range.start, record.outputUtf16Range.end),
    record.replacement,
  );
}
assert.equal("finding" in explicit.transformations[0], false);
assert.equal("matchedText" in explicit.transformations[0], false);
assert.equal(explicit.transformations[0].entityType, "EMAIL");
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

const pseudonymConfig = {
  default: {
    strategy: "pseudonymize",
    key_ref: "customers/email",
    key_version: "7",
  },
};
assert.throws(
  () => transform(transformText, scan(transformText), pseudonymConfig),
  (error) =>
    error instanceof DataFogError &&
    error.code === "key_provider_required" &&
    error.path === "/default/key_ref",
);
const providerCalls = [];
const manager = new PrivacyManager({
  async resolveKey(request) {
    providerCalls.push(request);
    return { key: Uint8Array.from({ length: 32 }, (_, index) => index), resolvedVersion: "7" };
  },
});
const pseudonymized = await manager.scanAndTransform(
  "jane@example.com jane@example.com",
  { transform: pseudonymConfig },
);
const expectedToken = "lIdYiXR1nTA9XURAF5GmA62F/aknbUP3Q2B31wnZ2hA=";
assert.equal(pseudonymized.text, expectedToken + " " + expectedToken);
assert.deepEqual(providerCalls, [{ keyRef: "customers/email", keyVersion: "7" }]);
for (const record of pseudonymized.transformations) {
  assert.equal(record.replacement, expectedToken);
  assert.equal(record.keyRef, "customers/email");
  assert.equal(record.resolvedKeyVersion, "7");
  assert.equal("finding" in record, false);
  assert.equal("matchedText" in record, false);
}
await assert.rejects(
  new PrivacyManager({
    async resolveKey() {
      return { key: new Uint8Array(31), resolvedVersion: "7" };
    },
  }).scanAndTransform("Email jane@example.com", { transform: pseudonymConfig }),
  (error) =>
    error instanceof DataFogError &&
    error.code === "invalid_key_material" &&
    error.path === "/transform/default/key_ref",
);


const structuredPseudonyms = await manager.scanAndTransformStructured({first_name:"May",last_name:"May"}, {transform:pseudonymConfig});
assert.equal(structuredPseudonyms.data.first_name, structuredPseudonyms.data.last_name);
assert.notEqual(structuredPseudonyms.data.first_name,"May");
assert.equal(providerCalls.length,2);
const invalidContextData = {first_name:"May"};
await assert.rejects(
  manager.transformStructured(invalidContextData, scanStructured(invalidContextData).findings, pseudonymConfig, {scope:""}),
  error => error instanceof DataFogError && error.code === "invalid_configuration",
);
await assert.rejects(
  manager.scanAndTransformStructured(invalidContextData, {transform:pseudonymConfig}, {scope:""}),
  error => error instanceof DataFogError && error.code === "invalid_configuration",
);
assert.equal(providerCalls.length,2, "invalid structured context must fail before resolving keys");
const mutableData = {first_name:"May"};
const mutableConfig = {transform:{default:{strategy:"pseudonymize",key_ref:"names"}}};
const snapshotManager = new PrivacyManager({async resolveKey() {
  mutableData.first_name="changed";
  mutableConfig.transform.default.strategy="remove";
  return {key: new Uint8Array(32),resolvedVersion:"v1"};
}});
const snapshotResult = await snapshotManager.scanAndTransformStructured(mutableData,mutableConfig);
assert.notEqual(snapshotResult.data.first_name,"");
assert.notEqual(snapshotResult.data.first_name,"changed");
assert.equal(snapshotResult.transformations[0].transformation.strategy,"pseudonymize");
const tokenRecords = new Map();
let tokenCounter = 0;
const tokenProvider = {
  async tokenizeBatch(scope, items) {
    return items.map((item) => {
      const payload = Uint8Array.of(++tokenCounter);
      tokenRecords.set(payload[0], {
        scope,
        tokenRef: item.tokenRef,
        version: "active-1",
        value: item.exactValue,
      });
      return { id: item.id, payload, resolvedVersion: "active-1" };
    });
  },
  async restoreBatch(scope, items) {
    return items.map((item) => {
      const record = tokenRecords.get(item.payload[0]);
      if (!record || record.scope !== scope || record.tokenRef !== item.tokenRef || record.version !== item.resolvedVersion) {
        const error = new Error("denied");
        error.code = "token_access_denied";
        throw error;
      }
      return { id: item.id, value: record.value };
    });
  },
};
const tokenManager = new PrivacyManager({ tokenProvider });
const tokenContext = { scope: "tenant/α" };
const tokenized = await tokenManager.scanAndTransform(
  "👋 jane@example.com jane@example.com",
  { transform: { default: { strategy: "tokenize", token_ref: "customers/default" } } },
  tokenContext,
);
assert.notEqual(tokenized.transformations[0].replacement, tokenized.transformations[1].replacement);
assert.equal(tokenized.transformations[0].tokenRef, "customers/default");
assert.equal(tokenized.transformations[0].resolvedTokenVersion, "active-1");

const structuredOriginal = {users:[{first_name:"👋 José"},{full_name:"May"}], count:2};
const structuredTokens = await tokenManager.scanAndTransformStructured(structuredOriginal, {transform:{default:{strategy:"tokenize",token_ref:"names"}}}, tokenContext);
const structuredRestored = await tokenManager.restoreStructured(structuredTokens.data, tokenContext);
assert.deepEqual(structuredRestored.data, structuredOriginal);
assert.equal(structuredRestored.restorations.length,2);
await assert.rejects(tokenManager.restoreStructured(structuredTokens.data,{scope:"wrong"}), e => e.code === "token_access_denied");
const invalidStructured = scanStructured(structuredOriginal).findings;
invalidStructured[1].finding.byteRange.end = 999;
const callsBefore = tokenCounter;
await assert.rejects(tokenManager.transformStructured(structuredOriginal,invalidStructured,{default:{strategy:"tokenize",token_ref:"names"}},tokenContext), e => e.code === "invalid_finding" && e.findingIndex === 1);
assert.equal(tokenCounter,callsBefore);
const restored = await tokenManager.restore(tokenized.text, tokenContext);
assert.equal(restored.text, "👋 jane@example.com jane@example.com");
assert.equal(restored.restorations.length, 2);
for (const record of restored.restorations) {
  assert.ok(
    tokenized.text
      .slice(record.sourceUtf16Range.start, record.sourceUtf16Range.end)
      .startsWith("DFTOKENv1("),
  );
  assert.equal(
    restored.text.slice(record.outputUtf16Range.start, record.outputUtf16Range.end),
    "jane@example.com",
  );
}
await assert.rejects(
  tokenManager.restore(tokenized.text, { scope: "tenant/b" }),
  (error) => error instanceof DataFogError && error.code === "token_access_denied",
);
await assert.rejects(
  tokenManager.restore("DFTOKENv2(3):abc", tokenContext),
  (error) => error instanceof DataFogError && error.code === "unsupported_token_version",
);

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

const denseTokens = await tokenManager.scanAndTransformStructured(denseData, {transform:{default:{strategy:"tokenize",token_ref:"dense"}}}, tokenContext);
const denseRestored = await tokenManager.restoreStructured(denseTokens.data, tokenContext);
assert.deepEqual(denseRestored.data, denseData);
assert.equal(denseRestored.restorations.length, 160);
for (const {path, restoration:r} of denseRestored.restorations) {
  const source = pointerValue(denseTokens.data, path);
  const output = pointerValue(denseRestored.data, path);
  assert.ok(source.slice(r.sourceUtf16Range.start,r.sourceUtf16Range.end).startsWith("DFTOKENv1("));
  assert.equal(output.slice(r.outputUtf16Range.start,r.outputUtf16Range.end), path === "/a" ? "may@example.test" : "other@example.test");
}

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
  PrivacyManager,
  type EntityType,
  type Finding,
  type FindingInput,
  type MaskRevealConfig,
  type KeyProvider,
  type ScanAndTransformConfig,
  type TextRange,
  type TransformationConfig,
  type TransformResult,
} from "@datafog/node";

const findings: Finding[] = scan("Email jane@example.com");
const suppliedFinding: FindingInput = findings[0];
const entityType: EntityType = findings[0]?.entityType ?? "CUSTOM_ENTITY";
const range: TextRange = findings[0]?.byteRange ?? { start: 0, end: 0 };
const utf16Range: TextRange = findings[0]?.utf16Range ?? { start: 0, end: 0 };
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
const provider: KeyProvider = {
  async resolveKey() {
    return { key: new Uint8Array(32), resolvedVersion: "1" };
  },
};
const pseudonymized: Promise<TransformResult> = new PrivacyManager(provider).transform(
  "Email jane@example.com",
  findings,
  { default: { strategy: "pseudonymize", key_ref: "customer/email" } },
);

void entityType;
void suppliedFinding;
void range;
void utf16Range;
void explicit;
void convenience;
void masked;
void pseudonymized;

import { discoverFields, scanStructured, transformStructured, scanAndTransformStructured, type StructuredScanResult, type StructuredTransformResult } from "@datafog/node";
const document = { first_name: "May", count: 1 };
const discovered = discoverFields(document, { mappings: { "/first_name": "PERSON" } });
const structured: StructuredScanResult = scanStructured(document);
const protectedDocument: StructuredTransformResult = transformStructured(document, structured.findings, maskConfig);
const protectedTogether: StructuredTransformResult = scanAndTransformStructured(document, { transform: maskConfig });
void discovered;
void protectedDocument;
void protectedTogether;
const structuredManagerResult: Promise<StructuredTransformResult> = new PrivacyManager(provider).transformStructured(document, structured.findings, maskConfig);
void structuredManagerResult;

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
