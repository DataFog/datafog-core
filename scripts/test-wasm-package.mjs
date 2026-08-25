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
import { init, scan, type Entity, type Label } from "@datafog/wasm";

const ready: Promise<void> = init();
const entities: Entity[] = scan("Email jane@example.com");
const label: Label = entities[0]?.label ?? "EMAIL";

void ready;
void label;
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
    const { init, scan } = await import("/node_modules/@datafog/wasm/index.js");

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

    for (const fixture of ["development.jsonl", "final.jsonl"]) {
      const source = await fetch(`/${fixture}`).then((response) => response.text());
      const records = source
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line));

      for (const record of records) {
        if (JSON.stringify(scan(record.text)) !== JSON.stringify(record.entities)) {
          throw new Error(`${fixture}: ${record.id}`);
        }
      }
    }
  });

  console.log("Installed @datafog/wasm package matches both fixtures.");
} finally {
  await browser?.close();
  await new Promise((resolve) => server?.close(resolve) ?? resolve());
  if (tarball) {
    rmSync(tarball, { force: true });
  }
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
