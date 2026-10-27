import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { Tokenizer } from "@huggingface/tokenizers";
import { prepareInput } from "../src/preprocessor.mjs";
import { decodeEntities } from "../src/decode.mjs";
import { DEFAULT_LABELS, validateLabels } from "../src/model.mjs";

const tokenizerJson = JSON.parse(
  await readFile(new URL("../models/tokenizer.json", import.meta.url), "utf8"),
);
const tokenizerConfig = JSON.parse(
  await readFile(
    new URL("../models/tokenizer_config.json", import.meta.url),
    "utf8",
  ),
);
const tokenizer = new Tokenizer(tokenizerJson, tokenizerConfig);
function output(indices, logits, valid = logits.map(() => 1)) {
  return {
    pair_indices: {
      dims: [1, 1, logits.length, 2],
      data: indices.flat().map(BigInt),
    },
    pair_logits: { dims: [1, 1, logits.length], data: logits },
    pair_valid: { dims: [1, 1, logits.length], data: valid },
  };
}

test("tokenizer matches independent Rust-tokenizers IDs instead of dropping Metaspace prefixes", () => {
  for (const [word, expected] of [
    ["(", [287]],
    ["entities", [6967]],
    ["name", [601]],
    ["john", [23810]],
    ["smith", [35107]],
    ["[E]", [128005]],
  ]) {
    assert.deepEqual(
      tokenizer.encode(word, { add_special_tokens: false }).ids,
      expected,
      word,
    );
  }
  assert.equal(
    tokenizerJson.pre_tokenizer.pretokenizers[0].add_prefix_space,
    undefined,
    "original model metadata stays unchanged",
  );
});

test("custom labels route queries only from schema markers, never from text", () => {
  const prepared = prepareInput(
    "[E] Tim Cook leads Apple.",
    ["person", "company"],
    tokenizer,
  );
  assert.equal(prepared.queryIndices.length, 2);
  assert.deepEqual(
    prepared.queryIndices.map((index) => prepared.inputIds[index]),
    [128005, 128005],
  );
  assert.deepEqual(
    prepared.words.map((word) => word.value),
    ["[", "e", "]", "tim", "cook", "leads", "apple", "."],
  );
  assert.ok(
    DEFAULT_LABELS.includes("email") && DEFAULT_LABELS.includes("phone number"),
  );
});

test("invalid and oversized requests fail instead of dropping labels or truncating text", () => {
  for (const labels of [
    [],
    [""],
    [" person"],
    ["person", "Person"],
    ["[E] person"],
    ["person\n"],
    new Array(17).fill("person"),
  ]) {
    assert.throws(() => validateLabels(labels), TypeError);
  }
  assert.throws(
    () => prepareInput("word ".repeat(600), ["person"], tokenizer),
    /exceed 512 model tokens/,
  );
});

test("padded high-score candidates and duplicate slots cannot steal a valid name", () => {
  const text = "John Smith";
  const prepared = prepareInput(text, ["person"], tokenizer);
  const entities = decodeEntities(
    output(
      [
        [2, 1],
        [0, 2],
        [0, 2],
        [0, 1],
      ],
      [100, 5, 4, 2],
    ),
    prepared,
    text,
    ["person"],
    0.5,
  );
  assert.equal(entities.length, 1);
  assert.equal(entities[0].text, "John Smith");
  assert.deepEqual(entities[0].utf16Range, { start: 0, end: 10 });
});

test("masked and below-threshold candidates return no entities", () => {
  const text = "John Smith";
  const prepared = prepareInput(text, ["person"], tokenizer);
  assert.deepEqual(
    decodeEntities(
      output(
        [
          [0, 2],
          [0, 1],
        ],
        [10, -10],
        [0, 1],
      ),
      prepared,
      text,
      ["person"],
      0.5,
    ),
    [],
  );
  assert.throws(
    () =>
      decodeEntities(output([[0, 2]], [NaN]), prepared, text, ["person"], 0.5),
    /nonfinite/,
  );
  assert.throws(
    () => decodeEntities({}, prepared, text, ["person"], 0.5),
    /unexpected.*shape/,
  );
});

test("source offsets survive emoji, combining marks, and synthetic terminal punctuation", () => {
  const text = "👋 Jose\u0301";
  const prepared = prepareInput(text, ["person"], tokenizer);
  const [entity] = decodeEntities(
    output([[1, 3]], [5]),
    prepared,
    text,
    ["person"],
    0.5,
  );
  assert.equal(entity.text, "Jose\u0301");
  assert.deepEqual(entity.utf16Range, { start: 3, end: 8 });
  assert.deepEqual(entity.codepointRange, { start: 2, end: 7 });
  assert.deepEqual(entity.byteRange, { start: 5, end: 11 });
  assert.equal(
    text.slice(entity.utf16Range.start, entity.utf16Range.end),
    entity.text,
  );
});
