import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from verify_installed_wheel import verify


class InstalledWheelIdentity(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        payload = self.root / "qualified-payload" / "v0.11.0" / "x86_64-pc-windows-msvc"
        payload.mkdir(parents=True)
        self.identity_file = payload / "payload-identity.json"
        self.binary = self.root / "ripr.exe"
        self.binary.write_bytes(b"qualified")
        self.hash = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        self.identity = {
            "kind": "ripr_final_native_payload", "schema_version": 1,
            "source": {"commit_sha": "a" * 40, "tree_sha": "b" * 40},
            "target": "x86_64-pc-windows-msvc", "version": "0.11.0", "executable": "ripr.exe",
            "files": [{"role": "executable", "path": "ripr.exe", "sha256": self.hash}],
        }
        self.download = {"url": "file:///ripr_rs-0.11.0-py3-none-win_amd64.whl",
                         "archive_info": {"hashes": {"sha256": "c" * 64}}}

    def verify(self):
        self.identity_file.write_text(json.dumps(self.identity), encoding="utf-8")
        (self.root / "pip-report.json").write_text(
            json.dumps({"install": [{"download_info": self.download}]}), encoding="utf-8")
        return verify(self.root, self.root, "x86_64-pc-windows-msvc", "0.11.0", "a" * 40, "win_amd64")

    def test_smoke_path_names_verified_executable_and_receipt_distinguishes_hashes(self):
        (self.root / "ripr").write_bytes(b"extensionless sibling")
        self.assertEqual(self.verify(), self.binary)
        receipt = json.loads((self.root / "installed-wheel-identity.json").read_bytes())
        self.assertEqual(receipt["executable_sha256"], self.hash)
        self.assertEqual(receipt["wheel_sha256"], "c" * 64)

    def test_substitution_refuses_before_receipt(self):
        self.binary.write_bytes(b"different")
        with self.assertRaisesRegex(ValueError, "pip-installed ripr.exe SHA-256"):
            self.verify()
        self.assertFalse((self.root / "installed-wheel-identity.json").exists())

    def test_foreign_candidate_refuses(self):
        self.identity["source"]["commit_sha"] = "d" * 40
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.verify()

    def test_wrong_wheel_tag_refuses(self):
        self.download["url"] = self.download["url"].replace("win_amd64", "win32")
        with self.assertRaisesRegex(ValueError, "expected win_amd64"):
            self.verify()
