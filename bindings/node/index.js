import {
  scan as nativeScan,
  scanAndTransform as nativeScanAndTransform,
  transform as nativeTransform,
} from "./native.js";

export function scan(text) {
  if (typeof text !== "string") {
    throw new TypeError("scan text must be a string");
  }

  return nativeScan(text);
}

function validateConfig(config) {
  if (typeof config !== "object" || config === null || Array.isArray(config)) {
    throw new TypeError("transformation configuration must be an object");
  }
  if (!["redact", "mask", "remove"].includes(config.strategy)) {
    throw new TypeError("strategy must be 'redact', 'mask', or 'remove'");
  }

  const allowed =
    config.strategy === "mask"
      ? new Set(["strategy", "character", "reveal"])
      : new Set(["strategy"]);
  for (const key of Object.keys(config)) {
    if (!allowed.has(key)) {
      throw new TypeError(`unexpected configuration field: ${key}`);
    }
  }

  if (config.strategy === "mask") {
    if (config.character !== undefined) {
      if (
        typeof config.character !== "string" ||
        Array.from(config.character).length !== 1 ||
        /[\p{White_Space}\p{Cc}]/u.test(config.character)
      ) {
        throw new TypeError(
          "mask character must be one non-whitespace, non-control code point",
        );
      }
    }
    if (config.reveal !== undefined) {
      if (
        typeof config.reveal !== "object" ||
        config.reveal === null ||
        Array.isArray(config.reveal)
      ) {
        throw new TypeError("mask reveal configuration must be an object");
      }
      for (const key of Object.keys(config.reveal)) {
        if (key !== "direction" && key !== "count") {
          throw new TypeError(`unexpected reveal field: ${key}`);
        }
      }
      if (!["first", "last"].includes(config.reveal.direction)) {
        throw new TypeError("reveal direction must be 'first' or 'last'");
      }
      if (
        !Number.isSafeInteger(config.reveal.count) ||
        config.reveal.count < 0
      ) {
        throw new TypeError("reveal count must be a non-negative safe integer");
      }
    }
  }

  return config;
}

export function transform(text, findings, config) {
  if (typeof text !== "string") {
    throw new TypeError("transform text must be a string");
  }
  if (!Array.isArray(findings)) {
    throw new TypeError("transform findings must be an array");
  }

  return nativeTransform(text, findings, validateConfig(config));
}

export function scanAndTransform(text, config) {
  if (typeof text !== "string") {
    throw new TypeError("scanAndTransform text must be a string");
  }

  return nativeScanAndTransform(text, validateConfig(config));
}
