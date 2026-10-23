import {Unzip, UnzipInflate} from "fflate";
import {HostError} from "./rpc.js";

export const MAX_EXPANDED_BYTES = 16 * 1024 * 1024;
const fail = () => { throw new HostError("invalid_xlsx"); };
const crcTable = Uint32Array.from({length: 256}, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? 0xedb88320 ^ value >>> 1 : value >>> 1;
  return value >>> 0;
});

/** Validate ZIP metadata before bounded streaming inflation; no filesystem extraction. */
export function xlsxParts(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u16 = offset => view.getUint16(offset, true), u32 = offset => view.getUint32(offset, true);
  if (u32(0) === 0xe011cfd0) throw new HostError("protected_xlsx");
  let end = bytes.length - 22;
  for (; end >= Math.max(0, bytes.length - 65557); end--) {
    if (u32(end) === 0x06054b50 && end + 22 + u16(end + 20) === bytes.length) break;
  }
  if (end < 0 || u32(end) !== 0x06054b50 || end + 22 + u16(end + 20) !== bytes.length || u16(end + 4) || u16(end + 6) || u16(end + 8) !== u16(end + 10)) fail();
  const count = u16(end + 10), directory = u32(end + 16);
  if (!count || count > 256) throw new HostError("resource_limit");
  if (directory + u32(end + 12) !== end) fail();
  const entries = new Map(), ranges = [];
  let cursor = directory, expanded = 0;
  const nameAt = (offset, length) => new TextDecoder("utf-8", {fatal: true}).decode(bytes.subarray(offset, offset + length));
  for (let index = 0; index < count; index++) {
    if (u32(cursor) !== 0x02014b50) fail();
    const flags = u16(cursor + 8), method = u16(cursor + 10), crc = u32(cursor + 16);
    const size = u32(cursor + 20), original = u32(cursor + 24), nameLength = u16(cursor + 28);
    const local = u32(cursor + 42), name = nameAt(cursor + 46, nameLength);
    if (flags & 1) throw new HostError("protected_xlsx");
    if (flags & ~0x080e || ![0, 8].includes(method)) throw new HostError("unsupported_xlsx");
    if (!name || name.startsWith("/") || name.includes("\\") || name.includes("\0") || name.split("/").some(part => part === "." || part === "..") || entries.has(name)) fail();
    expanded += original;
    if (expanded > MAX_EXPANDED_BYTES) throw new HostError("resource_limit");
    if (u32(local) !== 0x04034b50) fail();
    if (u16(local + 6) & 1) throw new HostError("protected_xlsx");
    if (u16(local + 6) !== flags || u16(local + 8) !== method || nameAt(local + 30, u16(local + 26)) !== name) fail();
    const dataEnd = local + 30 + u16(local + 26) + u16(local + 28) + size;
    let rangeEnd = dataEnd;
    if (flags & 8) {
      const descriptor = dataEnd + (u32(dataEnd) === 0x08074b50 ? 4 : 0);
      if (u32(descriptor) !== crc || u32(descriptor + 4) !== size || u32(descriptor + 8) !== original) fail();
      rangeEnd = descriptor + 12;
    } else if (u32(local + 14) !== crc || u32(local + 18) !== size || u32(local + 22) !== original) fail();
    if (rangeEnd > directory || (method === 0 && size !== original)) fail();
    ranges.push([local, rangeEnd]); entries.set(name, {original, size, method, crc});
    cursor += 46 + nameLength + u16(cursor + 30) + u16(cursor + 32);
  }
  if (cursor !== end) fail();
  ranges.sort((a, b) => a[0] - b[0]);
  let previous = 0;
  for (const [start, finish] of ranges) { if (start !== previous) fail(); previous = finish; }
  if (previous !== directory) fail();
  const parts = new Map(), started = new Set();
  const unzip = new Unzip(file => {
    const expected = entries.get(file.name);
    if (!expected || started.has(file.name) || file.compression !== expected.method) fail();
    started.add(file.name);
    const output = new Uint8Array(expected.original);
    let written = 0, crc = 0xffffffff;
    file.ondata = (error, chunk, final) => {
      if (error || written + chunk.length > output.length) fail();
      output.set(chunk, written); written += chunk.length;
      for (const byte of chunk) crc = crcTable[(crc ^ byte) & 255] ^ crc >>> 8;
      if (final) {
        if (written !== expected.original || ((crc ^ 0xffffffff) >>> 0) !== expected.crc) fail();
        parts.set(file.name, output);
      }
    };
    file.start();
  });
  unzip.register(UnzipInflate);
  // Bound each inflater input chunk too, so forged size metadata cannot inflate
  // an entire compressed archive before the output-size callback rejects it.
  for (let offset = 0; offset < bytes.length; offset += 1024) unzip.push(bytes.subarray(offset, offset + 1024), offset + 1024 >= bytes.length);
  if (parts.size !== entries.size) fail();
  return parts;
}
