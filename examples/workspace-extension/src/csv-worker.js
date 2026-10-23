import { parse } from "csv-parse/browser/esm/sync";
import { serve, HostError } from "./rpc.js";
import { MAX_SOURCE_BYTES, MAX_TEXT_LEAVES } from "./documents.js";

serve({
  parse(bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.length > MAX_SOURCE_BYTES) throw new HostError("resource_limit");
    let text;
    try { text = new TextDecoder("utf-8", {fatal: true, ignoreBOM: true}).decode(bytes); }
    catch { throw new HostError("invalid_encoding"); }
    finally { bytes.fill(0); }
    try {
      if (text.includes("\0")) throw new HostError("invalid_encoding");
      let cells = 0;
      const rows = parse(text, {
        bom: true, cast: false, cast_date: false, columns: false, delimiter: ",",
        record_delimiter: ["\r\n", "\n", "\r"], relax_column_count: true,
        relax_quotes: false, skip_empty_lines: false, skip_records_with_error: false,
        max_record_size: MAX_SOURCE_BYTES,
        on_record(row) {
          cells += row.length;
          if (cells > MAX_TEXT_LEAVES) throw new HostError("resource_limit");
          return row;
        },
      });
      if (!rows.length) throw new HostError("invalid_csv");
      return rows;
    } catch (error) {
      if (error instanceof HostError) throw error;
      throw new HostError("invalid_csv");
    } finally { text = undefined; }
  },
});
