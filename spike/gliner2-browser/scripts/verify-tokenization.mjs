#!/usr/bin/env node
/**
 * Verify that our transformers.js tokenization matches the Python reference.
 *
 * Compares token IDs against the smoke_inputs.json reference.
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { Gliner2Preprocessor } from "../src/gliner2-preprocessor.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");

const REFERENCE = JSON.parse(
  readFileSync(
    resolve(ROOT, "examples/transformers_js_ref/smoke_inputs.json"),
    "utf-8"
  )
);

async function main() {
  console.log("Loading preprocessor from okasi/gliner2-privacy-filter-pii-multi-onnx...");
  const preprocessor = await Gliner2Preprocessor.fromPretrained(
    "okasi/gliner2-privacy-filter-pii-multi-onnx"
  );

  const text = REFERENCE.text;
  const labels = REFERENCE.labels;
  const refInputIds = REFERENCE.feed.input_ids[0];
  const refWordIndices = REFERENCE.feed.text_word_indices[0];
  const refWordCounts = REFERENCE.feed.text_word_counts[0];
  const refStartMapping = REFERENCE.start_mapping;
  const refEndMapping = REFERENCE.end_mapping;

  console.log("\n=== Reference ===");
  console.log(`Text: "${text}"`);
  console.log(`Labels: ${labels.join(", ")}`);
  console.log(`Input IDs length: ${refInputIds.length}`);
  console.log(`Word indices: ${refWordIndices}`);
  console.log(`Word count: ${refWordCounts}`);
  console.log(`Start mapping length: ${refStartMapping.length}`);

  const prepared = preprocessor.prepareInputs(text, labels);
  const inputIds = prepared.inputIds;
  const wordIndices = prepared.textWordIndices;
  const words = prepared.words;
  const startMapping = prepared.startMapping;
  const endMapping = prepared.endMapping;

  console.log("\n=== Our Preprocessing ===");
  console.log(`Words (${words.length}): ${words.map((w) => `"${w.word}"`).join(", ")}`);
  console.log(`Input IDs length: ${inputIds.length}`);
  console.log(`Schema token count: ${prepared.schemaTokenCount}`);

  console.log("\n=== Comparison ===");
  console.log(`Our input_ids length: ${inputIds.length}, Reference: ${refInputIds.length}`);
  console.log(`Our word_indices: ${wordIndices}, Reference: ${refWordIndices}`);
  console.log(`Our word_count: ${words.length}, Reference: ${refWordCounts}`);
  console.log(`Our start_mapping length: ${startMapping.length}, Reference: ${refStartMapping.length}`);

  let idMatches = inputIds.length === refInputIds.length;
  const idDiffs = [];
  for (let i = 0; i < Math.max(inputIds.length, refInputIds.length); i++) {
    if (inputIds[i] !== refInputIds[i]) {
      idDiffs.push({
        pos: i,
        ours: inputIds[i],
        ref: refInputIds[i],
        ourToken: inputIds[i] !== undefined ? preprocessor.decode([inputIds[i]]) : "N/A",
        refToken: refInputIds[i] !== undefined ? preprocessor.decode([refInputIds[i]]) : "N/A",
      });
      idMatches = false;
    }
  }

  if (idMatches) {
    console.log("\n✓ Token IDs MATCH exactly!");
  } else {
    console.log(`\n✗ Token IDs MISMATCH (${idDiffs.length} differences):`);
    for (const diff of idDiffs.slice(0, 10)) {
      console.log(
        `  Position ${diff.pos}: ours=${diff.ours} ("${diff.ourToken}"), ref=${diff.ref} ("${diff.refToken}")`
      );
    }
    if (idDiffs.length > 10) {
      console.log(`  ... and ${idDiffs.length - 10} more`);
    }
  }

  const wordIdxMatches =
    wordIndices.length === refWordIndices.length &&
    wordIndices.every((v, i) => v === refWordIndices[i]);

  if (wordIdxMatches) {
    console.log("✓ Word indices MATCH exactly!");
  } else {
    console.log("✗ Word indices MISMATCH:");
    console.log(`  Ours: ${wordIndices}`);
    console.log(`  Ref:  ${refWordIndices}`);
  }

  const startMapMatches =
    startMapping.length === refStartMapping.length &&
    startMapping.every((v, i) => v === refStartMapping[i]);

  if (startMapMatches) {
    console.log("✓ Start mapping MATCH exactly!");
  } else {
    console.log("✗ Start mapping MISMATCH:");
    console.log(`  Ours (len=${startMapping.length}): ${startMapping.slice(0, 20).join(", ")}...`);
    console.log(`  Ref  (len=${refStartMapping.length}): ${refStartMapping.slice(0, 20).join(", ")}...`);
  }

  const endMapMatches =
    endMapping.length === refEndMapping.length &&
    endMapping.every((v, i) => v === refEndMapping[i]);

  if (endMapMatches) {
    console.log("✓ End mapping MATCH exactly!");
  } else {
    console.log("✗ End mapping MISMATCH:");
    console.log(`  Ours (len=${endMapping.length}): ${endMapping.slice(0, 20).join(", ")}...`);
    console.log(`  Ref  (len=${refEndMapping.length}): ${refEndMapping.slice(0, 20).join(", ")}...`);
  }

  console.log("\n=== Token Decode Debug ===");
  console.log("First 10 reference tokens:");
  for (let i = 0; i < 10; i++) {
    const id = refInputIds[i];
    const decoded = preprocessor.decode([id]);
    console.log(`  [${i}] id=${id} -> "${decoded}"`);
  }

  console.log("\nFirst 10 our tokens:");
  for (let i = 0; i < 10; i++) {
    const id = inputIds[i];
    const decoded = preprocessor.decode([id]);
    console.log(`  [${i}] id=${id} -> "${decoded}"`);
  }

  const overallMatch = idMatches && wordIdxMatches && startMapMatches && endMapMatches;
  console.log(`\n${"=".repeat(50)}`);
  console.log(overallMatch ? "✓ ALL CHECKS PASSED" : "✗ SOME CHECKS FAILED");
  console.log("=".repeat(50));

  process.exit(overallMatch ? 0 : 1);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
