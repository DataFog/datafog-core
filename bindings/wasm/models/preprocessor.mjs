// Schema packing adapted from Pastel-Cloud OÜ; see THIRD_PARTY_NOTICES.md.
import { MAX_INPUT_TOKENS, validateLabels } from "./model.mjs";

const WORD_PATTERN =
  /(?:https?:\/\/[^\s]+|www\.[^\s]+)|[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}|@[a-z0-9_]+|[\p{L}\p{N}\p{M}_\u200c\u200d]+(?:[-_][\p{L}\p{N}\p{M}_\u200c\u200d]+)*|\S/giu;

export function prepareInput(text, labels, tokenizer) {
  validateLabels(labels);
  const normalized = /[.!?]$/.test(text) ? text : text + ".";
  const words = Array.from(normalized.matchAll(WORD_PATTERN), (match) => ({
    value: match[0].toLowerCase(),
    start: match.index,
    end: match.index + match[0].length,
  }));
  const schema = ["(", "[P]", "entities", "("];
  for (const label of labels) schema.push("[E]", label);
  schema.push(")", ")", "[SEP_TEXT]");
  const inputIds = [];
  const wordIndices = [];
  const queryIndices = [];
  for (const [index, token] of schema.entries()) {
    if (index >= 4 && index < schema.length - 3 && index % 2 === 0)
      queryIndices.push(inputIds.length);
    inputIds.push(
      ...tokenizer.encode(token, { add_special_tokens: false }).ids,
    );
  }
  for (const word of words) {
    wordIndices.push(inputIds.length);
    const ids = tokenizer.encode(word.value, { add_special_tokens: false }).ids;
    if (!ids.length)
      throw new Error(
        "A word produced no tokens; cannot preserve entity offsets",
      );
    if (inputIds.length + ids.length > MAX_INPUT_TOKENS)
      throw new RangeError(
        `Input and labels exceed ${MAX_INPUT_TOKENS} model tokens; split the input into smaller passages`,
      );
    inputIds.push(...ids);
  }
  if (inputIds.length > MAX_INPUT_TOKENS)
    throw new RangeError(`Labels exceed ${MAX_INPUT_TOKENS} model tokens`);
  return { inputIds, wordIndices, queryIndices, words };
}
