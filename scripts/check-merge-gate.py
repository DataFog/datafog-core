"""Require successful queue checks; PRs validate the gate before queue admission."""

import json
import os
import sys


TECHNICAL_WORKFLOWS = (
    "advisory", "application-vaults", "browser-model", "browser-v1-1",
    "credential-stores", "devcontainer", "dfv-check", "dfv-models",
    "fogvault-integration", "fogvault-native-languages", "fuzz", "model-datafog",
    "native-python-host", "node-binding", "python-binding", "rust-quality",
    "wasm-binding", "workspace-contract", "workspace-integration",
)


def validate(event_name, results):
    if event_name not in {"pull_request", "merge_group"}:
        raise ValueError(f"Unsupported gate event: {event_name}")
    expected = {"policy", *TECHNICAL_WORKFLOWS}
    if not isinstance(results, dict) or set(results) != expected:
        raise ValueError("Gate results must contain exactly every configured dependency")
    for name, result in results.items():
        required = "success"
        if event_name == "pull_request" and name != "policy":
            required = "skipped"
        if not isinstance(result, dict) or result.get("result") != required:
            raise ValueError(f"{name}: expected {required}, received {result!r}")


def main():
    try:
        validate(os.environ["GITHUB_EVENT_NAME"], json.loads(os.environ["GATE_RESULTS"]))
    except (KeyError, ValueError, TypeError) as error:
        print(f"Merge gate failed: {error}", file=sys.stderr)
        return 1
    print("Every dependency has the required result for this event.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
