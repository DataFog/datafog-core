FROM ubuntu:24.04

ARG RUST_VERSION=1.88.0
ARG NODE_VERSION=24
ARG PYTHON_VERSION=3.12
ARG WASM_BINDGEN_VERSION=0.2.127

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

RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain ${RUST_VERSION} --profile minimal \
    && . "$HOME/.cargo/env" \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked

ENV PATH="/root/.cargo/bin:${PATH}"

RUN python3 -m pip install --break-system-packages maturin huggingface_hub

RUN npx playwright install-deps chromium

RUN useradd -m -s /bin/bash -G sudo ubuntu \
    && echo "ubuntu ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER ubuntu
WORKDIR /home/ubuntu

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain ${RUST_VERSION} --profile minimal \
    && . "$HOME/.cargo/env" \
    && rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked

ENV PATH="/home/ubuntu/.cargo/bin:${PATH}"

# Install Playwright Chromium browser for the agent user
RUN npx playwright install chromium

WORKDIR /workspace
