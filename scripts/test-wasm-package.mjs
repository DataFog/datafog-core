import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const wasmPackage = path.join(root, "bindings", "wasm");
const fixturesDirectory = path.join(root, "fixtures");
const temporaryDirectory = mkdtempSync(
  path.join(os.tmpdir(), "datafog-wasm-package-"),
);
const requireFromWasmPackage = createRequire(
  path.join(wasmPackage, "package.json"),
);
const { chromium } = requireFromWasmPackage("playwright");

function run(command, arguments_, cwd) {
  execFileSync(command, arguments_, { cwd, stdio: "inherit" });
}

function contentType(file) {
  if (file.endsWith(".html")) return "text/html; charset=utf-8";
  if (file.endsWith(".js") || file.endsWith(".mjs")) {
    return "text/javascript; charset=utf-8";
  }
  if (file.endsWith(".json") || file.endsWith(".jsonl")) {
    return "application/json; charset=utf-8";
  }
  if (file.endsWith(".wasm")) return "application/wasm";
  return "application/octet-stream";
}

function startServer(directory) {
  const server = createServer((request, response) => {
    const pathname = new URL(request.url, "http://localhost").pathname;
    const file = path.resolve(directory, `.${pathname === "/" ? "/index.html" : pathname}`);

    if (!file.startsWith(`${directory}${path.sep}`)) {
      response.writeHead(403).end();
      return;
    }

    try {
      response.writeHead(200, { "content-type": contentType(file) });
      response.end(readFileSync(file));
    } catch {
      response.writeHead(404).end();
    }
  });

  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      resolve({ server, url: `http://127.0.0.1:${address.port}` });
    });
  });
}

function writeConsumerFiles() {
  writeFileSync(
    path.join(temporaryDirectory, "package.json"),
    JSON.stringify(
      {
        name: "datafog-wasm-consumer-test",
        private: true,
        type: "module",
      },
      null,
      2,
    ),
  );

  writeFileSync(path.join(temporaryDirectory, "index.html"), "<!doctype html><title>WASM test</title>\n");

  writeFileSync(
    path.join(temporaryDirectory, "type-smoke.ts"),
    `
import {
  init,
  scan,
  scanAndTransform,
  restore,
  transform,
  type EntityType,
  type Finding,
  type FindingInput,
  type MaskRevealConfig,
  type ScanAndTransformConfig,
  type TextRange,
  type TransformationConfig,
  type TransformResult,
  type RestoreResult,
} from "@datafog/wasm";

const ready: Promise<void> = init();
const findings: Finding[] = scan("Email jane@example.com");
const suppliedFinding: FindingInput = findings[0];
const entityType: EntityType = findings[0]?.entityType ?? "CUSTOM_ENTITY";
const range: TextRange = findings[0]?.byteRange ?? { start: 0, end: 0 };
const utf16Range: TextRange = findings[0]?.utf16Range ?? { start: 0, end: 0 };
const transformed: TransformResult = transform(
  "Email jane@example.com",
  findings,
  { default: { strategy: "redact" } },
);
const scannedAndTransformed: TransformResult = scanAndTransform(
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
const unchanged: RestoreResult = restore("ordinary text", { scope: "tenant" });

void ready;
void entityType;
void suppliedFinding;
void range;
void utf16Range;
void transformed;
void scannedAndTransformed;
void masked;
void unchanged;

import { discoverFields, scanStructured, transformStructured, scanAndTransformStructured, type StructuredScanResult, type StructuredTransformResult } from "@datafog/wasm";
const document = { first_name: "May", count: 1 };
const discovered = discoverFields(document, { mappings: { "/first_name": "PERSON" } });
const structured: StructuredScanResult = scanStructured(document);
const protectedDocument: StructuredTransformResult = transformStructured(document, structured.findings, maskConfig);
const protectedTogether: StructuredTransformResult = scanAndTransformStructured(document, { transform: maskConfig });
void discovered;
void protectedDocument;
void protectedTogether;

`.trimStart(),
  );

  writeFileSync(
    path.join(temporaryDirectory, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          module: "ESNext",
          moduleResolution: "Bundler",
          noEmit: true,
          strict: true,
          target: "ES2022",
        },
      },
      null,
      2,
    ),
  );
}

let tarball;
let server;
let browser;

