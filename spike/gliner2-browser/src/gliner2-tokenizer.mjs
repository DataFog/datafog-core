/**
 * GLiNER2 preprocessing using @huggingface/transformers.
 *
 * This matches the Python GLiNER2 SchemaTransformer + ExtractorCollator
 * preprocessing used by the okasi ONNX export.
 */

import { AutoTokenizer } from "@huggingface/transformers";

const DEFAULT_LABELS = [
  "name",
  "address",
  "email",
  "phone_num",
  "id_num",
  "url",
  "username",
];

export class Gliner2Preprocessor {
  constructor(tokenizer, entityLabels = DEFAULT_LABELS) {
    this.tokenizer = tokenizer;
    this.entityLabels = entityLabels;

    const vocab = tokenizer.model?.vocab;
    const addedTokens = tokenizer.added_tokens_encoder || {};

    this.pId = addedTokens["[P]"] ?? vocab?.get("[P]") ?? null;
    this.eId = addedTokens["[E]"] ?? vocab?.get("[E]") ?? null;
    this.sepTextId =
      addedTokens["[SEP_TEXT]"] ?? vocab?.get("[SEP_TEXT]") ?? null;

    if (this.pId === null || this.eId === null || this.sepTextId === null) {
      console.warn(
        "[GLiNER2] Missing special tokens. This may not be a GLiNER2 tokenizer.",
        { pId: this.pId, eId: this.eId, sepTextId: this.sepTextId }
      );
    }
  }

  static async fromPretrained(modelId) {
    const tokenizer = await AutoTokenizer.from_pretrained(modelId, {
      local_files_only: false,
    });
    return new Gliner2Preprocessor(tokenizer);
  }

  static async fromLocal(tokenizerJsonPath) {
    const tokenizer = await AutoTokenizer.from_pretrained(tokenizerJsonPath, {
      local_files_only: true,
    });
    return new Gliner2Preprocessor(tokenizer);
  }

  splitIntoWords(text) {
    const words = [];
    const pattern = /\S+/g;
    let match;

    while ((match = pattern.exec(text)) !== null) {
      words.push({
        word: match[0],
        start: match.index,
        end: match.index + match[0].length,
      });
    }

    return words;
  }

  prepareInputs(text, labels = null) {
    const activeLabels = labels || this.entityLabels;
    const words = this.splitIntoWords(text);

    const schemaPrefix = "[P] entities";
    const schemaLabels = activeLabels
      .map((label) => `[E] ${label.toLowerCase()}`)
      .join(" ");
    const schemaText = `${schemaPrefix} ${schemaLabels} [SEP_TEXT]`;

    const schemaEncoded = this.tokenizer(schemaText, {
      add_special_tokens: false,
      return_tensors: false,
    });

    const schemaTokenCount = schemaEncoded.input_ids.length;

    const textTokens = [];
    const wordIndices = [];
    const startMapping = [];
    const endMapping = [];

    for (let wordIdx = 0; wordIdx < words.length; wordIdx++) {
      const { word, start, end } = words[wordIdx];

      wordIndices.push(schemaTokenCount + textTokens.length);

      const wordEncoded = this.tokenizer(word, {
        add_special_tokens: false,
        return_tensors: false,
      });

      const wordTokenIds = wordEncoded.input_ids;
      const tokenCount = wordTokenIds.length;

      for (let i = 0; i < tokenCount; i++) {
        textTokens.push(wordTokenIds[i]);
        startMapping.push(start);
        endMapping.push(end);
      }
    }

    const inputIds = [...schemaEncoded.input_ids, ...textTokens];
    const attentionMask = new Array(inputIds.length).fill(1);

    return {
      inputIds,
      attentionMask,
      textWordIndices: wordIndices,
      textWordCounts: words.length,
      startMapping,
      endMapping,
      words,
      labels: activeLabels,
      schemaTokenCount,
    };
  }

  prepareFeed(text, labels = null) {
    const prepared = this.prepareInputs(text, labels);

    return {
      feed: {
        input_ids: [prepared.inputIds],
        attention_mask: [prepared.attentionMask],
        text_word_indices: [prepared.textWordIndices],
        text_word_counts: [prepared.textWordCounts],
      },
      start_mapping: prepared.startMapping,
      end_mapping: prepared.endMapping,
      words: prepared.words,
      labels: prepared.labels,
      text,
    };
  }

  async tokenize(text) {
    const encoded = this.tokenizer(text, {
      add_special_tokens: false,
      return_tensors: false,
    });
    return {
      inputIds: encoded.input_ids,
      tokens: encoded.input_ids.map((id) =>
        this.tokenizer.decode([id], { skip_special_tokens: false })
      ),
    };
  }
}

export async function compareTokenIds(
  preprocessor,
  text,
  referenceTokenIds,
  labels = null
) {
  const prepared = preprocessor.prepareInputs(text, labels);
  const matches = prepared.inputIds.length === referenceTokenIds.length;

  const diffs = [];
  const maxLen = Math.max(prepared.inputIds.length, referenceTokenIds.length);

  for (let i = 0; i < maxLen; i++) {
    const got = prepared.inputIds[i];
    const expected = referenceTokenIds[i];
    if (got !== expected) {
      diffs.push({
        position: i,
        got,
        expected,
        gotToken: got !== undefined ? preprocessor.tokenizer.decode([got]) : null,
        expectedToken:
          expected !== undefined
            ? preprocessor.tokenizer.decode([expected])
            : null,
      });
    }
  }

  return {
    match: matches && diffs.length === 0,
    ourLength: prepared.inputIds.length,
    refLength: referenceTokenIds.length,
    diffs,
    ourIds: prepared.inputIds,
    refIds: referenceTokenIds,
  };
}
