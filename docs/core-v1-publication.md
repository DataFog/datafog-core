# Core 1.0 publication runbook

Release set: Core SDKs and Core-owned integration packages **1.0.0**, with
independently versioned FogVault **0.1.0**. The release owner authorized this
publication on October 30, 2026. Authorization does not replace any failing
candidate, registry permission or installed-consumer check.

## Select and qualify the immutable source

Use clean checkouts of the reviewed protected-branch merge commits. Record the
full Core and FogVault revisions before beginning. Leave unrelated documentation
and branding work in other checkouts untouched. Never move an existing release
tag or replace an uploaded package version.

Run the read-only gates against the **final Core commit**, not a PR merge ref or
an earlier green candidate:

```sh
GITHUB_REPOSITORY=DataFog/datafog-core GITHUB_SHA="$CORE_RELEASE_REV" python3 scripts/check-fogvault-release.py
GITHUB_REPOSITORY=DataFog/datafog-core GITHUB_SHA="$CORE_RELEASE_REV" python3 scripts/check-model-integration.py
python3 scripts/check-release.py rust
```

The joint gate requires all eleven native/browser, browser-model and installed
language-package scopes across three workflows. The native model gate requires
three backends on each of four supported platforms. Intel macOS model jobs are
advisory under the existing policy; their results must not be described as
required passes while still running. The ordinary verifier and contract inventory
must also pass. Any changed final source commit needs fresh exact-head evidence.

Preserve each selected workflow's ID, attempt, full head revision, successful
required jobs, artifact download identity and individual archive SHA-256. Download
only artifacts from the qualifying exact-head runs; a similarly named artifact
from a PR or another revision is insufficient.

## Publish dependencies before consumers

| Stage | Registry packages, in dependency order |
| --- | --- |
| FogVault retained forks | `fogvault-i18n-config@0.4.8` → `fogvault-i18n-embed-impl@0.8.4` → `fogvault-i18n-embed@0.16.0` → `fogvault-i18n-embed-fl@0.10.1` → `fogvault-age@0.11.5` |
| FogVault public API | `fogvault@0.1.0` and `fogvault-backup@0.1.0`, then `fogvault-native@0.1.0`; npm `@datafog/fogvault-browser@0.1.0` |
| Core base SDKs | crates.io/PyPI `datafog-core@1.0.0`; npm `@datafog/wasm@1.0.0`; all five `@datafog/node` platform packages, then `@datafog/node@1.0.0` |
| Core Rust integrations | `datafog-fogvault-migration@1.0.0`, then `datafog-fogvault-native@1.0.0` |
| Core browser integrations | `@datafog/core-fogvault-browser@1.0.0`, then `@datafog/core-fogvault-legacy-browser@1.0.0` |
| Core native language integrations | npm `@datafog/core-fogvault-node@1.0.0` and PyPI `datafog-fogvault==1.0.0` |

Wait for each Rust dependency's exact version to appear in the public index
before publishing its consumer. Disabling a feature does not remove Cargo's
registry metadata requirement. The FogVault WASM Rust crate is internal and is
not published independently; its compiled bytes ship in the FogVault browser npm
archive. Fork versions retain upstream numbering and original license notices.

The existing `publish-rust.yml`, `publish-node.yml`, `publish-python.yml` and
`publish-wasm.yml` workflows own the base SDK uploads. Their corresponding tag
patterns are `rust-v1.0.0`, `node-v1.0.0`, `python-v1.0.0` and `wasm-v1.0.0`;
pushing these tags triggers real publication. Their manual runs default to safe
dry runs. Do not create any publication tag before its exact source gates pass.

Push at most three publication tags in one operation. GitHub suppresses tag push
workflow events when more than three tags are pushed together; an atomic push of
all four SDK tags therefore creates the refs without starting these publishers.
See the [GitHub push-event documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#push).
If tags already exist, leave them unchanged. Inspect existing runs for each
workflow and exact release revision before dispatching a missing publisher;
never dispatch a duplicate while a matching upload is running or already complete.
Use `workflow_dispatch` on the unchanged SDK tag with `dry_run=false` only when
its previous run and public registry state establish that publication is still
needed. Manual dry runs retain their default `dry_run=true`.

The companion publication workflow, `publish-companions.yml`, handles the new
FogVault/integration release packages under the same release coordination. Keep
its dry run enabled until namespace ownership, registry permissions, predecessor
availability and archive identities are verified. Select only the current
stage's package or ecosystem so an earlier dependency failure cannot be ignored.

## Archive and registry checks

- Rust: retain the normal Cargo package verification output, normalized manifests
  and approved dependency identities. Run public-registry dry runs after the
  predecessors exist. Compare the candidate's source/manifest and dependency
  content before uploading; do not use `--no-verify` or a local source override to
  bypass missing registry dependencies.
- Native Node: publish the **single unified archive** from the successful exact-head
  `Core–FogVault unified Node archive` job. Check its `assembly.json` revision,
  successful installed receipt and tarball SHA-256. It contains the three qualified
  targets: `darwin-arm64`, `linux-x64-gnu`, and `win32-x64-msvc`. A runner's local
  single-target tarball must never substitute for the unified distribution.
- Browser npm: use the actual packed candidates identified by the successful joint
  browser reports. Validate archive name/version and digest against those reports;
  retain the bundled worker/WASM and third-party notices. Do not rebuild an
  unqualified replacement during upload.
- Python: use only the wheels named by successful exact-head installed Python
  receipts from each qualified runner. Validate each wheel's SHA-256 and internal
  distribution/version/target metadata. The stable runtime and distribution
  versions must agree. Do not manufacture an untested source distribution or
  advertise additional wheel targets.
- npm: dry-run the exact tarball with lifecycle scripts disabled, then publish that
  tarball with `--ignore-scripts --access public --tag latest`. Core's base Node
  publisher uploads its platform packages before the parent package and likewise
  avoids implicit publish hooks. Never use a normal `npm publish` from the Node
  source directory as an accidental substitute for this ordered process.

Registry credentials stay in the intended GitHub environments or the registry's
trusted-publisher flow. Do not place them in commands, logs or release artifacts.
A permission or trusted-publisher failure requires an owner access correction,
not removal of the publication check.

## Partial publication and completion

Registry uploads across ecosystems are not atomic. Record each successful
name/version, archive digest and public download URL as it completes. On retry,
first inspect the public registry: skip an existing name/version only after its
published bytes match the qualified candidate. If bytes differ, stop and review;
do not overwrite, reuse a version for different contents, automatically yank,
or delete a successful release to make a rerun easier.

After uploads, use fresh consumer directories with public registry dependencies
only. Confirm Rust Core and optional native integration can build without Git,
path dependencies or patches; install the npm/Python distributions from the public
registries and rerun the applicable native/browser installed-consumer flows.
Retain beta.3 interchange and lock/reopen/read checks. Compare public npm tarball
and wheel digests against the actual uploaded candidates and verify stable
`latest` and Python runtime/distribution versions.

Create the consolidated `v1.0.0` release record and mark tracker packages released
only after their public installation checks pass. A package that uploaded while a
later stage failed is a partial release, not a completed coordinated release.
The release record links the immutable source revisions, package inventory,
checksums and retained qualification/post-publication receipts.

G1/G2/G4 shipping-extension and provider acceptance remain open under the
[agreed SDK release scope](core-v1-release-scope.md). Publication does not claim
mobile/WebKit qualification, universal vault interoperability, certification or
approval of a real product host's key storage and send observation.
