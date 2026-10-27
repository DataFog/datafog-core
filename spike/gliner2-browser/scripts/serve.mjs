#!/usr/bin/env node
/**
 * Simple HTTP server for spike testing.
 * Serves static files with proper MIME types for WASM.
 */
import { createServer } from "node:http";
import { readFileSync, existsSync, statSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const PORT = process.env.PORT || 8765;

const MIME_TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
  ".onnx": "application/octet-stream",
  ".data": "application/octet-stream",
};

function resolve(url) {
  let path = url.split("?")[0];
  if (path === "/") path = "/index.html";
  
  // Try public/ first, then root
  const publicPath = join(ROOT, "public", path);
  if (existsSync(publicPath) && statSync(publicPath).isFile()) {
    return publicPath;
  }
  
  // Try root (for src/, models/, node_modules/)
  const rootPath = join(ROOT, path);
  if (existsSync(rootPath) && statSync(rootPath).isFile()) {
    return rootPath;
  }
  
  // Check node_modules for onnxruntime-web
  if (path.includes("onnxruntime-web")) {
    const nmPath = join(ROOT, "node_modules", path.replace(/^\/node_modules\//, ""));
    if (existsSync(nmPath)) {
      return nmPath;
    }
  }
  
  return null;
}

const server = createServer((req, res) => {
  const file = resolve(req.url);
  
  if (!file) {
    res.writeHead(404);
    res.end("Not found");
    return;
  }
  
  const ext = extname(file);
  const contentType = MIME_TYPES[ext] || "application/octet-stream";
  
  try {
    const data = readFileSync(file);
    res.writeHead(200, { 
      "Content-Type": contentType,
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    });
    res.end(data);
  } catch (err) {
    res.writeHead(500);
    res.end("Server error");
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`GLiNER2 spike server running at http://127.0.0.1:${PORT}`);
  console.log(`Press Ctrl+C to stop\n`);
});
