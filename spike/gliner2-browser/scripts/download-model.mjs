#!/usr/bin/env node
/**
 * Download GLiNER2 PII ONNX model files from HuggingFace.
 * 
 * Uses okasi/gliner2-privacy-filter-pii-multi-onnx which provides Q4/Q8 quantized
 * exports of fastino/gliner2-privacy-filter-PII-multi.
 */
import { createWriteStream, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const MODEL_DIR = join(__dirname, "..", "models");

const HF_REPO = "okasi/gliner2-privacy-filter-pii-multi-onnx";
const HF_BASE = `https://huggingface.co/${HF_REPO}/resolve/main`;

const FILES = [
  // Q8 model (default, best accuracy)
  { name: "model_q8.onnx", size: "~53 MB graph" },
  { name: "model_q8.onnx.data", size: "~457 MB weights" },
  { name: "model_q8_metadata.json", size: "small" },
  // Tokenizer
  { name: "tokenizer.json", size: "small" },
  { name: "tokenizer_config.json", size: "small" },
  { name: "config.json", size: "small" },
  // Entity labels
  { name: "entity_labels.json", size: "small" },
];

async function downloadFile(name) {
  const url = `${HF_BASE}/${name}`;
  const dest = join(MODEL_DIR, name);
  
  if (existsSync(dest)) {
    console.log(`  [skip] ${name} (exists)`);
    return;
  }
  
  console.log(`  [download] ${name}...`);
  
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) {
    throw new Error(`Failed to download ${name}: ${response.status}`);
  }
  
  mkdirSync(dirname(dest), { recursive: true });
  const fileStream = createWriteStream(dest);
  await pipeline(response.body, fileStream);
  
  console.log(`  [done] ${name}`);
}

async function computeSha256(path) {
  const data = await readFile(path);
  return createHash("sha256").update(data).digest("hex");
}

async function main() {
  console.log(`Downloading GLiNER2 PII ONNX model from ${HF_REPO}...\n`);
  console.log(`Target: ${MODEL_DIR}\n`);
  
  mkdirSync(MODEL_DIR, { recursive: true });
  
  for (const file of FILES) {
    await downloadFile(file.name);
  }
  
  // Compute and save manifest
  console.log("\nComputing SHA-256 hashes...");
  const manifest = { source: HF_REPO, files: {} };
  for (const file of FILES) {
    const path = join(MODEL_DIR, file.name);
    if (existsSync(path)) {
      manifest.files[file.name] = await computeSha256(path);
      console.log(`  ${file.name}: ${manifest.files[file.name].slice(0, 16)}...`);
    }
  }
  
  await writeFile(
    join(MODEL_DIR, "manifest.json"),
    JSON.stringify(manifest, null, 2)
  );
  
  console.log("\nDone. Model files ready in models/");
}

main().catch((err) => {
  console.error("Download failed:", err.message);
  process.exit(1);
});
