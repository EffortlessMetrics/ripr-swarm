"""Every generated task manifest must be isolated from an enclosing workspace."""
from pathlib import Path
import tempfile
import tomllib
import unittest

from test_scoring import TASK, load_lib


class WorkspaceIsolationTests(unittest.TestCase):
    def setUp(self):
        self.lib = load_lib()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def test_custom_manifest_gets_the_same_workspace_isolation_as_default(self):
        manifest = '[package]\nname="oracle_fixture"\nversion="0.1.0"\nedition="2021"\n[lib]\ndoctest=false\n'
        destination = str(Path(self.tmp.name) / "custom")
        self.lib.build_repo(dict(TASK, cargo_toml=manifest), destination)
        parsed = tomllib.loads((Path(destination) / "Cargo.toml").read_text())
        self.assertIn("workspace", parsed)
        self.assertEqual(parsed["workspace"], {})
        self.assertEqual(parsed["package"]["name"], "oracle_fixture")
        self.assertFalse(parsed["lib"]["doctest"])

    def test_explicit_workspace_table_is_preserved_not_duplicated(self):
        manifest = '[package]\nname="oracle_fixture"\nversion="0.1.0"\nedition="2021"\n[workspace]\nresolver="2"\n'
        destination = str(Path(self.tmp.name) / "explicit")
        self.lib.build_repo(dict(TASK, cargo_toml=manifest), destination)
        parsed = tomllib.loads((Path(destination) / "Cargo.toml").read_text())
        self.assertEqual(parsed["workspace"], {"resolver": "2"})
