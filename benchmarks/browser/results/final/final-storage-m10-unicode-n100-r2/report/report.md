# Browser reversible pseudonymization benchmark — final

This report is generated only from the selected immutable JSONL receipts. Pilot and final receipts are analyzed separately. Each row represents an independent profile trial; repeated operations within a trial do not add samples. Percentiles use linear interpolation over sorted independent trial durations. They are reported only at 100 or more independent eligible profile trials per scenario/backend/interval, which provides at least five observations in the upper 5% tail. Below that floor, percentiles are blank and marked exploratory; no single observation is presented as a percentile. A blank percentile can also mean no correctness-eligible measured duration was available.

## Trial summary

| Scenario | Backend | Independent trials | Failures | Failure rate | Unsupported | Explicit exclusions | Eligible |
|---|---|---:|---:|---:|---:|---:|---:|
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | 100 | 0 | 0.0% | 0 (0.0%) | 0 (0.0%) | 100 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | 100 | 0 | 0.0% | 0 (0.0%) | 0 (0.0%) | 100 |

## Timing summary

| Scenario | Backend | Interval | n | Percentile status | p50 (ms) | p95 (ms) | Unsupported | Measurement failures |
|---|---|---|---:|---|---:|---:|---:|---:|
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | adapterOpenAndUnlock | 100 | reported | 120.750 | 155.420 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | approvalToSavedActivation | 100 | reported | 3.800 | 5.415 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | authenticatedBundleRecovery | 100 | reported | 124.600 | 163.560 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | backupExport | 0 | exploratory_insufficient_independent_trials |  |  | 100 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | backupImport | 0 | exploratory_insufficient_independent_trials |  |  | 100 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | conversationIntentPersistence | 100 | reported | 2.500 | 3.520 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | existingVaultContainerOpen | 100 | reported | 10.200 | 13.645 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | existingWorkspaceReopen | 100 | reported | 1.300 | 1.600 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | historyPopulation | 100 | reported | 0.100 | 0.100 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | initialVaultPasswordUnlock | 100 | reported | 110.350 | 148.330 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | initialWorkspaceOpen | 100 | reported | 1.400 | 2.005 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | observedSendPersistence | 100 | reported | 2.100 | 2.900 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | postRestartPasswordUnlock | 100 | reported | 111.950 | 141.625 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | postRestartReopen | 100 | reported | 4.400 | 5.105 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | postRestartScopedRestore | 100 | reported | 0.600 | 0.700 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | postRestartSdkInitialization | 100 | reported | 7.200 | 10.815 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | postRestartVaultOpen | 100 | reported | 15.500 | 23.435 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | sdkInitialization | 100 | reported | 7.100 | 11.505 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | trialPreparation | 100 | reported | 0.600 | 0.700 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | vaultContainerOpen | 100 | reported | 10.700 | 14.705 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | vaultCreate | 100 | reported | 127.000 | 170.050 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | vaultPasswordUnlock | 100 | reported | 109.400 | 141.960 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | warmScopedRestore | 100 | reported | 0.100 | 0.200 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | fogvault | workspaceCreate | 100 | reported | 4.500 | 6.610 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | adapterOpenAndUnlock | 100 | reported | 95.850 | 115.180 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | approvalToSavedActivation | 100 | reported | 6.550 | 9.615 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | backupExport | 100 | reported | 38.750 | 43.620 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | backupRecovery | 100 | reported | 139.850 | 174.710 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | conversationIntentPersistence | 100 | reported | 4.400 | 6.310 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | existingWorkspaceReopen | 100 | reported | 1.300 | 2.605 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | historyPopulation | 100 | reported | 0.100 | 0.105 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | initialVaultPasswordUnlock | 100 | reported | 93.600 | 125.320 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | initialWorkspaceOpen | 100 | reported | 1.500 | 2.810 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | observedSendPersistence | 100 | reported | 3.900 | 4.800 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | postRestartOpenAndUnlock | 100 | reported | 99.300 | 129.705 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | postRestartReopen | 100 | reported | 4.600 | 6.070 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | postRestartScopedRestore | 100 | reported | 0.600 | 0.705 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | postRestartSdkInitialization | 100 | reported | 7.250 | 11.510 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | sdkInitialization | 100 | reported | 7.200 | 12.015 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | trialPreparation | 100 | reported | 0.600 | 0.900 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | vaultCreate | 100 | reported | 118.950 | 150.165 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | vaultPasswordUnlock | 100 | reported | 94.500 | 113.095 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | warmScopedRestore | 100 | reported | 0.100 | 0.200 | 0 | 0 |
| storage_workspace-m10-fresh-mostly_new-unicode-s1 | sqlite | workspaceCreate | 100 | reported | 5.100 | 7.115 | 0 | 0 |

