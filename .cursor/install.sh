#!/usr/bin/env bash
set -euo pipefail

# DataFog Core cloud agent environment install script
# Provisions model weights for cargo dfv verification.
#
# Idempotency is per engine: skip only when that engine's
# bundle-manifest.json exists. A bundle directory without a manifest is
# treated as a partial failure and removed before provisioning again.
# models/manifest.json is always rebuilt at the end.

echo "=== DataFog Core Environment Install ==="

# Add ~/.local/bin to PATH for hf CLI
export PATH="$HOME/.local/bin:$PATH"

# Rust, Node, wasm-bindgen-cli, maturin, huggingface_hub, and Playwright
# are installed by the Dockerfile. This script only handles:
# - npm dependencies for bindings
# - Model weights provisioning
# - Combined manifest creation

# Install npm dependencies for WASM binding (includes Playwright)
echo "Installing WASM binding dependencies..."
cd bindings/wasm
npm install
cd ../..

# Install npm dependencies for Node binding
echo "Installing Node binding dependencies..."
cd bindings/node
npm install
cd ../..

echo "Provisioning model weights..."
MODELS_DIR="$PWD/models"
mkdir -p "$MODELS_DIR"

write_combined_manifest() {
    echo "Creating combined models/manifest.json..."
    python3 - <<'PY'
import json
from pathlib import Path

models_dir = Path("models")
manifest = {}

for engine in ("gliner2", "gliner2-base-v1", "datafog"):
    engine_manifest = models_dir / engine / "bundle-manifest.json"
    if not engine_manifest.exists():
        continue
    files = json.loads(engine_manifest.read_text()).get("files", {})
    for name, info in files.items():
        manifest[f"{engine}/{name}"] = {
            "path": f"{engine}/{name}",
            "sha256": info["sha256"],
            "bytes": info["bytes"],
        }

(models_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
print(f"Created manifest with {len(manifest)} entries")
PY
}

# Determine platform
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  PLATFORM="linux-x86_64" ;;
    Linux-aarch64) PLATFORM="linux-aarch64" ;;
    Darwin-arm64)  PLATFORM="macos-arm64" ;;
    Darwin-x86_64) PLATFORM="macos-x86_64" ;;
    *)             PLATFORM="" ;;
esac

# DataFog model (platform-specific native runtime)
DATAFOG_DEST="$MODELS_DIR/datafog"
if [[ -f "$DATAFOG_DEST/bundle-manifest.json" ]]; then
    echo "Skipping DataFog model (found $DATAFOG_DEST/bundle-manifest.json)"
elif [[ -z "$PLATFORM" ]]; then
    echo "Skipping DataFog model (unsupported platform: $(uname -s)-$(uname -m))"
else
    if [[ -d "$DATAFOG_DEST" ]]; then
        echo "Removing incomplete DataFog bundle (no bundle-manifest.json)"
        rm -rf "$DATAFOG_DEST"
    fi
    echo "Downloading DataFog model for $PLATFORM..."
    DATAFOG_ARCHIVE="datafog-pii-0.1.0-runtime-0.2.0-$PLATFORM.tar.gz"
    DATAFOG_TMP="/tmp/datafog-assets"
    mkdir -p "$DATAFOG_TMP"

    hf download DataFog/pii-en-65m \
        "runtimes/v0.2.0/$DATAFOG_ARCHIVE" \
        --revision 7cadd41e21f59b2a149e31378d073cf6ca057531 \
        --local-dir "$DATAFOG_TMP"

    python3 scripts/provision-datafog-model.py \
        --archive "$DATAFOG_TMP/runtimes/v0.2.0/$DATAFOG_ARCHIVE" \
        --destination "$DATAFOG_DEST"

    rm -rf "$DATAFOG_TMP"
fi

# GLiNER2 model (portable, CPU-only)
GLINER2_DEST="$MODELS_DIR/gliner2"
if [[ -f "$GLINER2_DEST/bundle-manifest.json" ]]; then
    echo "Skipping GLiNER2 model (found $GLINER2_DEST/bundle-manifest.json)"
else
    if [[ -d "$GLINER2_DEST" ]]; then
        echo "Removing incomplete GLiNER2 bundle (no bundle-manifest.json)"
        rm -rf "$GLINER2_DEST"
    fi
    echo "Downloading GLiNER2 model..."
    GLINER2_TMP="/tmp/gliner2-download"
    mkdir -p "$GLINER2_TMP"

    hf download fastino/gliner2-privacy-filter-PII-multi \
        config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
        --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 \
        --local-dir "$GLINER2_TMP"

    python3 scripts/provision-gliner2-model.py \
        --source "$GLINER2_TMP" \
        --destination "$GLINER2_DEST"

    rm -rf "$GLINER2_TMP"
fi

# GLiNER2 base-v1 (issue #87 regression checkpoint; portable, CPU-only)
GLINER2_BASE_DEST="$MODELS_DIR/gliner2-base-v1"
if [[ -f "$GLINER2_BASE_DEST/bundle-manifest.json" ]]; then
    echo "Skipping GLiNER2 base-v1 (found $GLINER2_BASE_DEST/bundle-manifest.json)"
else
    if [[ -d "$GLINER2_BASE_DEST" ]]; then
        echo "Removing incomplete GLiNER2 base-v1 bundle (no bundle-manifest.json)"
        rm -rf "$GLINER2_BASE_DEST"
    fi
    echo "Downloading GLiNER2 base-v1 for entity-count regression..."
    GLINER2_BASE_TMP="/tmp/gliner2-base-v1-download"
    mkdir -p "$GLINER2_BASE_TMP"

    hf download fastino/gliner2-base-v1 \
        config.json encoder_config/config.json tokenizer.json tokenizer_config.json \
        added_tokens.json special_tokens_map.json spm.model model.safetensors README.md \
        --revision f9634218e53580c56edf0de97ca1a7d3f1c2354e \
        --local-dir "$GLINER2_BASE_TMP"

    python3 scripts/provision-gliner2-model.py \
        --source "$GLINER2_BASE_TMP" \
        --model-id fastino/gliner2-base-v1 \
        --revision f9634218e53580c56edf0de97ca1a7d3f1c2354e \
        --destination "$GLINER2_BASE_DEST"

    rm -rf "$GLINER2_BASE_TMP"
fi

write_combined_manifest

echo "=== Install complete ==="
echo "Run 'cargo dfv doctor' to verify the environment."
