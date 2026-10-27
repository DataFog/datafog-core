#!/usr/bin/env bash
set -euo pipefail

# DataFog Core cloud agent environment install script
# Installs toolchains and provisions model weights for cargo dfv verification
#
# Idempotency: checks for existing bundle-manifest.json before downloading models.
# Uses 'hf download' (from huggingface_hub) matching dfv-models.yml workflow.

echo "=== DataFog Core Environment Install ==="

# Skip packages already installed by Dockerfile
# Rust, Node, and wasm-bindgen-cli are in the Dockerfile
# This script only handles:
# - maturin and huggingface_hub pip packages
# - Playwright browser install
# - npm dependencies for bindings
# - Model weights provisioning

# Python dependencies (maturin and huggingface_hub for provisioning)
echo "Installing Python dependencies..."
python3 -m pip install --upgrade pip
python3 -m pip install maturin 'huggingface_hub>=0.25.0'

# Playwright Chromium browser (Dockerfile installs deps, we need the browser)
echo "Installing Playwright Chromium browser..."
cd bindings/wasm
npm install
npx playwright install chromium
cd ../..

# Install npm dependencies for Node binding
echo "Installing Node binding dependencies..."
cd bindings/node
npm install
cd ../..

# Model weights provisioning
echo "Provisioning model weights..."
MODELS_DIR="$PWD/models"
mkdir -p "$MODELS_DIR"

# Determine platform
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  PLATFORM="linux-x86_64" ;;
    Linux-aarch64) PLATFORM="linux-aarch64" ;;
    Darwin-arm64)  PLATFORM="macos-arm64" ;;
    Darwin-x86_64) PLATFORM="macos-x86_64" ;;
    *)             PLATFORM="" ;;
esac

# DataFog model (if platform supported and not already provisioned)
DATAFOG_MANIFEST="$MODELS_DIR/datafog/bundle-manifest.json"
if [[ -n "$PLATFORM" ]] && [[ ! -f "$DATAFOG_MANIFEST" ]]; then
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
elif [[ -f "$DATAFOG_MANIFEST" ]]; then
    echo "DataFog model already provisioned (found $DATAFOG_MANIFEST)"
fi

# GLiNER2 model (portable, CPU-only, if not already provisioned)
GLINER2_MANIFEST="$MODELS_DIR/gliner2/bundle-manifest.json"
if [[ ! -f "$GLINER2_MANIFEST" ]]; then
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
elif [[ -f "$GLINER2_MANIFEST" ]]; then
    echo "GLiNER2 model already provisioned (found $GLINER2_MANIFEST)"
fi

echo "=== Install complete ==="
echo "Run 'cargo dfv doctor' to verify the environment."
