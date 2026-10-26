#!/usr/bin/env bash
set -euo pipefail

# DataFog Core cloud agent environment install script
# Installs toolchains and provisions model weights for cargo dfv verification

echo "=== DataFog Core Environment Install ==="

# Rust 1.88
echo "Installing Rust 1.88..."
if command -v rustup &>/dev/null; then
    rustup install 1.88 --profile minimal
    rustup default 1.88
else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.88 --profile minimal
    source "$HOME/.cargo/env"
fi
rustup target add wasm32-unknown-unknown

# Python 3.12 and maturin
echo "Installing Python dependencies..."
sudo apt-get update -qq
sudo apt-get install -y -qq python3-dev python3.12-venv 2>/dev/null || true
python3 -m pip install --upgrade pip
python3 -m pip install maturin

# Node.js 24 (if not already installed)
echo "Checking Node.js..."
if ! command -v node &>/dev/null || [[ "$(node --version | cut -d. -f1 | tr -d 'v')" -lt 24 ]]; then
    echo "Installing Node.js 24..."
    curl -fsSL https://deb.nodesource.com/setup_24.x | sudo -E bash -
    sudo apt-get install -y nodejs
fi

# wasm-bindgen-cli 0.2.127
echo "Installing wasm-bindgen-cli..."
cargo install wasm-bindgen-cli --version 0.2.127 --locked

# Playwright Chromium
echo "Installing Playwright Chromium..."
cd bindings/wasm
npm install
npx playwright install --with-deps chromium
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

# DataFog model (if platform supported and huggingface-cli available)
if [[ -n "$PLATFORM" ]] && command -v huggingface-cli &>/dev/null; then
    echo "Downloading DataFog model for $PLATFORM..."
    DATAFOG_ARCHIVE="datafog-pii-0.1.0-runtime-0.2.0-$PLATFORM.tar.gz"
    DATAFOG_TMP="/tmp/datafog-assets"
    mkdir -p "$DATAFOG_TMP"
    
    huggingface-cli download DataFog/pii-en-65m \
        "runtimes/v0.2.0/$DATAFOG_ARCHIVE" \
        --revision 7cadd41e21f59b2a149e31378d073cf6ca057531 \
        --local-dir "$DATAFOG_TMP" || true
    
    if [[ -f "$DATAFOG_TMP/runtimes/v0.2.0/$DATAFOG_ARCHIVE" ]]; then
        python3 scripts/provision-datafog-model.py \
            --archive "$DATAFOG_TMP/runtimes/v0.2.0/$DATAFOG_ARCHIVE" \
            --destination "$MODELS_DIR/datafog" || true
    fi
    rm -rf "$DATAFOG_TMP"
fi

# GLiNER2 model (portable, CPU-only)
if command -v huggingface-cli &>/dev/null; then
    echo "Downloading GLiNER2 model..."
    GLINER2_TMP="/tmp/gliner2-download"
    mkdir -p "$GLINER2_TMP"
    
    huggingface-cli download fastino/gliner2-privacy-filter-PII-multi \
        config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
        --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 \
        --local-dir "$GLINER2_TMP" || true
    
    if [[ -d "$GLINER2_TMP" ]] && [[ -f "$GLINER2_TMP/model.safetensors" ]]; then
        python3 scripts/provision-gliner2-model.py \
            --source "$GLINER2_TMP" \
            --destination "$MODELS_DIR/gliner2" || true
    fi
    rm -rf "$GLINER2_TMP"
fi

# Create combined manifest for doctor
if [[ -d "$MODELS_DIR" ]]; then
    echo "Creating models manifest..."
    python3 - <<'MANIFEST_SCRIPT'
import json
import hashlib
from pathlib import Path

models_dir = Path("models")
manifest = {}

for bundle_dir in models_dir.iterdir():
    if not bundle_dir.is_dir():
        continue
    bundle_manifest = bundle_dir / "bundle-manifest.json"
    if bundle_manifest.exists():
        data = json.loads(bundle_manifest.read_text())
        for name, identity in data.get("files", {}).items():
            full_path = f"{bundle_dir.name}/{name}"
            manifest[full_path] = {
                "path": full_path,
                "sha256": identity["sha256"]
            }

if manifest:
    (models_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True))
    print(f"Created manifest with {len(manifest)} files")
MANIFEST_SCRIPT
fi

echo "=== Install complete ==="
echo "Run 'cargo dfv doctor' to verify the environment."
