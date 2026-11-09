"""Real Cargo control for custom task manifests under an enclosing workspace."""
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

from test_scoring import TASK, load_lib


@unittest.skipUnless(shutil.which("cargo"), "native Cargo unavailable")
class NativeWorkspaceTests(unittest.TestCase):
    def test_custom_manifest_is_isolated_inside_an_enclosing_workspace(self):
        lib = load_lib()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[workspace]\nmembers=[]\n', encoding="utf-8")
            scratch = root / "scratch"
            scratch.mkdir()
            task = dict(TASK, cargo_toml='[package]\nname="oracle_fixture"\nversion="0.1.0"\nedition="2021"\n[lib]\ndoctest=false\n')
            repo, target = str(root / "agent"), str(root / "target")
            lib.build_repo(task, repo)
            with mock.patch.object(tempfile, "tempdir", str(scratch)):
                result = lib.score(task, repo, target)
            self.assertEqual(result["quality_status"], "complete", result)
            self.assertEqual(result["killed"], 1, result)
