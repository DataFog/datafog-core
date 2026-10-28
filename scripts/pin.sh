#!/usr/bin/env bash
# Read pinned tool versions from repository root files.
# Usage: scripts/pin.sh <tool>
#   tools: rust, node, wasm-bindgen, playwright
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

case "${1:-}" in
    rust)
        grep '^channel' "$REPO_ROOT/rust-toolchain.toml" | cut -d'"' -f2
        ;;
    node)
        cat "$REPO_ROOT/.nvmrc" | tr -d '[:space:]'
        ;;
    wasm-bindgen)
        grep -A2 '^name = "wasm-bindgen"$' "$REPO_ROOT/Cargo.lock" | grep '^version' | head -1 | cut -d'"' -f2
        ;;
    playwright)
        node -p "require('$REPO_ROOT/bindings/wasm/package.json').devDependencies.playwright"
        ;;
    *)
        echo "Usage: $0 <rust|node|wasm-bindgen|playwright>" >&2
        exit 1
        ;;
esac