## Failures, unsupported trials, and exclusions

Admission failures and failed correctness gates are retained in the failure rate. Unsupported correctness gates and explicit exclusions are counted separately. All three classes are listed below and omitted from performance percentiles.

No excluded trials.

## Key observations

This final run covers one synthetic storage/Workspace scenario: 10 new mappings, fresh workspace, Unicode input, 100 independent profile trials per backend. All 200 trials were admitted and passed all nine required correctness gates. No failures, unsupported gates, or explicit exclusions occurred. Each pair ran sequentially with alternating backend order and a full Chrome process restart.

| Interval | SQLite3MC p50 / p95 (ms) | FogVault p50 / p95 (ms) |
|---|---:|---:|
| Vault create | 118.950 / 150.165 | 127.000 / 170.050 |
| Adapter open and unlock (combined components) | 95.850 / 115.180 | 120.750 / 155.420 |
| Approval to saved activation | 6.550 / 9.615 | 3.800 / 5.415 |
| Observed-send persistence | 3.900 / 4.800 | 2.100 / 2.900 |
| Warm scoped restore | 0.100 / 0.200 | 0.100 / 0.200 |
| Post-restart scoped restore | 0.600 / 0.705 | 0.600 / 0.700 |
| Encrypted backup archive export | 38.750 / 43.620 | Unsupported |
| Recovery preserving mappings and authority | 139.850 / 174.710 (encrypted backup import) | 124.600 / 163.560 (authenticated bundle migration) |

These results show workload-specific tradeoffs: SQLite has lower combined open/unlock time; FogVault has lower approval-save and observed-send persistence time in this one scenario. They do not establish an overall winner. Recovery mechanisms differ, and archive export/import is unavailable in the published FogVault adapter.

## Context and coverage

- Frozen benchmark source commit: `67fcc506c55bce8854af5cd19d5c00c386c98986`.
- Packages: DataFog Core and Core-owned adapters 1.1.0; FogVault 0.1.0; SQLite3 Multiple Ciphers 2.6.1. SHA-256 manifests and package archives are retained beside the raw receipts.
- Browser and host: installed retail Google Chrome 157.0.8103.40, macOS 26.6.2, Apple M5 Pro, 18 logical CPUs, 48 GiB RAM. Power state was unavailable.
- SQLite used the shipped adapter defaults. FogVault used its published default durability with 64 MiB, three-pass KDF, `arraybuffer` storage profile, and published limits. No settings were adjusted for measurement.
- Conformance coverage: SQLite used the exact published artifacts in retail Chrome 157; FogVault used its installed-package Playwright Chromium suite. Timed trials used retail Chrome for both backends.
- Only this one synthetic workload was run to n=100. Other mapping sizes, history levels, value reuse profiles, input shapes, complete-extension CNN mode, operating systems, browsers, and physical devices remain unmeasured.
- Timings use `performance.now()` inside the extension page. Main-process RSS was sampled with `ps`; renderer/utility process memory, per-file ciphertext size, reliable bytes-written counts, and instrumentation overhead calibration are unavailable.
- No follow-up performance issues are indicated by this single scenario. Expand coverage before making product-wide performance recommendations.
