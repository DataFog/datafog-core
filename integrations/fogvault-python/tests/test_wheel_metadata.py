"""Exact packaged metadata checks across LF and Windows CRLF source checkouts."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

SPEC = importlib.util.spec_from_file_location("installed_runner", Path(__file__).with_name("run-installed.py"))
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class WheelMetadataTests(unittest.TestCase):
    def verify(self, source, packaged=None, license_notice=b"MIT license\n", include_license=True):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory)
            (package / "README.md").write_bytes(source)
            (package / "LICENSE").write_bytes(b"MIT license\n")
            stream = io.BytesIO()
            with ZipFile(stream, "w") as archive:
                # Maturin embeds the README body and may append a final newline.
                archive.writestr("datafog_fogvault-1.0.0.dist-info/METADATA",
                                 b"Metadata-Version: 2.4\nName: datafog-fogvault\nVersion: 1.0.0\n\n" +
                                 (source if packaged is None else packaged) + b"\n")
                if include_license:
                    archive.writestr("datafog_fogvault-1.0.0.dist-info/licenses/LICENSE", license_notice)
            with ZipFile(stream) as archive:
                RUNNER.verify_wheel_metadata(archive, package)

    def test_lf_and_crlf_readmes_preserve_reviewed_unicode_and_bytes(self):
        for newline in ("\n", "\r\n"):
            with self.subTest(newline=repr(newline)):
                self.verify(("# Core–FogVault 🦀" + newline + newline + "Reviewed content." + newline).encode("utf-8"))

    def test_changed_or_omitted_readme_still_rejects(self):
        for content in (b"", b"Changed content\n"):
            with self.subTest(content=content), self.assertRaisesRegex(ValueError, "README"):
                self.verify(b"Reviewed content\n", content)

    def test_comparison_does_not_silently_normalize_packaged_source_differences(self):
        with self.assertRaisesRegex(ValueError, "README"):
            self.verify(b"Line one\nLine two\n", b"Line one\r\nLine two\r\n")

    def test_license_remains_required_and_byte_exact(self):
        with self.assertRaisesRegex(ValueError, "license"):
            self.verify(b"Reviewed content\n", license_notice=b"changed license\n")
        with self.assertRaisesRegex(ValueError, "license"):
            self.verify(b"Reviewed content\n", include_license=False)


if __name__ == "__main__":
    unittest.main()
