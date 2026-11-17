# Native credential-store qualification

Application-vault release qualification uses actual macOS Keychain, Windows
Credential Manager and Linux Secret Service. The existing process-local provider
tests remain useful for deterministic failures; they do not substitute for these
native-provider runs.

The `Native credential stores` workflow builds the current native qualification
probe and records source identity, executable hashes, commands, platform details
and per-phase results. A missing provider, unexpected protection mode, failed
assertion, incomplete cleanup or timed-out required phase fails qualification.
Reports contain no data keys, credential passwords or protected source text.

The probe creates uniquely named synthetic applications and deletes only the
exact service/account entries it owns. Its creation recorder forwards unchanged
arguments to the real platform credential builder and returns the actual native
credential object; it records identifiers for cleanup, not secret contents.
Fresh-process reopen and writer phases exercise uninstrumented defaults.

Qualification covers default credential-store creation, saved-marker restoration
from a fresh executable, multiple writers, missing/wrong-key failures without
replacement or file-key downgrade, and cleanup. Linux runs an actual isolated
D-Bus session and GNOME Keyring daemon with private XDG directories. Disposable
macOS CI may use an owned temporary keychain and restores the runner's original
settings afterward; local tests never change the user's default/search list or
lock their login keychain.

## Release evidence

The publication gate requires all three exact workflow scopes:

- `Credential store / macos-15`
- `Credential store / windows-2025`
- `Credential store / ubuntu-24.04`

Evidence must come from a successful push or manual run at the exact release
commit. Green PR checks, an older candidate, a skipped provider job or a failed
rerun do not authorize publication. All existing release gates remain required.

## Boundaries

Hosted runners qualify real providers in those runner environments. Subprocess
persistence is not machine-reboot or new-login persistence. Sid explicitly
deferred those checks until dedicated restartable hosts are available; they must
not be inferred from a credential persistence flag or service restart.

The real-provider probe is a Rust executable using the shared public
`ApplicationVault` implementation. Installed Python binding contracts are tested
separately. A deployed host's executable identity, code signature, logon session
and Keychain access policy require host-specific validation; same-binary child
processes do not qualify every consuming application or an executable upgrade.

Locked-store outcomes depend on OS session and prompt policy. Linux's upstream
provider can request an interactive unlock and wait for a prompt. A hard timeout
is a failed required probe, not a controlled application error. A prompt dismissed
in headless CI does not establish a bounded desktop interaction policy. Any
noninteractive fixture policy must be identified in its evidence rather than
presented as universal default behavior.
