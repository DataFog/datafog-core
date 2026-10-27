#!/usr/bin/env node
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { realpath, stat } from "node:fs/promises";
import { extname, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(fileURLToPath(new URL("../", import.meta.url)));
const mime = {
  ".html": "text/html; charset=utf-8",
  ".mjs": "text/javascript",
  ".js": "text/javascript",
  ".json": "application/json",
  ".wasm": "application/wasm",
};

export function createModelServer() {
  return createServer(async (request, response) => {
    try {
      if (!["GET", "HEAD"].includes(request.method)) {
        response.writeHead(405);
        response.end();
        return;
      }
      const path = decodeURIComponent(
        new URL(request.url, "http://localhost").pathname,
      );
      if (
        path !== "/" &&
        !/^\/(?:src|public|models|node_modules)\//.test(path)
      ) {
        response.writeHead(404);
        response.end();
        return;
      }
      const file = await realpath(
        resolve(root, path === "/" ? "public/index.html" : "." + path),
      );
      // realpath also prevents symlinks from exposing files outside this spike.
      if (
        !file.startsWith(root + sep) ||
        !["src", "public", "models", "node_modules"].includes(
          relative(root, file).split(sep)[0],
        ) ||
        !(await stat(file)).isFile()
      ) {
        response.writeHead(404);
        response.end();
        return;
      }
      response.writeHead(200, {
        "Content-Type": mime[extname(file)] || "application/octet-stream",
        "Cross-Origin-Opener-Policy": "same-origin",
        "Cross-Origin-Embedder-Policy": "require-corp",
        "Cache-Control": "no-store",
      });
      if (request.method === "HEAD") {
        response.end();
        return;
      }
      const stream = createReadStream(file);
      stream.on("error", (error) => response.destroy(error));
      stream.pipe(response);
    } catch (error) {
      response.writeHead(
        ["ENOENT", "ENOTDIR", "EACCES"].includes(error.code) ||
          error instanceof URIError
          ? 404
          : 500,
      );
      response.end("Cannot serve requested file");
    }
  });
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const server = createModelServer();
  server.listen(Number(process.env.PORT || 8765), "127.0.0.1", () =>
    console.log(`GLiNER2.5 Small: http://127.0.0.1:${server.address().port}`),
  );
}
