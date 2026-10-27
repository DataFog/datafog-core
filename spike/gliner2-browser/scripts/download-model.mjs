#!/usr/bin/env node
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { mkdir, rename, rm, stat } from "node:fs/promises";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { MODEL } from "../../../bindings/wasm/models/model.mjs";

const directory = new URL("../models/", import.meta.url);
async function verify(path, expected) {
  if ((await stat(path)).size !== expected.bytes)
    throw new Error(`Size mismatch: ${path}`);
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  if (hash.digest("hex") !== expected.sha256)
    throw new Error(`Checksum mismatch: ${path}`);
}

for (const [name, expected] of Object.entries(MODEL.files)) {
  const destination = new URL(name, directory);
  await mkdir(dirname(fileURLToPath(destination)), { recursive: true });
  try {
    await stat(destination);
    await verify(destination, expected);
    console.log(`Verified ${name}`);
    continue;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const temporary = new URL(destination.href + ".partial");
  try {
    const response = await fetch(
      `https://huggingface.co/${MODEL.repository}/resolve/${MODEL.revision}/${name}`,
      { signal: AbortSignal.timeout(600000) },
    );
    if (!response.ok)
      throw new Error(`Download failed: ${name}, HTTP ${response.status}`);
    await pipeline(response.body, createWriteStream(temporary));
    await verify(temporary, expected);
    await rename(temporary, destination);
    console.log(`Downloaded and verified ${name}`);
  } finally {
    await rm(temporary, { force: true });
  }
}
console.log(`GLiNER2.5 Small ready (${MODEL.revision})`);
