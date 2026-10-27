#!/usr/bin/env bash
set -euo pipefail

# DataFog Core cloud agent environment install script
# Provisions model weights for cargo dfv verification.
#
# Idempotency: checks for existing models/manifest.json before downloading.
# Uses 'hf download' (from huggingface_hub) matching dfv-models.yml workflow.

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

# Model weights provisioning
echo "Provisioning model weights..."
MODELS_DIR="$PWD/models"

# Check idempotency via combined manifest
if [[ -f "$MODELS_DIR/manifest.json" ]]; then
    echo "Models already provisioned (found $MODELS_DIR/manifest.json)"
    echo "=== Install complete ==="
    echo "Run 'cargo dfv doctor' to verify the environment."
    exit 0
fi

mkdir -p "$MODELS_DIR"

# Determine platform
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  PLATFORM="linux-x86_64" ;;
    Linux-aarch64) PLATFORM="linux-aarch64" ;;
    Darwin-arm64)  PLATFORM="macos-arm64" ;;
    Darwin-x86_64) PLATFORM="macos-x86_64" ;;
    *)             PLATFORM="" ;;
esac

# DataFog model (platform-specific native runtime)
if [[ -n "$PLATFORM" ]]; then
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
        --destination "$MODELS_DIR/datafog"
    
    rm -rf "$DATAFOG_TMP"
else
    echo "Skipping DataFog model (unsupported platform: $(uname -s)-$(uname -m))"
fi

# GLiNER2 model (portable, CPU-only)
echo "Downloading GLiNER2 model..."
GLINER2_TMP="/tmp/gliner2-download"
mkdir -p "$GLINER2_TMP"

hf download fastino/gliner2-privacy-filter-PII-multi \
    config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
    --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 \
    --local-dir "$GLINER2_TMP"

python3 scripts/provision-gliner2-model.py \
    --source "$GLINER2_TMP" \
    --destination "$MODELS_DIR/gliner2"

rm -rf "$GLINER2_TMP"

# Create combined manifest for doctor (same as dfv-models.yml)
echo "Creating combined models/manifest.json..."
python3 - <<'PY'
import json
from pathlib import Path

models_dir = Path("models")
manifest = {}

# GLiNER2 manifest
gliner2_manifest_path = models_dir / "gliner2" / "bundle-manifest.json"
if gliner2_manifest_path.exists():
    gliner2_manifest = json.loads(gliner2_manifest_path.read_text())
    for name, info in gliner2_manifest.get("files", {}).items():
        manifest[f"gliner2/{name}"] = {
            "path": f"gliner2/{name}",
            "sha256": info["sha256"],
            "bytes": info["bytes"]
        }

# DataFog manifest
datafog_manifest_path = models_dir / "datafog" / "bundle-manifest.json"
if datafog_manifest_path.exists():
    datafog_manifest = json.loads(datafog_manifest_path.read_text())
    for name, info in datafog_manifest.get("files", {}).items():
        manifest[f"datafog/{name}"] = {
            "path": f"datafog/{name}",
            "sha256": info["sha256"],
            "bytes": info["bytes"]
        }

(models_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
print(f"Created manifest with {len(manifest)} entries")
PY

echo "=== Install complete ==="
echo "Run 'cargo dfv doctor' to verify the environment."
