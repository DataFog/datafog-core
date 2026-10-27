export const MODEL = Object.freeze({
  repository: "nicolasembleton/gliner2.5-small-v1-onnx",
  revision: "5e2e3f51adfb0eeb7c1f83464400b4d498d41659",
  files: Object.freeze({
    "onnx/model.onnx": {
      bytes: 288521577,
      sha256:
        "12dda5cf4b0e9ed3af17596cef1cdbd22185a32fcc230af4f476353b5e4ade1f",
    },
    "tokenizer.json": {
      bytes: 8341713,
      sha256:
        "cbc8ae6037812709c9c26f2a160f8dc48b0440bcb79c8141804259ae2d6adac3",
    },
    "tokenizer_config.json": {
      bytes: 646,
      sha256:
        "fd4a31dc2f1f17e31638c5f0e783b81cdb2fbe6bddd116a8d9e5d50d78148cf1",
    },
  }),
});

export const DEFAULT_LABELS = Object.freeze([
  "person",
  "address",
  "email",
  "phone number",
  "identification number",
  "url",
  "username",
]);
export const MAX_INPUT_TOKENS = 512;

export function validateLabels(labels) {
  if (!Array.isArray(labels) || labels.length === 0 || labels.length > 16) {
    throw new TypeError("Provide between 1 and 16 entity labels");
  }
  if (
    labels.some(
      (label) =>
        typeof label !== "string" ||
        !label.trim() ||
        label !== label.trim() ||
        label.length > 64 ||
        /[\[\]\p{C}]/u.test(label),
    )
  ) {
    throw new TypeError(
      "Labels must be nonempty, trimmed text of at most 64 characters without brackets or control characters",
    );
  }
  if (
    new Set(labels.map((label) => label.toLowerCase())).size !== labels.length
  ) {
    throw new TypeError("Entity labels must be unique");
  }
}
