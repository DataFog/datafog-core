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

export function transform(text, findings, strategy) {
  if (typeof text !== "string") {
    throw new TypeError("transform text must be a string");
  }
  if (!Array.isArray(findings)) {
    throw new TypeError("transform findings must be an array");
  }
  if (typeof strategy !== "string") {
    throw new TypeError("transform strategy must be a string");
  }

  return nativeTransform(text, findings, strategy);
}

export function scanAndTransform(text, strategy) {
  if (typeof text !== "string") {
    throw new TypeError("scanAndTransform text must be a string");
  }
  if (typeof strategy !== "string") {
    throw new TypeError("scanAndTransform strategy must be a string");
  }

  return nativeScanAndTransform(text, strategy);
}
