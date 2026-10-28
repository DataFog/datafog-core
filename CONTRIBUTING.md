# Contributing

Follow [AGENTS.md](AGENTS.md) for repository rules and PR writing guidance.

## Dev environment setup

Two paths to a working environment. Pick one.

### Option A: Devcontainer (recommended)

Zero local setup. Open the repo in:
- **VS Code**: Install the Dev Containers extension, then "Reopen in Container"
- **Cursor**: Same flow as VS Code
- **GitHub Codespaces**: Click "Code → Codespaces → New codespace"

The container builds from `.cursor/Dockerfile`, which reads the pin files automatically.

### Option B: Local install

Install toolchains from the repository pin files.

**macOS (Homebrew + nvm):**
```bash
brew install rustup-init
rustup-init -y && source "$HOME/.cargo/env"
rustup show                      # installs from rust-toolchain.toml

brew install nvm
nvm install                      # installs from .nvmrc

WASM_BINDGEN_VERSION=$(grep -A2 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | head -1 | cut -d'"' -f2)
cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked

pip3 install maturin

PLAYWRIGHT_VERSION=$(node -p "require('./bindings/wasm/package.json').devDependencies.playwright")
npx playwright@$PLAYWRIGHT_VERSION install chromium
```

**Linux (Ubuntu/Debian):**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup show                      # installs from rust-toolchain.toml

curl -fsSL https://fnm.vercel.app/install | bash
source ~/.bashrc                 # or restart shell
fnm install                      # installs from .nvmrc

WASM_BINDGEN_VERSION=$(grep -A2 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | head -1 | cut -d'"' -f2)
cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked

pip3 install maturin

PLAYWRIGHT_VERSION=$(node -p "require('./bindings/wasm/package.json').devDependencies.playwright")
npx playwright@$PLAYWRIGHT_VERSION install --with-deps chromium
```

### Verify your setup

```bash
cargo dfv doctor
```

**Passing output:**
```
Environment check:

  ✓ rust_toolchain: rustc 1.88.0 (...)
  ✓ python: Python 3.12.x
  ✓ maturin: maturin 1.x.x
  ✓ node: v24.x.x
  ✓ wasm_bindgen_cli: wasm-bindgen 0.2.127
  ✓ wasm_target: wasm32-unknown-unknown
  ✓ playwright_chromium: ...
  ...

All required checks passed.
```

Doctor reads versions from pin files (`rust-toolchain.toml`, `.nvmrc`, `Cargo.lock`, `bindings/wasm/package.json`). A missing pin file is a hard error. Doctor never installs anything—it only reports what to fix.

### Before reviewing a PR

| Change type | Run these | Time |
| --- | --- | --- |
| Any code change | `cargo dfv build && cargo dfv check` | ~3 min |
| Detection logic | Add `cargo dfv smoke` | +2 min |
| Docs-only | `cargo fmt --all --check` | <10 sec |

For a full verification before merge:
```bash
cargo dfv doctor && cargo dfv build && cargo dfv check && cargo dfv smoke
```

### Troubleshooting

**Wrong Rust version:**
```
✗ rust_toolchain: rustc 1.82.0
    expected: rustc 1.88.0
    fix: rustup show
```
Run `rustup show` in the repo root. The `rust-toolchain.toml` tells rustup which version to use.

**Wrong Node version:**
```
✗ node: v22.3.0
    expected: Node.js 24+
    fix: nvm install 24
```
Run `nvm install` (reads `.nvmrc`) or `fnm install`. Restart your shell if needed.

**Wrong wasm-bindgen-cli:**
```
✗ wasm_bindgen_cli: wasm-bindgen 0.2.100
    expected: wasm-bindgen 0.2.127
    fix: cargo install wasm-bindgen-cli --version 0.2.127 --locked
```
The version must exactly match `Cargo.lock`. Run the fix command shown.

**Missing Playwright Chromium:**
```
✗ playwright_chromium
    expected: Playwright Chromium browser
    fix: npx playwright@1.62.1 install chromium
```
Run the fix command. On Linux, add `--with-deps` for system dependencies.

**PATH issues:**
- Rust: Ensure `~/.cargo/bin` is in PATH. Run `source "$HOME/.cargo/env"`.
- Node: Ensure nvm/fnm shim is loaded. Restart your terminal or source your shell config.
- After installing tools, run `cargo dfv doctor` again to verify.

## Changelog policy

Effective October 22, 2026, every PR that changes user-facing
behavior must add a concise bullet under `[Unreleased]` in `CHANGELOG.md`. Earlier
changes are not backfilled; their existing release notes remain in `docs/releases/`.

Use `Added`, `Changed`, `Fixed`, `Deprecated`, `Removed`, or `Security` as appropriate.
Describe observable behavior, including API, binding, configuration, installation,
and security changes. Record only what the PR implements. Keep entries unreleased
until release preparation moves them to a dated version heading.
A PR that only moves existing entries into a release section may use the exemption below.

CI checks the complete committed PR against its actual target branch, including
stacked PRs. An entry in an earlier commit of the same PR counts; follow-up commits
do not need duplicate entries. Entries from a parent PR do not cover a child PR.
Heading-only, blank, and historical-release edits do not satisfy the check.
Reviewers must check that the entry describes the change accurately.

## Exemptions

Tests, internal refactoring, and documentation or tooling changes with no user-facing
effect may use the `no-changelog` label. Include a nonempty reason in the PR body:

```text
Changelog exemption: Adds regression tests without changing runtime behavior.
```

Both the label and reason are required. Reviewers confirm the exemption. Bot PRs
follow the same policy. CI reruns when the description, labels, commits, or target
branch change.

## Checking locally

```sh
python3 scripts/test-changelog.py
python3 scripts/check-changelog.py --base origin/main
```

The checker reads committed changes. Fetch the target branch first, and use that
branch for `--base` when checking a stacked PR. Missing base refs and malformed
event data fail the check. Local checks do not consult GitHub labels unless an
event file is supplied with `--event`.

Once the workflow lands on `main`, add `Changelog policy` to the required checks
in GitHub branch rules. The workflow alone does not prevent merging a failed check.
