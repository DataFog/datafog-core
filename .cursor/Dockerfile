FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    git \
    libssl-dev \
    pkg-config \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    python3.12-venv \
    sudo \
    wget \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

COPY .nvmrc rust-toolchain.toml Cargo.lock /opt/datafog-pins/
COPY bindings/wasm/package.json /opt/datafog-pins/bindings/wasm/package.json
COPY scripts/pin.sh /opt/datafog-pins/scripts/pin.sh
RUN chmod +x /opt/datafog-pins/scripts/pin.sh

RUN NODE_VERSION=$(/opt/datafog-pins/scripts/pin.sh node) && \
    curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

RUN PLAYWRIGHT_VERSION=$(/opt/datafog-pins/scripts/pin.sh playwright) && \
    npx playwright@${PLAYWRIGHT_VERSION} install-deps chromium

RUN python3 -m pip install --break-system-packages maturin huggingface_hub

RUN usermod -aG sudo ubuntu \
    && echo "ubuntu ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER ubuntu
WORKDIR /home/ubuntu

COPY --chown=ubuntu:ubuntu rust-toolchain.toml /home/ubuntu/rust-toolchain.toml

RUN RUST_VERSION=$(/opt/datafog-pins/scripts/pin.sh rust) && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
        --default-toolchain ${RUST_VERSION} --profile minimal && \
    . "$HOME/.cargo/env" && \
    rustup target add wasm32-unknown-unknown

ENV PATH="/home/ubuntu/.cargo/bin:${PATH}"

RUN WASM_BINDGEN_VERSION=$(/opt/datafog-pins/scripts/pin.sh wasm-bindgen) && \
    cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked

RUN PLAYWRIGHT_VERSION=$(/opt/datafog-pins/scripts/pin.sh playwright) && \
    npx playwright@${PLAYWRIGHT_VERSION} install chromium

WORKDIR /workspace
