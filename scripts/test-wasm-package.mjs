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
  transform,
  type EntityType,
  type Finding,
  type MaskRevealConfig,
  type TextRange,
  type TransformationConfig,
  type TransformResult,
} from "@datafog/wasm";

const ready: Promise<void> = init();
const findings: Finding[] = scan("Email jane@example.com");
const entityType: EntityType = findings[0]?.entityType ?? "CUSTOM_ENTITY";
const range: TextRange = findings[0]?.byteRange ?? { start: 0, end: 0 };
const transformed: TransformResult = transform(
  "Email jane@example.com",
  findings,
  { strategy: "redact" },
);
const scannedAndTransformed: TransformResult = scanAndTransform(
  "Email jane@example.com",
  { strategy: "redact" },
);
const reveal: MaskRevealConfig = { direction: "last", count: 4 };
const maskConfig: TransformationConfig = {
  strategy: "mask",
  character: "•",
  reveal,
};
const masked: TransformResult = scanAndTransform("Email jane@example.com", maskConfig);

void ready;
void entityType;
void range;
void transformed;
void scannedAndTransformed;
void masked;
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

  for (const fixture of ["development.jsonl", "final.jsonl"]) {
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
    const { init, scan, scanAndTransform, transform } = await import(
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

    const emojiFinding = scan("👋 jane@example.com")[0];
    if (
      JSON.stringify(emojiFinding.byteRange) !== JSON.stringify({ start: 5, end: 21 }) ||
      JSON.stringify(emojiFinding.codepointRange) !== JSON.stringify({ start: 2, end: 18 })
    ) {
      throw new Error("Unicode ranges do not use the documented coordinate systems");
    }

    const text = "👋 jane@example.com and jane@example.com";
    const findings = scan(text);
    const explicit = transform(text, findings, { strategy: "redact" });
    const convenient = scanAndTransform(text, { strategy: "redact" });
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
          Array.from(explicit.text)
            .slice(record.outputCodepointRange.start, record.outputCodepointRange.end)
            .join("") !== record.replacement,
      )
    ) {
      throw new Error("transformation records do not select their replacements");
    }

    if (
      scanAndTransform("Email jane@example.com", { strategy: "mask" }).text !==
      "Email ****************"
    ) {
      throw new Error("full masking did not replace every code point");
    }

    const partialMask = scanAndTransform("Email jane@example.com", {
      strategy: "mask",
      character: "•",
      reveal: { direction: "last", count: 4 },
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
        strategy: "mask",
        reveal: { direction: "first", count: 99 },
      }).text !== "Email jane@example.com"
    ) {
      throw new Error("oversized reveal count should preserve the finding");
    }

    const removed = scanAndTransform("Email jane@example.com today", {
      strategy: "remove",
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
        () => scanAndTransform("Email jane@example.com", invalidConfig),
        "TypeError",
      );
    }

    expectThrows(
      () =>
        transform(
          text,
          [{ ...findings[0], confidence: 2 }],
          { strategy: "redact" },
        ),
      "Error",
    );
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
