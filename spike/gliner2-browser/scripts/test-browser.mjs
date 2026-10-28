#!/usr/bin/env node
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { createModelServer } from "./serve.mjs";

const server = createModelServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
let browser;
const results = { tests: [] };
async function check(name, run) {
  await run();
  results.tests.push(name);
  console.log(`PASS ${name}`);
}
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await check(
    "server rejects traversal outside its static directories",
    async () => {
      const response = await fetch(
        `http://127.0.0.1:${server.address().port}/src/%2e%2e%2fpackage.json`,
      );
      assert.equal(response.status, 404);
    },
  );
  await check("missing model fails explicitly", async () => {
    await page.route("**/models/onnx/model.onnx", (route) =>
      route.fulfill({ status: 404, body: "missing" }),
    );
    const message = await page.evaluate(async () => {
      const { ModelSession } = await import("@datafog/wasm/models");
      try {
        await ModelSession.open({
          modelBaseUrl: new URL("/models/", location.href),
          wasmPaths: new URL(
            "/node_modules/onnxruntime-web/dist/",
            location.href,
          ).href,
        });
        return "unexpected success";
      } catch (error) {
        return error.code;
      }
    });
    assert.equal(message, "model_load_failed");
    await page.unroute("**/models/onnx/model.onnx");
  });
  await check("same-size corrupted asset fails its checksum", async () => {
    const corrupt = Buffer.from(
      await readFile(
        new URL("../models/tokenizer_config.json", import.meta.url),
      ),
    );
    corrupt[0] ^= 1;
    await page.route("**/models/tokenizer_config.json", (route) =>
      route.fulfill({ status: 200, body: corrupt }),
    );
    const message = await page.evaluate(async () => {
      const { ModelSession } = await import("@datafog/wasm/models");
      try {
        await ModelSession.open({
          modelBaseUrl: new URL("/models/", location.href),
          wasmPaths: new URL(
            "/node_modules/onnxruntime-web/dist/",
            location.href,
          ).href,
        });
        return "unexpected success";
      } catch (error) {
        return error.code;
      }
    });
    assert.equal(message, "model_load_failed");
    await page.unroute("**/models/tokenizer_config.json");
  });
  await check("UI loads the pinned Small model", async () => {
    await page.getByRole("button", { name: "Load model" }).click();
    await page.waitForFunction(
      () => document.getElementById("status").textContent === "Model ready.",
      { timeout: 120000 },
    );
    assert.equal(
      await page.getByRole("button", { name: "Scan text" }).isEnabled(),
      true,
    );
  });
  await check("broader defaults return actual entities in the UI", async () => {
    results.sampleText = await page.locator("#text").inputValue();
    await page.getByRole("button", { name: "Scan text" }).click();
    await page.waitForFunction(() => !document.getElementById("json").hidden);
    results.sample = JSON.parse(await page.locator("#json").textContent());
    assert.ok(
      results.sample.findings.some(
        (entity) =>
          entity.entityType === "PERSON" && entity.matchedText === "John Smith",
      ),
    );
    assert.ok(
      results.sample.findings.some(
        (entity) =>
          entity.entityType === "EMAIL" &&
          entity.matchedText === "john.smith@example.com",
      ),
    );
    for (const entity of results.sample.findings)
      assert.equal(
        results.sampleText.slice(
          entity.utf16Range.start,
          entity.utf16Range.end,
        ),
        entity.matchedText,
      );
  });
  await check(
    "custom labels remain available through an explicit session mapping",
    async () => {
      const custom = await page.evaluate(async () => {
        const { ModelSession } = await import("@datafog/wasm/models");
        const session = await ModelSession.open({
          modelBaseUrl: new URL("/models/", location.href),
          wasmPaths: new URL(
            "/node_modules/onnxruntime-web/dist/",
            location.href,
          ).href,
          labels: {
            company: "COMPANY",
            person: "PERSON",
            product: "PRODUCT",
            location: "LOCATION",
          },
        });
        const descriptor = session.describe();
        if (descriptor.labels.company !== "COMPANY" || descriptor.backend !== "gliner25_onnx_web") {
          throw new Error("Browser session descriptor differs from its configured labels");
        }
        try {
          return await session.scanModel(
            "Apple CEO Tim Cook announced iPhone 15 in Cupertino yesterday.",
          );
        } finally {
          await session.close();
          if (JSON.stringify(session.describe()) !== JSON.stringify(descriptor)) {
            throw new Error("Closing a browser session changed its descriptor");
          }
        }
      });
      for (const [entityType, matchedText] of [
        ["COMPANY", "Apple"],
        ["PERSON", "Tim Cook"],
        ["PRODUCT", "iPhone 15"],
      ]) {
        assert.ok(
          custom.findings.some(
            (finding) =>
              finding.entityType === entityType &&
              finding.matchedText === matchedText,
          ),
        );
      }
      results.customSample = custom;
    },
  );
  // API failure and cleanup checks use the same module and real WASM model as the UI.
  await check(
    "API handles negatives, empty input, invalid input, Unicode, and closure",
    async () => {
      const api = await page.evaluate(async () => {
        const { ModelSession } = await import("@datafog/wasm/models");
        const session = await ModelSession.open({
          modelBaseUrl: new URL("/models/", location.href),
          wasmPaths: new URL(
            "/node_modules/onnxruntime-web/dist/",
            location.href,
          ).href,
        });
        const output = {};
        try {
          output.negative = await session.scan("Turn off the lights.", {
            entities: ["PERSON", "ADDRESS"],
          });
          output.empty = await session.scan(" \n ");
          output.unicodeText = "👋 Contact María García at maria@example.com.";
          output.unicode = await session.scan(output.unicodeText, {
            entities: ["PERSON", "EMAIL"],
          });
          const { transform } = await import("@datafog/wasm");
          output.transformed = transform(
            output.unicodeText,
            output.unicode.findings,
            { default: { strategy: "redact" } },
          );
          output.failures = [];
          for (const [text, config] of [
            ["text", { entities: [] }],
            ["text", { entities: ["UNKNOWN"] }],
            ["text", { entities: ["PERSON", "PERSON"] }],
            ["text", { locale: "bogus" }],
            ["word ".repeat(600), { entities: ["PERSON"] }],
            ["\uD800", {}],
            ["text", null],
            ["x".repeat(65537), { entities: ["EMAIL"] }],
          ]) {
            try {
              await session.scan(text, config);
              output.failures.push("unexpected success");
            } catch (error) {
              output.failures.push(error.code || error.message);
            }
          }
          const config = { entities: ["PERSON"] };
          const running = session.scan("Contact John Smith.", config);
          config.entities[0] = "EMAIL";
          try {
            await session.scan("Contact Jane Doe.");
            output.concurrent = "unexpected success";
          } catch (error) {
            output.concurrent = error.code;
          }
          // A rule-only request bypasses model admission even during inference.
          output.ruleOnly = await session.scan("Email jane@example.com", {
            entities: ["EMAIL"],
          });
          try {
            await session.close();
            output.busyClose = "unexpected success";
          } catch (error) {
            output.busyClose = error.code;
          }
          output.originalSelection = await running;
        } finally {
          await session.close();
        }
        await session.close();
        try {
          await session.scan("John Smith");
          output.closed = "unexpected success";
        } catch (error) {
          output.closed = error.code;
        }
        return output;
      });
      assert.deepEqual(api.negative.findings, []);
      assert.deepEqual(api.empty.findings, []);
      assert.ok(
        api.unicode.findings.some(
          (entity) =>
            entity.entityType === "PERSON" &&
            entity.matchedText === "María García",
        ),
      );
      for (const e of api.unicode.findings) {
        assert.equal(
          api.unicodeText.slice(e.utf16Range.start, e.utf16Range.end),
          e.matchedText,
        );
        assert.equal(
          Array.from(api.unicodeText)
            .slice(e.codepointRange.start, e.codepointRange.end)
            .join(""),
          e.matchedText,
        );
        assert.equal(
          new TextDecoder().decode(
            new TextEncoder()
              .encode(api.unicodeText)
              .slice(e.byteRange.start, e.byteRange.end),
          ),
          e.matchedText,
        );
      }
      assert.deepEqual(
        api.failures.slice(0, 4),
        new Array(4).fill("invalid_configuration"),
      );
      assert.equal(api.failures[4], "model_inference_failed");
      assert.match(api.failures[5], /well-formed/);
      assert.equal(api.failures[6], "invalid_configuration");
      assert.equal(api.failures[7], "model_resource_exhausted");
      assert.ok(
        api.originalSelection.findings.some(
          (finding) =>
            finding.entityType === "PERSON" &&
            finding.matchedText === "John Smith",
        ),
      );
      assert.equal(api.concurrent, "model_session_busy");
      assert.equal(api.busyClose, "model_session_busy");
      assert.equal(api.closed, "model_session_closed");
      assert.equal(api.ruleOnly.findings.length, 1);
      assert.equal(api.ruleOnly.findings[0].entityType, "EMAIL");
      assert.ok(!api.ruleOnly.findings[0].detectorName.includes("/model/"));
      assert.ok(!api.transformed.text.includes("María García"));
      assert.ok(!api.transformed.text.includes("maria@example.com"));
      const person = api.unicode.findings.find(
        (finding) => finding.entityType === "PERSON",
      );
      assert.equal(person.confidence, undefined);
      const evidence = api.unicode.evidence.find(
        (item) =>
          api.unicode.findings[item.finding_index].entityType === "PERSON",
      );
      assert.equal(
        evidence.sources[0].raw_score.scale_id,
        "gliner25_pair_sigmoid",
      );
      assert.ok(evidence.sources[0].raw_score.value > 0.5);
      assert.equal(evidence.sources[0].calibrated_confidence, undefined);
    },
  );
  await check(
    "single-thread WASM works without cross-origin isolation",
    async () => {
      const fallback = await browser.newPage();
      await fallback.route(
        `http://127.0.0.1:${server.address().port}/`,
        async (route) => {
          const response = await route.fetch();
          const headers = response.headers();
          delete headers["cross-origin-opener-policy"];
          delete headers["cross-origin-embedder-policy"];
          await route.fulfill({ response, headers });
        },
      );
      try {
        await fallback.goto(`http://127.0.0.1:${server.address().port}`);
        const detected = await fallback.evaluate(async () => {
          if (crossOriginIsolated) throw new Error("Fallback test is isolated");
          const { ModelSession } = await import("@datafog/wasm/models");
          const session = await ModelSession.open({
            modelBaseUrl: new URL("/models/", location.href),
            wasmPaths: new URL(
              "/node_modules/onnxruntime-web/dist/",
              location.href,
            ).href,
          });
          try {
            return await session.scan("John Smith lives in Boston.", {
              entities: ["PERSON"],
            });
          } finally {
            await session.close();
          }
        });
        assert.ok(
          detected.findings.some(
            (entity) => entity.matchedText === "John Smith",
          ),
        );
      } finally {
        await fallback.close();
      }
    },
  );
  console.log(`Completed ${results.tests.length} browser checks`);
} catch (error) {
  results.error = String(error.stack || error);
  throw error;
} finally {
  try {
    if (browser) await browser.close();
  } finally {
    try {
      await new Promise((resolve) => server.close(resolve));
    } finally {
      await mkdir(new URL("../results/", import.meta.url), { recursive: true });
      await writeFile(
        new URL("../results/entities.json", import.meta.url),
        JSON.stringify(results, null, 2) + "\n",
      );
    }
  }
}
