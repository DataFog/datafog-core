#!/usr/bin/env bash
# Fail when the devcontainer feature options drift from the repo's pin files.
#
# Feature installs happen at image-build time, before the repo is mounted, so
# versions must be passed as options in .devcontainer/devcontainer.json. This
# guard keeps those options identical to the files that remain the source of
# truth for every other consumer (CI, .cursor/Dockerfile via pin.sh, local
# toolchains). Update the pin file and the option in the same commit.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="$REPO_ROOT/.devcontainer/devcontainer.json"
FEATURE_PREFIX="ghcr.io/datafog/devcontainer-features"

fail=0

option_value() {
    # $1 = feature id, $2 = option name
    python3 - "$CONFIG" "$FEATURE_PREFIX/$1" "$2" <<'PY'
import json, sys
config_path, feature, option = sys.argv[1:4]
with open(config_path) as handle:
    config = json.load(handle)
features = config.get("features", {})
key = next((k for k in features if k.split(":")[0] == feature), None)
value = features.get(key, {}).get(option) if key else None
print("" if value is None else value)
PY
}

expect() {
    # $1 = description, $2 = expected, $3 = actual
    if [ "$2" != "$3" ]; then
        printf 'FAIL: %s\n  pin file: %s\n  devcontainer.json: %s\n' \
            "$1" "$2" "$3" >&2
        fail=1
    else
        printf 'ok: %s (%s)\n' "$1" "$2"
    fi
}

expect "node version matches .nvmrc" \
    "$(tr -d '[:space:]' < "$REPO_ROOT/.nvmrc")" \
    "$(option_value node-pinned version)"

expect "rust toolchain matches rust-toolchain.toml" \
    "$(awk -F'"' '/^channel/ {print $2}' "$REPO_ROOT/rust-toolchain.toml")" \
    "$(option_value rust-pinned toolchain)"

expect "rust components match rust-toolchain.toml" \
    "$(python3 - "$REPO_ROOT/rust-toolchain.toml" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
match = re.search(r'^components\s*=\s*\[(.*?)\]', text, re.M | re.S)
print(",".join(part.strip().strip('"') for part in match.group(1).split(",") if part.strip()) if match else "")
PY
)" \
    "$(option_value rust-pinned components)"

expect "wasm-bindgen matches Cargo.lock" \
    "$(grep -A2 '^name = "wasm-bindgen"$' "$REPO_ROOT/Cargo.lock" | awk -F'"' '/^version/ {print $2; exit}')" \
    "$(option_value rust-pinned wasm_bindgen)"

expect "playwright matches bindings/wasm/package.json" \
    "$(python3 -c "import json;print(json.load(open('$REPO_ROOT/bindings/wasm/package.json'))['devDependencies']['playwright'])")" \
    "$(option_value playwright version)"

exit "$fail"
