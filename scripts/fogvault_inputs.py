"""Identity of the actual Core WASM build/package sources, including dirty bytes."""
import hashlib
from pathlib import Path
import subprocess


def core_build_inputs(root):
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
    ).decode().split("\0")
    prefixes = ("crates/core/", "bindings/wasm/", ".cargo/")
    exact = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".nvmrc"}
    selected = {name for name in names if name in exact or name.startswith(prefixes)}
    return {name: hashlib.sha256((Path(root) / name).read_bytes()).hexdigest()
            for name in sorted(selected) if (Path(root) / name).is_file()}
