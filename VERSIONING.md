# Versioning and compatibility policy

DataFog Core follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).
This document defines what a version number promises. The API surface itself is
inventoried in [`docs/contracts/core-v1.json`](docs/contracts/core-v1.json) and
enforced by `scripts/check-core-contract.py`.

## What the numbers mean

- **MAJOR** — an inventoried export, signature, error code or range unit is
  removed or redesigned incompatibly, a documented transformation is intentionally
  redesigned, or a contract version bumps. Corrective privacy fixes follow the
  minor-release rule below.
- **MINOR** — additive work. New detectors, new entities, new transformations, new
  bindings or targets, new optional engines.
- **PATCH** — fixes that do not change findings, spans, or error codes for an
  input that already worked.

### Finding output is a minor change

The finding set is part of the contract. A patch must not start matching more or
fewer spans. If a fix changes what is found, it ships as a MINOR with the affected
entities named in the changelog.

### Corrective privacy behavior is a minor change

Privacy fixes that restore masking, complete detected-span coverage, safe input
validation or confidentiality ship as a MINOR when public entry points and wire
contracts remain available. They may tighten unsafe edge cases and change
finding or transformation output; document the behavior and cover it with tests.
This differs from removing an API or intentionally redesigning a transformation.

Core 1.5 applies this rule to bounded mask reveal, scanner-overlap normalization,
reserved-token validation and safe Python provider/configuration handling. It
retains the API surface and the existing model and Workspace contract identifiers.

### Contract versions are major

`WORKSPACE_CONTRACT_VERSION` and the model `contract_version` are protocol
identifiers. Either bump is MAJOR work, and a dependent SDK raises its floor on
the same release.

### Declarations and inventory move together

`docs/contracts/core-v1.json` records reviewed exports and declarations by
digest. A change that does not update the inventory fails
`scripts/check-core-contract.py`, so an unrecorded public surface cannot ship.

## Deprecation window

An inventoried export is deprecated in a MINOR and removed no earlier than the
second following MINOR or 90 days, whichever is longer. Deprecations name the last
supporting version as an exact installable pin and the MAJOR that removes it.

At the removal MAJOR the name stays present as a stub that raises with the same
pin, rather than disappearing.

## Experimental surface

Experimental code is marked in its reference page and docstrings, and may change
in a MINOR:

- GLiNER2 and model sessions other than the embedded CNN
- browser/WASM workspace entry points beyond the published contract
- contributed engine definitions

## Platform policy

A platform is supported when the release publishes wheels for it. Targets are:

- `manylinux_2_28` x86_64 and aarch64
- `musllinux_1_2` x86_64 and aarch64
- `manylinux_2_28` s390x and powerpc64le
- macOS 10.12+ x86_64, macOS 11+ arm64
- Windows amd64 and arm64

The ABI3 wheels support GIL-enabled CPython 3.10 and newer. PyPy and
free-threaded CPython require a source build, as do glibc older than 2.28,
musl older than 1.2, and targets outside the list above. Source builds require
Rust 1.88 or newer, Cargo and a target linker, and may fetch build dependencies.
The sdist includes `rust-toolchain.toml` and builds with that exact pin when
Rustup is installed. An offline host needs the toolchain and Cargo dependencies
provisioned already. `pip install --only-binary=:all: datafog-core` fails clearly
when no compatible wheel exists instead of starting a source build. Dropping a target is a MINOR. Adding one is a MINOR.

## Support window

The latest two MINOR releases of the current MAJOR receive fixes. Older MINORs
receive security fixes only. Every published version stays installable.

## Yanking

A broken PATCH is yanked and replaced by the next PATCH. A broken MINOR or MAJOR
is superseded rather than yanked, and the changelog opens with the replacement.

## Downstream

`datafog-core` versions independently of the `datafog` Python application, which
consumes `capabilities()` and pins this package exactly for reproducible output.
Bumping the contract requires a matching floor there in the same release train.
