import { scan as nativeScan } from "./native.js";

export function scan(text) {
  if (typeof text !== "string") {
    throw new TypeError("scan text must be a string");
  }

  return nativeScan(text);
}
