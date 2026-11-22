# Browser benchmark artifacts

`pilot/` contains the paired Chrome 157 lifecycle pilot and all exact package, extension, dependency, and fixture artifacts. The pilot harness repository was dirty, so those receipts are explicitly diagnostic.

`diagnostics/final-storage-m10-unicode-n100-qualification-loader/` retains a stopped setup attempt. A qualification-loader schema mismatch prevented the SQLite flow from starting; its receipts and staged artifacts are preserved, without the large temporary browser workspaces.

`qualification/` contains the package-hash-bound adapter conformance receipts and required tarballs. SQLite exercised 22 cases against exact published Core and SQLite adapter tarballs in retail Chrome. FogVault exercised 14 installed-package cases using the exact published Core and FogVault tarballs and the Core FogVault adapter package whose contents matched the published artifact. Fault gates are referenced by the final trial receipts.

`final/final-storage-m10-unicode-n100-r2/` contains 100 independent fresh-profile trials per backend, alternated sequentially, on one synthetic storage/Workspace Unicode scenario. Raw receipts, matrix ledger, fixture, package and extension artifacts, dependency manifests, generated report, CSV, and per-interval SVG charts are retained. The final source commit used by the trials is recorded in every receipt and in the artifact manifest.

Artifact references are repository-root-relative and indexed in `artifact-manifest.json`; resolve them from the repository root after cloning or moving the checkout. Percentiles are reported only for at least 100 independent eligible trials in the same scenario/backend/interval. The result supports only the workload and hardware/browser coverage in the report; it does not establish a product-wide winner.
