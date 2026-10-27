"""Verify local GLiNER2 files and install a complete bundle atomically.

Use --manifest for a caller-pinned model, or --model-id and --revision to
generate a manifest from explicitly selected local files. Omit both for PII.
Downloading is separate. This script never imports or executes model code.
Requires Python 3.11+ for file_digest.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tempfile


def validate_manifest(manifest):
    if set(manifest) != {"schema_version", "model", "platform", "files"}:
        raise ValueError("invalid manifest fields")
    if type(manifest["schema_version"]) is not int or manifest["schema_version"] != 1:
        raise ValueError("unsupported manifest schema")
    model = manifest["model"]
    if not isinstance(model, dict) or set(model) != {"backend", "id", "revision"}:
        raise ValueError("invalid model identity")
    if model["backend"] != "gliner2":
        raise ValueError("this provisioner requires the gliner2 backend")
    for name in ("id", "revision"):
        value = model[name]
        if not isinstance(value, str) or not value.strip() or value.strip() != value:
            raise ValueError("model identity fields must be nonempty strings")
    if manifest["platform"] != "portable-cpu":
        raise ValueError("this worker requires portable-cpu assets")
    files = manifest["files"]
    if not isinstance(files, dict) or not files:
        raise ValueError("files must be a nonempty object")
    required = {"config.json", "encoder_config/config.json", "tokenizer.json",
                "tokenizer_config.json", "model.safetensors"}
    if not required.issubset(files):
        raise ValueError("manifest is missing required GLiNER2 span-model files")
    return manifest


def validate_asset(name, identity):
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or ".." in path.parts
            or "\\" in name or ":" in name or "." in name.split("/")):
        raise ValueError("invalid relative asset path")
    if not isinstance(identity, dict) or set(identity) != {"sha256", "bytes"}:
        raise ValueError("invalid asset identity")
    digest = identity["sha256"]
    if (not isinstance(digest, str) or len(digest) != hashlib.sha256().digest_size * 2
            or any(char not in "0123456789abcdef" for char in digest)):
        raise ValueError("invalid SHA-256 digest")
    if type(identity["bytes"]) is not int or identity["bytes"] < 0:
        raise ValueError("invalid asset byte size")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--manifest", type=Path,
                           help="an existing versioned model manifest")
    selection.add_argument("--model-id", help="create a manifest for these local model files")
    parser.add_argument("--revision", help="exact revision of the explicitly downloaded model")
    args = parser.parse_args()
    if (args.model_id is None) != (args.revision is None):
        parser.error("--model-id and --revision must be supplied together")
    source_root = args.source.resolve(strict=True)
    if args.model_id is not None:
        files = {}
        for path in sorted(source_root.rglob("*")):
            if path.is_symlink():
                raise ValueError("model source must not contain symlinks")
            if path.is_file():
                with path.open("rb") as stream:
                    digest = hashlib.file_digest(stream, "sha256").hexdigest()
                files[path.relative_to(source_root).as_posix()] = {
                    "sha256": digest, "bytes": path.stat().st_size,
                }
        manifest = validate_manifest({
            "schema_version": 1,
            "model": {"backend": "gliner2", "id": args.model_id, "revision": args.revision},
            "platform": "portable-cpu", "files": files,
        })
    elif args.manifest:
        manifest = validate_manifest(json.loads(args.manifest.read_text()))
    else:
        root = Path(__file__).resolve().parent.parent
        lock = json.loads((root / "crates/core/src/models/gliner2-lock.json").read_text())
        # Default PII checkpoint: record model identity so eval/doctor can
        # verify id+revision without hardcoded engine fallbacks.
        manifest = validate_manifest({
            "schema_version": 1,
            "model": {
                "backend": "gliner2",
                "id": lock["model"],
                "revision": lock["revision"],
            },
            "platform": "portable-cpu",
            "files": lock["files"],
        })
    for name, identity in manifest["files"].items():
        validate_asset(name, identity)
    destination = args.destination.absolute()
    if destination.exists():
        raise ValueError("destination must be a fresh directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".gliner2-install-", dir=destination.parent))
    try:
        for name, identity in manifest["files"].items():
            source = source_root / name
            if source.is_symlink() or not source.is_file():
                raise ValueError(f"missing or unsafe required asset: {name}")
            resolved = source.resolve(strict=True)
            if not resolved.is_relative_to(source_root):
                raise ValueError("asset escapes source directory")
            target = staging / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(resolved, target)
            with target.open("rb") as stream:
                actual = hashlib.file_digest(stream, "sha256").hexdigest()
            if actual != identity["sha256"] or target.stat().st_size != identity["bytes"]:
                raise ValueError(f"asset integrity mismatch: {name}")
        encoded = json.dumps(manifest, sort_keys=True).encode()
        (staging / "bundle-manifest.json").write_bytes(encoded)
        staging.rename(destination)
        print(json.dumps({"bundle": str(destination),
                          "manifest_sha256": hashlib.sha256(encoded).hexdigest()}))
    finally:
        if staging.exists():
            shutil.rmtree(staging)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, TypeError, AttributeError) as error:
        raise SystemExit(str(error)) from None
