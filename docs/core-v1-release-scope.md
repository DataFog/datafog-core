# Core 1.0 release candidate scope

Decision date: October 30, 2026. The owner authorized preparing Core v1 and the
minimum FogVault packaging needed to consume it. Nothing in this document
authorizes registry publication, a release tag, or an extension product launch.

## What Core v1 commits to

Core is an independently usable SDK, with Rust, Python, native Node and browser
WASM entry points. Its public API and value representations, explicit errors,
Unicode offsets, source/record admission, workspace ownership, approved saves,
complete-head comparisons, restoration and recovery follow the reviewed
[contract inventory](contracts/core-v1.json) and
[compatibility statement](contracts/core-v1.md).

The Workspace API contract is version 1. The snapshot writer remains version 3,
with supported readers for versions 1, 2 and 3. The package version, Workspace
contract version, snapshot version, fixture versions and DFVBRG01 storage binding
are separate identifiers. An unchanged package version label is not evidence of
compatibility. The exact published Core 0.5.0-beta.3 interchange test remains a
separate regression; it is not replaced by testing the current source twice.

Core-owned FogVault integrations are included as independently installable
packages with explicit supported profiles. Their one-active-preparation policy
does not impose a new limit on raw Rust Core callers. Node/Python integration
factories currently admit password-only profiles; unsupported profiles reject
without fallback. Host authentication and protected native control storage remain
host responsibilities.

## Qualification boundary

| Area | Core v1 requirement | Separate product or later evidence |
| --- | --- | --- |
| Rules and SDK detection | Existing detector regressions, exact offsets, selected-entity routing, errors, asset identity and installed package tests pass. | New detectors, new locale/model coverage and accuracy improvements. |
| Browser model | Measure the pinned supported model with the existing scoped corpus; retain actual misses and false positives. Enforce runtime contracts and approved-value recall. | An extension launch accuracy promise or qualification across arbitrary provider models and websites. |
| Markers | Freeze current syntax and strict known/unknown/damaged/unissued-marker behavior through Core fixtures. | G1's at-least-99% external ChatGPT/Claude/Gemini preservation claim. Zero real provider runs is not a pass. |
| Snapshot and SDK stability | Review the complete public inventory, decoder fixtures, byte-preserving legacy reads/no-ops, explicit save/recovery migration and actual four-SDK parity. | New snapshot layouts or a universal external vault protocol. |
| Vault integration | Confirm real native/browser save, lock, reopen, binding, conflict, unknown-outcome and recovery behavior with the final dependency pair. | Josh's shipping extension, physical key providers, signed store installation, hardware certification and all-device resource floors. |
| Native models | Preserve supported CPU configuration, manifest/load/offset/error and tested worker lifecycle contracts. | CUDA, actual device OOM campaigns and guarantees about every user-supplied checkpoint's statistical quality. |

This separates the SDK release from extension acceptance. It does not mark the
old broader G1/G2/G4 product issues passed, lower their product thresholds or
manufacture an accuracy result. Any failure of a supported Core contract remains
a Core release blocker.

The bounded measurement command is `cargo dfv check --core-sdk`, after explicit
tool/model provisioning and a fresh SDK build. It uses the existing reference
MV3 harness as an SDK consumer; that harness is not Josh's shipping extension.
The raw quality measurements and six approved-value recall/recovery phases stay
separate. See [qualification and limitations](core-sdk-qualification.md).

## Existing issue and PR dispositions

- PR #115 remains preserved historical fixture/scoring work. Its partially
  labelled corpus contains EMAIL and DATE examples without those gold labels;
  adding it to global rule scoring penalized correct detections. Its old native
  scores are not the browser Small baseline. Use the already scoped browser
  corpus/scorer for this candidate; salvage additional fixtures separately after
  their label coverage and evaluation scope are reviewed.
- PRs #184 and #202 remain external-provider preparation, with no actual provider
  preservation result. Review/consolidate them for the product's G1 campaign.
- Issue #85 records an older native GLiNER2 checkpoint/configuration's contraction
  false positive. The browser corpus separately measures its own contractions,
  names and false positives; those observations do not close the native report.
  No blanket removal of PERSON findings is part of Core v1 preparation.
- Issue #90 remains broader native/device fault qualification. Existing tests
  cover real worker death, cancellation, busy admission and close/cleanup paths.
  They do not establish every allocator-OOM, GPU or custom backend behavior.
- CSV is owner-deferred from v1. Shared engine redesign, new engines, CUDA,
  batching, city/state entities and compression are not added to this release.

## Release sequence and evidence

1. Prepare versioned Core candidates and the reviewed contract inventory.
2. Prepare only the FogVault dependency distribution needed by Core. FogVault
   has its own 0.1.0 candidate and release schedule; Core's package number does
   not require a FogVault 1.0 label. Preserve its current backup/API behavior,
   cryptographic source provenance and upstream advisory coverage.
3. Verify package archives and clean registry-style consumers without local
   source overrides. Retain exact source, dependency, tool and artifact identity.
   An unpublished local registry proof is not a claim of public availability.
4. Run final Core SDK/reference-host acceptance, contract parity and both joint
   FogVault scopes on the exact candidate. Resolve material supported-path
   failures before release under the recorded contract freeze.
5. Finish changelog/version/notice/dependency checks and safe publication dry
   runs. Actual publication and post-publication registry checks require a later
   explicit release action. Uploads remain disabled during this preparation.

Resource observations support one pending integrated review. They are not a
global memory SLA: input/output copies, streams, models and multiple owners need
application-level admission. Supported logical limits and observed costs remain
documented; mobile/device support and shipping-host promises require their own
evidence before being advertised.