try {
  run("npm", ["run", "build"], wasmPackage);

  const packed = JSON.parse(
    execFileSync("npm", ["pack", "--json"], {
      cwd: wasmPackage,
      encoding: "utf8",
    }),
  );
  tarball = path.join(wasmPackage, packed[0].filename);

  const packageJson = JSON.parse(
    readFileSync(path.join(wasmPackage, "package.json"), "utf8"),
  );

  writeConsumerFiles();

  run(
    "npm",
    [
      "install",
      "--ignore-scripts",
      tarball,
      `typescript@${packageJson.devDependencies.typescript}`,
    ],
    temporaryDirectory,
  );

  run(
    path.join(temporaryDirectory, "node_modules", ".bin", "tsc"),
    ["--project", "tsconfig.json"],
    temporaryDirectory,
  );

  for (const fixture of ["development.jsonl", "final.jsonl", "structured.jsonl", "structured-transform.jsonl"]) {
    writeFileSync(
      path.join(temporaryDirectory, fixture),
      readFileSync(path.join(fixturesDirectory, fixture)),
    );
  }

  const serverInfo = await startServer(temporaryDirectory);
  server = serverInfo.server;
  browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(serverInfo.url);

  await page.evaluate(async () => {
    const { DataFogError, init, restore, scan, scanAndTransform, transform, scanStructured, discoverFields, transformStructured, scanAndTransformStructured, restoreStructured } = await import(
      "/node_modules/@datafog/wasm/index.js"
    );

    function expectThrows(callback, name) {
      try {
        callback();
      } catch (error) {
        if (error instanceof Error && error.name === name) {
          return;
        }
        throw new Error(`Expected ${name}, received ${error}`);
      }
      throw new Error(`Expected ${name} to be thrown`);
    }

    expectThrows(() => scan("Email jane@example.com"), "Error");
    await Promise.all([init(), init()]);
    expectThrows(() => scan(123), "TypeError");

    function legacyProjection(finding) {
      return {
        label: finding.entityType,
        text: finding.matchedText,
        start: finding.codepointRange.start,
        end: finding.codepointRange.end,
      };
    }

    function verifyContract(text, finding) {
      const bytes = new TextEncoder().encode(text);
      const matchedBytes = bytes.slice(finding.byteRange.start, finding.byteRange.end);
      const matchedText = new TextDecoder().decode(matchedBytes);
      if (matchedText !== finding.matchedText) {
        throw new Error("byte range does not select matched text");
      }

      const matchedCodepoints = Array.from(text)
        .slice(finding.codepointRange.start, finding.codepointRange.end)
        .join("");
      if (matchedCodepoints !== finding.matchedText) {
        throw new Error("code-point range does not select matched text");
      }
      if (
        text.slice(finding.utf16Range.start, finding.utf16Range.end) !==
        finding.matchedText
      ) {
        throw new Error("UTF-16 range does not select matched text");
      }
      if (finding.confidence !== undefined) {
        throw new Error("rule-based findings must omit confidence");
      }
      if (!finding.detectorName.startsWith("datafog-core/") || !finding.detectorVersion) {
        throw new Error("detector provenance is missing");
      }
    }

    for (const fixture of ["development.jsonl", "final.jsonl"]) {
      const source = await fetch(`/${fixture}`).then((response) => response.text());
      const records = source
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line));

      for (const record of records) {
        const findings = scan(record.text);
        if (
          JSON.stringify(findings.map(legacyProjection)) !==
          JSON.stringify(record.entities)
        ) {
          throw new Error(`${fixture}: ${record.id}`);
        }
        findings.forEach((finding) => verifyContract(record.text, finding));
      }
    }

const structuredRecords = (await fetch("/structured.jsonl").then(r => r.text())).trim().split("\n").map(JSON.parse);

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

const structuredTransformRecords = (await fetch("/structured-transform.jsonl").then(r => r.text())).trim().split("\n").map(JSON.parse);

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
if (denseFindings.length !== 160) throw new Error("dense finding count");
for (const {path, finding} of denseFindings) verifyContract(pointerValue(denseData, path), finding);
for (const strategy of [{strategy:"redact"}, {strategy:"remove"}, {strategy:"mask",character:"🔒"}]) {
  const config = {default:strategy};
  const result = transformStructured(denseData, [...denseFindings].reverse(), config);
  if (JSON.stringify(result) !== JSON.stringify(scanAndTransformStructured(denseData, {transform:config}))) throw new Error("dense explicit mismatch");
  if (result.transformations.length !== 160) throw new Error("dense transformation count");
  for (const {path, transformation:t} of result.transformations) {
    const source = pointerValue(denseData, path);
    const output = pointerValue(result.data, path);
    const expectedSource = path === "/a" ? "may@example.test" : "other@example.test";
    if (source.slice(t.sourceUtf16Range.start,t.sourceUtf16Range.end) !== expectedSource) throw new Error("dense source range");
    if (Array.from(output).slice(t.outputCodepointRange.start,t.outputCodepointRange.end).join("") !== t.replacement) throw new Error("dense codepoint range");
    if (output.slice(t.outputUtf16Range.start,t.outputUtf16Range.end) !== t.replacement) throw new Error("dense UTF-16 range");
    if (new TextDecoder().decode(new TextEncoder().encode(output).subarray(t.outputByteRange.start,t.outputByteRange.end)) !== t.replacement) throw new Error("dense byte range");
  }
}

for (const strategy of [{strategy:"pseudonymize",key_ref:"names"},{strategy:"tokenize",token_ref:"names"}]) {
  let rejected=false;
  try { scanAndTransformStructured({first_name:"May"},{transform:{default:strategy}}); } catch(e) { rejected=e.code === "unsupported_strategy"; }
  if (!rejected) throw new Error("structured provider operation accepted in WASM");
}
let restoreRejected = false;
try { restoreStructured({}, {scope:"test"}); } catch(e) { restoreRejected=e.code === "unsupported_strategy"; }
if (!restoreRejected) throw new Error("structured restore accepted in WASM");
    const emojiFinding = scan("👋 jane@example.com")[0];
    if (
      JSON.stringify(emojiFinding.byteRange) !== JSON.stringify({ start: 5, end: 21 }) ||
      JSON.stringify(emojiFinding.codepointRange) !== JSON.stringify({ start: 2, end: 18 }) ||
      JSON.stringify(emojiFinding.utf16Range) !== JSON.stringify({ start: 3, end: 19 }) ||
      "👋 jane@example.com".slice(
        emojiFinding.utf16Range.start,
        emojiFinding.utf16Range.end,
      ) !== emojiFinding.matchedText
    ) {
      throw new Error("Unicode ranges do not use the documented coordinate systems");
    }
    const { utf16Range: _derivedRange, ...preSliceEightFinding } = emojiFinding;
    if (
      transform("👋 jane@example.com", [preSliceEightFinding], {
        default: { strategy: "redact" },
      }).text !== "👋 [EMAIL]"
    ) {
      throw new Error("UTF-16 output fields changed the accepted finding input shape");
    }
    if (
      JSON.stringify(scan("Email jane@example.com", { locale: "en-US" })) !==
      JSON.stringify(scan("Email jane@example.com"))
    ) {
      throw new Error("standalone scan configuration changed detector output");
    }

    const text = "👋 jane@example.com and jane@example.com";
    const findings = scan(text);
    const explicit = transform(text, findings, {
      default: { strategy: "redact" },
    });
    const convenient = scanAndTransform(text, {
      transform: { default: { strategy: "redact" } },
    });
    if (JSON.stringify(explicit) !== JSON.stringify(convenient)) {
      throw new Error("explicit and convenience transforms differ");
    }
    if (explicit.text !== "👋 [EMAIL] and [EMAIL]") {
      throw new Error(`unexpected transformed text: ${explicit.text}`);
    }
    if (
      explicit.transformations.length !== 2 ||
      explicit.transformations.some(
        (record) =>
          record.strategy !== "redact" ||
          record.replacement !== "[EMAIL]" ||
          text.slice(record.sourceUtf16Range.start, record.sourceUtf16Range.end) !==
            "jane@example.com" ||
          explicit.text.slice(record.outputUtf16Range.start, record.outputUtf16Range.end) !==
            record.replacement ||
          Array.from(explicit.text)
            .slice(record.outputCodepointRange.start, record.outputCodepointRange.end)
            .join("") !== record.replacement,
      )
    ) {
      throw new Error("transformation records do not select their replacements");
    }
    if (
      "finding" in explicit.transformations[0] ||
      "matchedText" in explicit.transformations[0]
    ) {
      throw new Error("transformation records must not echo original PII");
    }

    if (
      scanAndTransform("Email jane@example.com", {
        transform: { default: { strategy: "mask" } },
      }).text !==
      "Email ****************"
    ) {
      throw new Error("full masking did not replace every code point");
    }

    const partialMask = scanAndTransform("Email jane@example.com", {
      transform: {
        default: {
          strategy: "mask",
          character: "•",
          reveal: { direction: "last", count: 4 },
        },
      },
    });
    if (
      partialMask.text !== "Email ••••••••••••.com" ||
      partialMask.transformations[0].strategy !== "mask" ||
      partialMask.transformations[0].replacement !== "••••••••••••.com" ||
      JSON.stringify(partialMask.transformations[0].outputByteRange) !==
        JSON.stringify({ start: 6, end: 46 })
    ) {
      throw new Error("partial multibyte masking contract failed");
    }

    if (
      scanAndTransform("Email jane@example.com", {
        transform: {
          default: {
            strategy: "mask",
            reveal: { direction: "first", count: 99 },
          },
        },
      }).text !== "Email jane@example.com"
    ) {
      throw new Error("oversized reveal count should preserve the finding");
    }

    const removed = scanAndTransform("Email jane@example.com today", {
      transform: { default: { strategy: "remove" } },
    });
    if (
      removed.text !== "Email  today" ||
      removed.transformations[0].strategy !== "remove" ||
      removed.transformations[0].replacement !== "" ||
      JSON.stringify(removed.transformations[0].outputCodepointRange) !==
        JSON.stringify({ start: 6, end: 6 })
    ) {
      throw new Error("exact removal contract failed");
    }

    for (const invalidConfig of [
      { strategy: "mask", character: "" },
      { strategy: "mask", character: "**" },
      { strategy: "mask", character: " " },
      { strategy: "mask", unexpected: true },
      { strategy: "remove", character: "*" },
      { strategy: "mask", reveal: { direction: "last", count: -1 } },
      { strategy: "mask", reveal: { direction: "middle", count: 4 } },
    ]) {
      expectThrows(
        () =>
          scanAndTransform("Email jane@example.com", {
            transform: { default: invalidConfig },
          }),
        "DataFogError",
      );
    }

    try {
      transform(
        text,
        [{ ...findings[0], confidence: 2 }],
        { default: { strategy: "redact" } },
      );
      throw new Error("invalid finding should fail");
    } catch (error) {
      if (
        !(error instanceof DataFogError) ||
        error.code !== "invalid_finding" ||
        error.reason !== "invalid_confidence" ||
        error.path !== "/findings/0/confidence" ||
        error.findingIndex !== 0
      ) {
        throw error;
      }
    }

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
    if (
      selected.text !== "Email support@example.com or call **********0100" ||
      selected.transformations.length !== 1
    ) {
      throw new Error("selection, overrides, or allowlists failed");
    }

    try {
      scanAndTransform("Email jane@example.com", {
        transform: {
          default: { strategy: "pseudonymize", key_ref: "customers/email" },
        },
      });
      throw new Error("browser pseudonymization should be unsupported");
    } catch (error) {
      if (
        !(error instanceof DataFogError) ||
        error.code !== "unsupported_strategy" ||
        error.path !== "/transform/default/key_ref"
      ) {
        throw error;
      }
    }

    try {
      scanAndTransform("Email jane@example.com", {
        transform: {
          default: { strategy: "tokenize", token_ref: "customers/default" },
        },
      });
      throw new Error("browser tokenization should be unsupported");
    } catch (error) {
      if (!(error instanceof DataFogError) || error.code !== "unsupported_strategy") {
        throw error;
      }
    }
    try {
      restore("ordinary text", { scope: "tenant" });
      throw new Error("browser restoration should be unsupported");
    } catch (error) {
      if (!(error instanceof DataFogError) || error.code !== "unsupported_strategy") {
        throw error;
      }
    }
    try {
      restore("DFTOKENv1(8):YQ.Yg.Yw", { scope: "tenant" });
      throw new Error("browser token restoration should be unsupported");
    } catch (error) {
      if (!(error instanceof DataFogError) || error.code !== "unsupported_strategy") {
        throw error;
      }
    }

    try {
      transform(text, findings, {
        default: { strategy: "redact" },
        overides: {},
      });
      throw new Error("unknown configuration field should fail");
    } catch (error) {
      if (
        !(error instanceof DataFogError) ||
        error.code !== "invalid_configuration" ||
        error.reason !== "unknown_field" ||
        error.path !== "/overides"
      ) {
        throw error;
      }
    }
  });

  console.log("Installed @datafog/wasm package matches fixtures and the Finding contract.");
} finally {
  await browser?.close();
  await new Promise((resolve) => server?.close(resolve) ?? resolve());
  if (tarball) {
    rmSync(tarball, { force: true });
  }
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
