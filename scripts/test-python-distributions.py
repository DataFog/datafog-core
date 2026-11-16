#!/usr/bin/env python3
"""Exercise archive validation failures without modifying candidate artifacts."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("distributions", Path(__file__).with_name("check-python-distributions.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class DistributionChecks(unittest.TestCase):
    def test_wheel_requires_matching_notices_and_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.whl"
            def write(omit=None, version="1.4.0", notices=lambda value: value):
                prefix = "datafog_core-1.4.0.dist-info/"
                with zipfile.ZipFile(path, "w") as archive:
                    metadata = "Name: datafog-core\nVersion: " + version + "\n"
                    for name, source in module.LICENSES.items():
                        metadata += "License-File: licenses/" + name + "\n"
                        if name != omit:
                            archive.writestr(prefix + "licenses/licenses/" + name, notices(source.read_bytes()))
                    archive.writestr(prefix + "METADATA", metadata)
                    archive.writestr("datafog_core/py.typed", "")
            write()
            self.assertEqual(module.check(path, "1.4.0")["version"], "1.4.0")
            # Windows-built notices must validate on an LF release runner.
            write(notices=lambda value: value.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
            module.check(path, "1.4.0")
            for transform in (
                lambda value: value + b"changed legal text\n",
                lambda value: value.replace(b"\r\n", b"\n").replace(b"\n", b"\r"),
            ):
                write(notices=transform)
                with self.assertRaises(ValueError):
                    module.check(path, "1.4.0")
            write(omit="LICENSE-CC0-names.txt")
            with self.assertRaises((KeyError, ValueError)):
                module.check(path, "1.4.0")
            write(version="1.3.0")
            with self.assertRaises(ValueError):
                module.check(path, "1.4.0")

    def test_sdist_requires_root_pin_and_license(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "candidate.tar.gz"
            def write(pin, license_text=None):
                with tarfile.open(path, "w:gz") as archive:
                    files = {"LICENSE": (module.ROOT / "LICENSE").read_bytes() if license_text is None else license_text,
                             "rust-toolchain.toml": pin, "pyproject.toml": b'[project]\nversion="1.4.0"\n'}
                    for name, value in files.items():
                        info = tarfile.TarInfo("candidate/" + name)
                        info.size = len(value)
                        archive.addfile(info, io.BytesIO(value))
            write((module.ROOT / "rust-toolchain.toml").read_bytes())
            module.check(path, "1.4.0")
            license_text = (module.ROOT / "LICENSE").read_bytes().replace(b"\r\n", b"\n")
            write((module.ROOT / "rust-toolchain.toml").read_bytes(), license_text.replace(b"\n", b"\r\n"))
            module.check(path, "1.4.0")
            write((module.ROOT / "rust-toolchain.toml").read_bytes(), license_text + b"changed legal text\n")
            with self.assertRaises(ValueError):
                module.check(path, "1.4.0")
            pin = (module.ROOT / "rust-toolchain.toml").read_bytes()
            alternate_pin = pin.replace(b"\r\n", b"\n") if b"\r\n" in pin else pin.replace(b"\n", b"\r\n")
            write(alternate_pin)
            with self.assertRaises(ValueError):
                module.check(path, "1.4.0")
            write(b"wrong toolchain")
            with self.assertRaises(ValueError):
                module.check(path, "1.4.0")

    def test_license_comparison_accepts_lf_against_windows_source(self):
        self.assertTrue(module.same_license_text(b"legal text\n", b"legal text\r\n"))
        self.assertFalse(module.same_license_text(b"legal text \n", b"legal text\r\n"))

    def test_distribution_license_copies_match_sources(self):
        for name, source in module.LICENSES.items():
            self.assertEqual((module.ROOT / "bindings/python/licenses" / name).read_bytes(), source.read_bytes())


if __name__ == "__main__":
    unittest.main()
