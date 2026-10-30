# Native registry candidate qualification

`python3 scripts/qualify-native-registry.py` verifies unpublished Core,
migration and native adapter 1.0.0 archives alongside the eight FogVault 0.1.0
and renamed-fork candidates. It never publishes a package.

The default Fog source is the pinned checkout prepared beneath Core's
`target/fogvault-integration/dependencies/fogvault`. Its clean Git revision must
match the native adapter's declared dependency. An explicit `--fogvault-source`
can select another reviewed checkout. `--allow-dirty-fogvault` is reserved for
uncommitted local candidates: the report retains their commit, dirty status and
actual source hashes rather than claiming they match an existing release.

The command copies sources into a disposable combined workspace, changes only
that copy's native Git dependency into a versioned path dependency, and removes
nested workspace boundaries. It seeds the workspace lock from the exact existing
approved registry identity/checksum union. Compatible versions already present in
that union may unify; every package graph rejects new versions or checksums.
Source locks remain unchanged.

Workspace metadata provisioning may fetch missing registry index entries and
archives on a cold host. Its resolved graph must pass the same exact approved
identity/checksum checks before any archive is accepted. The independent final
consumer continues to run offline in a fresh Cargo home.

Normal `cargo package --workspace --locked` builds and verifies every archive.
There is no `--no-verify`. The command checks that normalized manifests resolve
registry dependencies without paths, Git sources or patches and include readmes
and MIT notices. Each actual archive's registry graph is retained in the report.

A fresh Cargo home then uses a local candidate registry and checksum-verified
official archives. The existing native adapter integration suite runs against
registry dependencies, including the exact published Core `=0.5.0-beta.3`
bidirectional bundle regression. The synthetic binding fixture is copied into
that consumer; only its relative fixture path changes. The consumer graph is
checked against candidate archive checksums and the approved registry union.

New output directories under `target/native-registry-*/` preserve command logs,
archive hashes, actual graphs and source identities. Failures remain failures;
these checks do not establish independent cryptographic review, real operating
system control isolation or registry publication approval.
