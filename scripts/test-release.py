"""Check RC spelling, complete package alignment, peers and release dist-tags."""
import importlib.util
import json
from pathlib import Path
import shutil
import tomllib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("release_check", Path(__file__).with_name("check-release.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseVersions(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        files = {*release.CARGO_MANIFESTS, *release.PYTHON_MANIFESTS, *release.NPM_MANIFESTS, *release.LOCKFILES}
        for relative in release.NPM_MANIFESTS:
            lock = Path(relative).with_name("package-lock.json")
            if (release.ROOT / lock).exists():
                files.add(str(lock))
        for relative in files:
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(release.ROOT / relative, target)
        # Exercise an RC independently of the repository's next release version.
        current = tomllib.loads((release.ROOT / "crates/core/Cargo.toml").read_text())["package"]["version"]
        pep = release.python_version(current)
        for relative in files:
            target = self.root / relative
            text = target.read_text().replace('"' + current + '"', '"1.1.0-rc.1"').replace('"=' + current + '"', '"=1.1.0-rc.1"')
            if relative in release.PYTHON_MANIFESTS:
                text = text.replace('"' + pep + '"', '"1.1.0rc1"').replace('version = "1.1.0-rc.1"', 'version = "1.1.0rc1"')
            for name in ("datafog-core", "datafog-fogvault-native", "datafog-fogvault-migration"):
                text = text.replace(f'"{name} {current}"', f'"{name} 1.1.0-rc.1"')
            target.write_text(text)
        owned = {json.loads((self.root / relative).read_text())["name"] for relative in release.NPM_MANIFESTS}
        for relative in release.NPM_MANIFESTS:
            target = self.root / relative
            document = json.loads(target.read_text())
            document["version"] = "1.1.0-rc.1"
            document["publishConfig"]["tag"] = "next"
            for name in document.get("peerDependencies", {}):
                if name in owned:
                    document["peerDependencies"][name] = "1.1.0-rc.1"
            target.write_text(json.dumps(document, indent=2))
            lock = target.with_name("package-lock.json")
            if lock.exists():
                locked = json.loads(lock.read_text())
                locked["version"] = locked["packages"][""]["version"] = "1.1.0-rc.1"
                if "peerDependencies" in document:
                    locked["packages"][""]["peerDependencies"] = document["peerDependencies"]
                lock.write_text(json.dumps(locked, indent=2))

    def tearDown(self):
        self.temporary.cleanup()

    def check(self, **options):
        return release.check(self.root, check_contract=False, environ=options.pop("environ", {}), **options)

    def change(self, relative, old, new):
        path = self.root / relative
        path.write_text(path.read_text().replace(old, new))

    def test_rc_aligns_every_core_owned_package_and_registry_spelling(self):
        self.assertEqual(self.check(), "1.1.0-rc.1")
        self.assertEqual(release.logical_version("1.1.0rc1"), "1.1.0-rc.1")
        self.assertEqual(release.python_version("1.1.0-rc.1"), "1.1.0rc1")
        self.assertEqual(release.npm_tag("1.1.0-rc.1"), "next")
        self.assertEqual(release.npm_tag("1.1.0-beta.2"), "beta")
        self.assertEqual(release.python_version("1.1.0-beta.2"), "1.1.0b2")
        self.assertEqual(release.npm_tag("1.1.0"), "latest")

    def test_companion_mismatch_and_lock_mismatch_fail(self):
        for relative in ("integrations/sqlite-browser/package.json", "integrations/fogvault-native/Cargo.lock"):
            path = self.root / relative
            prior = path.read_text()
            path.write_text(prior.replace("1.1.0-rc.1", "1.0.0"))
            with self.subTest(relative=relative), self.assertRaisesRegex(ValueError, "do not match"):
                self.check()
            path.write_text(prior)

    def test_pep440_spelling_is_required_for_both_python_distributions(self):
        for relative in release.PYTHON_MANIFESTS:
            self.change(relative, "1.1.0rc1", "1.1.0-rc.1")
            with self.subTest(relative=relative), self.assertRaisesRegex(ValueError, "PEP 440"):
                self.check()
            self.change(relative, "1.1.0-rc.1", "1.1.0rc1")

    def test_rc_cannot_publish_any_npm_package_as_latest(self):
        for relative in release.NPM_MANIFESTS:
            self.change(relative, '"tag": "next"', '"tag": "latest"')
            with self.subTest(relative=relative), self.assertRaisesRegex(ValueError, "npm next"):
                self.check()
            self.change(relative, '"tag": "latest"', '"tag": "next"')

    def test_core_path_dependencies_and_prerelease_peers_must_match(self):
        relative = "bindings/wasm/Cargo.toml"
        self.change(relative, 'version = "=1.1.0-rc.1"', 'version = "1.0.0"')
        with self.assertRaisesRegex(ValueError, "path dependency"):
            self.check()
        self.change(relative, 'version = "1.0.0"', 'version = "=1.1.0-rc.1"')
        self.change("integrations/sqlite-browser/package.json", '"@datafog/wasm": "1.1.0-rc.1"', '"@datafog/wasm": "^1.0.0"')
        with self.assertRaisesRegex(ValueError, "Peer requirement excludes"):
            self.check()

    def test_lockfiles_require_complete_identities_and_semver_spelling(self):
        relative = "integrations/fogvault-native/Cargo.lock"
        path = self.root / relative
        prior = path.read_text()
        path.write_text(prior.replace('name = "datafog-fogvault-native"', 'name = "missing-native"'))
        with self.assertRaisesRegex(ValueError, "lockfile identities"):
            self.check()
        path.write_text(prior.replace('version = "1.1.0-rc.1"', 'version = "1.1.0rc1"'))
        with self.assertRaisesRegex(ValueError, "SemVer release spelling"):
            self.check()

    def test_exact_rc_release_tag_is_required(self):
        self.assertEqual(self.check(prefix="wasm", environ={"GITHUB_REF_TYPE": "tag", "GITHUB_REF_NAME": "wasm-v1.1.0-rc.1"}), "1.1.0-rc.1")
        for name in ("wasm-v1.1.0rc1", "wasm-v1.1.0", "node-v1.1.0-rc.1"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Expected release tag"):
                self.check(prefix="wasm", environ={"GITHUB_REF_TYPE": "tag", "GITHUB_REF_NAME": name})

    def test_malformed_or_unsupported_versions_fail(self):
        for value in ("1.1.0-rc", "1.1.0-rc.01", "01.1.0", "1.1.0rc01", "1.1.0-dev.1", "latest"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                release.logical_version(value)


if __name__ == "__main__":
    unittest.main()
