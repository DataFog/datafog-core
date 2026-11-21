"""Minimum qualification scopes: independent of the workflows they protect.

Release and queue gates share shipping scopes but keep separate identity policies.
Additional successful jobs are allowed; removing a supported scope is deliberate.
"""

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-2025")
RELEASE_SCOPES = {
    "credential-stores.yml": {
        f"Credential store / {platform}" for platform in PLATFORMS
    },
    "application-vaults.yml": {
        f"Application vault / {platform}" for platform in (
            "ubuntu-24.04", "ubuntu-24.04-arm", "macos-15", "windows-2025")
    },
    "fogvault-integration.yml": {
        *(f"Core–FogVault native / {platform}" for platform in PLATFORMS),
        *(f"Core–FogVault joint / {engine}" for engine in ("chromium", "firefox", "msedge")),
    },
    "browser-model.yml": {"extraction"},
    "browser-v1-1.yml": {"installed-extension"},
    "fogvault-native-languages.yml": {
        "Core–FogVault unified Node archive",
        *(f"Core–FogVault Node/Python / {platform}" for platform in PLATFORMS),
    },
}

QUEUE_SCOPES = {
    workflow.removesuffix(".yml"): set(scopes)
    for workflow, scopes in RELEASE_SCOPES.items()
}
QUEUE_SCOPES.update({
    "python-binding": {
        *(f"CPython {version} (ubuntu-latest)" for version in ("3.10", "3.11", "3.12", "3.13", "3.14")),
        "CPython 3.12 (macos-15)",
    },
    "model-datafog": {
        *(f"installed-model ({backend}, {platform})"
          for backend in ("datafog", "gliner2", "gliner2-general")
          for platform in ("ubuntu-24.04", "ubuntu-24.04-arm", "macos-15", "windows-latest")),
    },
    "workspace-integration": {
        f"Workspace integration ({platform})" for platform in ("ubuntu-latest", "macos-latest")
    },
    "native-python-host": {
        "Native encrypted host (ubuntu-latest, Python 3.10)",
        "Native encrypted host (ubuntu-latest, Python 3.14)",
        "Native encrypted host (macos-15, Python 3.12)",
    },
    "fuzz": {"build", *(f"fuzz ({target})" for target in ("workspace_snapshot", "utf16_range", "email_context"))},
})
