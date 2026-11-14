# Contributing

Follow [AGENTS.md](AGENTS.md) for repository rules and PR writing guidance.

## Dev environment setup

Two paths to a working environment. Pick one.

### Option A: Devcontainer (recommended)

Zero local setup. Open the repo in:
- **VS Code**: Install the Dev Containers extension, then "Reopen in Container"
- **Cursor**: Same flow as VS Code
- **GitHub Codespaces**: Click "Code → Codespaces → New codespace"

`.devcontainer/devcontainer.json` composes the environment from feature packages in
`ghcr.io/datafog/devcontainer-features`, on an `ubuntu:24.04` base pinned by digest in the
same file; `.devcontainer/devcontainer-lock.json` pins every feature digest. Features install
at image-build time, before the checkout is mounted, so their versions are feature options in
`devcontainer.json` and are not read from the pin files. The pin files stay the source of truth
for CI and local installs, and `scripts/check-devcontainer-pins.sh` fails when an option
disagrees with one of them. Update the pin file and the matching feature option in the same
commit; when a feature legitimately resolves to a new digest, refresh the lock with
`devcontainer upgrade --workspace-folder .`. CI builds with `devcontainer build
--frozen-lockfile`, which fails instead of silently rewriting a stale lock. A
Dependabot devcontainer PR therefore needs the same commit to refresh the lock
and, for pinned tools, the matching pin file.

The Cursor **cloud** environment builds `.cursor/environment.json` → `.cursor/Dockerfile`
rather than the devcontainer, and that Dockerfile reads the pin files through `scripts/pin.sh`.
That is the reason it still exists. Its `FROM` keeps the plain `ubuntu:24.04` tag and is not
digest-pinned; only the devcontainer reference is.

### Option B: Local install

Install toolchains from the repository pin files.

**macOS (fnm + Homebrew):**
```bash
# Rust (rustup reads rust-toolchain.toml automatically)
brew install rustup
rustup default stable && source "$HOME/.cargo/env"
rustup show  # verifies pinned version from rust-toolchain.toml

# Node (fnm reads .nvmrc)
brew install fnm
echo 'eval "$(fnm env --use-on-cd)"' >> ~/.zshrc  # or ~/.bashrc
eval "$(fnm env --use-on-cd)"
fnm install

# wasm-bindgen-cli (exact version from Cargo.lock)
cargo install wasm-bindgen-cli --version "$(./scripts/pin.sh wasm-bindgen)" --locked

# wasm target (for WASM builds only)
rustup target add wasm32-unknown-unknown

# maturin (for Python builds)
pipx install maturin  # or: python3 -m venv .venv && .venv/bin/pip install maturin

# Playwright Chromium (for WASM tests)
npx playwright@$(./scripts/pin.sh playwright) install chromium
```

**Linux (Ubuntu/Debian):**
```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup show

# Node (fnm reads .nvmrc)
curl -fsSL https://fnm.vercel.app/install | bash
source ~/.bashrc
fnm install

# wasm-bindgen-cli
cargo install wasm-bindgen-cli --version "$(./scripts/pin.sh wasm-bindgen)" --locked

# wasm target
rustup target add wasm32-unknown-unknown

# maturin
pipx install maturin

# Playwright Chromium
npx playwright@$(./scripts/pin.sh playwright) install --with-deps chromium
```

### Verify your setup

Build bindings first (required for freshness checks), then run doctor:

```bash
cargo dfv build
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
  ✓ playwright_chromium: Playwright 1.62.1 (Chromium ...)
  ✓ python_binding_fresh: up to date
  ✓ node_binding_fresh: up to date
  ✓ wasm_binding_fresh: up to date
  ...

All required checks passed.
```

Doctor reads versions from pin files (`rust-toolchain.toml`, `.nvmrc`, `Cargo.lock`, `bindings/wasm/package.json`). A missing pin file is a hard error. Doctor never installs anything—it only reports what to fix.

### Before reviewing a PR

Timings measured on a Linux cloud VM (4 vCPU, 16 GB RAM):

| Change type | Run these | Time |
| --- | --- | --- |
| Any code change | `cargo dfv build && cargo dfv check` | ~75s |
| Detection logic | Add `cargo dfv smoke` | +25s |
| Docs-only | CI handles it; no local run needed | — |

For a full verification before merge:
```bash
cargo dfv build && cargo dfv doctor && cargo dfv check && cargo dfv smoke
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
    fix: Install Node >= 24 (nvm install / fnm install / mise install)
```
Run `fnm install` (reads `.nvmrc`) or your preferred version manager. Restart your shell if needed.

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
    expected: Playwright 1.62.1 with Chromium
    fix: npx playwright@1.62.1 install chromium
```
Run the fix command. On Linux, add `--with-deps` for system dependencies.

**PATH issues:**
- Rust: Ensure `~/.cargo/bin` is in PATH. Run `source "$HOME/.cargo/env"`.
- Node: Ensure fnm/nvm shim is loaded. Restart your terminal or source your shell config.
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

The Python binding has its own checks once a wheel is built:
`.venv/bin/python bindings/python/tests/test_installed.py`,
`.venv/bin/python bindings/python/tests/test_typing.py`, and
`.venv/bin/pytest bindings/python/tests/test_gil_release.py`.

The fuzz targets live in `crates/core/fuzz` and are not part of the workspace.
Run them with `cargo +nightly fuzz run <target>`; `cargo +nightly build --release
--bins --manifest-path crates/core/fuzz/Cargo.toml` catches compile breaks without
fuzzing. See `crates/core/fuzz/README.md`.

The checker reads committed changes. Fetch the target branch first, and use that
branch for `--base` when checking a stacked PR. Missing base refs and malformed
event data fail the check. Local checks do not consult GitHub labels unless an
event file is supplied with `--event`.

Once the workflow lands on `main`, add `Changelog policy` to the required checks
in GitHub branch rules. The workflow alone does not prevent merging a failed check.
