"""Skip expensive PR checks only for an explicit set of human prose paths."""
import os
import subprocess


def prose(path):
    # Contract drafts include executable examples and machine-readable inputs.
    if path.startswith(b"docs/contracts/"):
        return False
    return (
        (path.startswith(b"docs/") and path.endswith((b".md", b".mdx")))
        or (path.startswith(b".cursor/skills/") and path.endswith(b".md"))
        or path in {b"README.md", b"CHANGELOG.md", b"CONTRIBUTING.md", b"ROADMAP.md", b"AGENTS.md"}
    )


def requires_checks(event, base, head):
    if event != "pull_request" or not base or not head:
        return True
    try:
        result = subprocess.run(
            ["git", "diff", "--no-renames", "--name-only", "-z", base, head, "--"],
            check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError):
        return True
    paths = result.stdout.split(b"\0")[:-1]
    return not paths or any(not prose(path) for path in paths)


if __name__ == "__main__":
    value = requires_checks(os.environ.get("EVENT", ""), os.environ.get("BASE", ""), os.environ.get("HEAD", ""))
    print(f"code={str(value).lower()}")
