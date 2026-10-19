# Model package release

Target: `0.4.5-beta.1` for Cargo/npm and `0.4.5b1` for Python.
See [model sessions](model-sessions.md) and [GLiNER2 setup](gliner2-model.md).

Both backends use the same Core-owned errors, findings and coordinate conversion.
Python and Node are thin bindings. GLiNER2 requires its separately installed local
Python worker; ordinary Core imports do not load model runtime dependencies.

## Checks before publication

- Run the existing Rust, Python, Node and WASM package checks.
- Explicitly run model integration on Linux x86_64 GNU, Linux ARM64 GNU,
  macOS ARM64, macOS Intel and Windows x86_64 MSVC through Rust and installed
  Python/Node packages. Check loading, result/error contracts, offsets and cleanup.
- Review the final diff and test results before publishing.
- Build and publish through the existing package-specific workflows. Include the
  optional `datafog-gliner2-worker` wheel and source distribution with Python Core.
- Use npm's explicit `beta` tag for root/native packages, preserve `latest`, and
  mark the GitHub release as a prerelease.

Model integration is manually dispatched and caches checksum-verified pinned
assets. Routine CI does not download models. Accuracy evaluation, performance
benchmarks and cross-run release-artifact gates are outside this package scope.
