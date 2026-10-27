/**
 * GLiNER2 preprocessing that matches the Python GLiNER2 SchemaTransformer + ExtractorCollator.
 *
 * The okasi ONNX export expects a specific input format:
 *
 * Schema format: `( [P] entities ( [E] label1 [E] label2 ... ) ) [SEP_TEXT]`
 * Text format: tokenized words after [SEP_TEXT]
 *
 * Key differences from naive tokenization:
 * 1. Punctuation is split into separate words
 * 2. Schema has parentheses wrapping around [P] and labels
 * 3. word_indices point to the first token of each word
 * 4. start_mapping/end_mapping are per-word character positions
 */

import { AutoTokenizer, env } from "@huggingface/transformers";

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
  }

  static async fromPretrained(modelId, options = {}) {
    env.allowRemoteModels = !options.localOnly;
    env.useBrowserCache = false;

    const tokenizer = await AutoTokenizer.from_pretrained(modelId, {
      local_files_only: options.localOnly || false,
    });
    return new Gliner2Preprocessor(tokenizer, options.labels || DEFAULT_LABELS);
  }

  splitIntoWords(text) {
    const words = [];
    const pattern = /\S+/g;
    let match;

    while ((match = pattern.exec(text)) !== null) {
      const token = match[0];
      const tokenStart = match.index;

      let leadingPunct = "";
      let trailingPunct = "";
      let core = token;

      while (core.length > 0 && /[.,!?;:'"\-–—()\[\]{}]/.test(core[0])) {
        leadingPunct += core[0];
        core = core.slice(1);
      }

      while (core.length > 0 && /[.,!?;:'"\-–—()\[\]{}]/.test(core[core.length - 1])) {
        trailingPunct = core[core.length - 1] + trailingPunct;
        core = core.slice(0, -1);
      }

      let offset = tokenStart;

      for (const ch of leadingPunct) {
        words.push({ word: ch, start: offset, end: offset + 1 });
        offset++;
      }

      if (core.length > 0) {
        words.push({ word: core, start: offset, end: offset + core.length });
        offset += core.length;
      }

      for (const ch of trailingPunct) {
        words.push({ word: ch, start: offset, end: offset + 1 });
        offset++;
      }
    }

    return words;
  }

  encodeWord(word) {
    const encoded = this.tokenizer(word, {
      add_special_tokens: false,
      return_tensors: false,
    });
    const data = encoded.input_ids.data || encoded.input_ids;
    return Array.from(data).map(Number);
  }

  prepareInputs(text, labels = null) {
    const activeLabels = labels || this.entityLabels;
    const words = this.splitIntoWords(text);

    const schemaText = ` ( [P] entities ( ${activeLabels.map((l) => `[E] ${l}`).join(" ")} ) ) [SEP_TEXT]`;

    const schemaEncoded = this.tokenizer(schemaText, {
      add_special_tokens: false,
      return_tensors: false,
    });
    const schemaIds = Array.from(schemaEncoded.input_ids.data || schemaEncoded.input_ids).map(Number);

    const textTokens = [];
    const wordIndices = [];
    const startMapping = [];
    const endMapping = [];

    for (const { word, start, end } of words) {
      wordIndices.push(schemaIds.length + textTokens.length);
      startMapping.push(start);
      endMapping.push(end);

      const wordWithSpace = " " + word.toLowerCase();
      const wordTokens = this.encodeWord(wordWithSpace);
      textTokens.push(...wordTokens);
    }

    const inputIds = [...schemaIds, ...textTokens];
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
      schemaTokenCount: schemaIds.length,
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

  decode(tokenIds) {
    return this.tokenizer.decode(tokenIds, { skip_special_tokens: false });
  }
}
