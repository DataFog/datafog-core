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
      const { GlinerSession } = await import("/src/session.mjs");
      try {
        await GlinerSession.create();
        return "unexpected success";
      } catch (error) {
        return error.message;
      }
    });
    assert.match(message, /Cannot load onnx\/model.onnx: HTTP 404/);
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
      const { GlinerSession } = await import("/src/session.mjs");
      try {
        await GlinerSession.create();
        return "unexpected success";
      } catch (error) {
        return error.message;
      }
    });
    assert.match(message, /checksum mismatch: tokenizer_config.json/);
    await page.unroute("**/models/tokenizer_config.json");
  });
  await check("UI loads the pinned Small model", async () => {
    await page.getByRole("button", { name: "Load model" }).click();
    await page.waitForFunction(
      () => document.getElementById("status").textContent === "Model ready.",
      { timeout: 120000 },
    );
    assert.equal(
      await page.getByRole("button", { name: "Extract entities" }).isEnabled(),
      true,
    );
  });
  await check("broader defaults return actual entities in the UI", async () => {
    results.sampleText = await page.locator("#text").inputValue();
    await page.getByRole("button", { name: "Extract entities" }).click();
    await page.waitForFunction(() => !document.getElementById("json").hidden);
    results.sample = JSON.parse(await page.locator("#json").textContent());
    assert.ok(
      results.sample.entities.some(
        (entity) => entity.label === "person" && entity.text === "John Smith",
      ),
    );
    assert.ok(
      results.sample.entities.some(
        (entity) =>
          entity.label === "email" && entity.text === "john.smith@example.com",
      ),
    );
    for (const entity of results.sample.entities)
      assert.equal(
        results.sampleText.slice(
          entity.utf16Range.start,
          entity.utf16Range.end,
        ),
        entity.text,
      );
  });
  await check(
    "custom entity types are not restricted to names and addresses",
    async () => {
      await page
        .locator("#text")
        .fill("Apple CEO Tim Cook announced iPhone 15 in Cupertino yesterday.");
      await page.locator("#labels").fill("company, person, product, location");
      await page.getByRole("button", { name: "Extract entities" }).click();
      await page.waitForFunction(() => !document.getElementById("json").hidden);
      const custom = JSON.parse(await page.locator("#json").textContent());
      assert.ok(
        custom.entities.some(
          (entity) => entity.label === "company" && entity.text === "Apple",
        ),
      );
      assert.ok(
        custom.entities.some(
          (entity) => entity.label === "product" && entity.text === "iPhone 15",
        ),
      );
      assert.ok(
        custom.entities.some(
          (entity) => entity.label === "person" && entity.text === "Tim Cook",
        ),
      );
      results.customSample = custom;
    },
  );
  // API failure and cleanup checks use the same module and real WASM model as the UI.
  await check(
    "API handles negatives, empty input, invalid input, Unicode, and closure",
    async () => {
      const api = await page.evaluate(async () => {
        const { GlinerSession } = await import("/src/session.mjs");
        const session = await GlinerSession.create();
        const output = {};
        try {
          output.negative = await session.extract("Turn off the lights.", {
            labels: ["person", "address"],
          });
          output.empty = await session.extract(" \n ");
          output.unicodeText = "👋 Contact María García in Madrid.";
          output.unicode = await session.extract(output.unicodeText, {
            labels: ["person", "location"],
          });
          output.failures = [];
          for (const [text, options] of [
            ["text", { labels: [] }],
            ["text", { threshold: 0 }],
            ["text", { threshold: NaN }],
            ["word ".repeat(600), {}],
            ["\uD800", {}],
          ]) {
            try {
              await session.extract(text, options);
              output.failures.push("unexpected success");
            } catch (error) {
              output.failures.push(error.message);
            }
          }
          const running = session.extract("Contact John Smith.");
          try {
            await session.extract("Contact Jane Doe.");
            output.concurrent = "unexpected success";
          } catch (error) {
            output.concurrent = error.message;
          }
          await running;
        } finally {
          await session.close();
        }
        await session.close();
        try {
          await session.extract("John Smith");
          output.closed = "unexpected success";
        } catch (error) {
          output.closed = error.message;
        }
        return output;
      });
      assert.deepEqual(api.negative.entities, []);
      assert.deepEqual(api.empty.entities, []);
      assert.ok(
        api.unicode.entities.some(
          (entity) =>
            entity.label === "person" && entity.text === "María García",
        ),
      );
      for (const e of api.unicode.entities) {
        assert.equal(
          api.unicodeText.slice(e.utf16Range.start, e.utf16Range.end),
          e.text,
        );
        assert.equal(
          Array.from(api.unicodeText)
            .slice(e.codepointRange.start, e.codepointRange.end)
            .join(""),
          e.text,
        );
        assert.equal(
          new TextDecoder().decode(
            new TextEncoder()
              .encode(api.unicodeText)
              .slice(e.byteRange.start, e.byteRange.end),
          ),
          e.text,
        );
      }
      assert.match(api.failures[0], /entity labels/);
      assert.match(api.failures[1], /threshold/);
      assert.match(api.failures[2], /threshold/);
      assert.match(api.failures[3], /exceed 512/);
      assert.match(api.failures[4], /valid Unicode/);
      assert.match(api.concurrent, /already running/);
      assert.match(api.closed, /closed/);
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
          const { GlinerSession } = await import("/src/session.mjs");
          const session = await GlinerSession.create();
          try {
            return await session.extract("John Smith lives in Boston.", {
              labels: ["person"],
            });
          } finally {
            await session.close();
          }
        });
        assert.ok(
          detected.entities.some((entity) => entity.text === "John Smith"),
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
