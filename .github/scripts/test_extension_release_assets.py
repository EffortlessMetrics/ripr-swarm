"""Exercise the publisher's archive admission with a full policy-shaped inventory."""

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import tomllib
import types
import unittest

from extension_release_assets import subjects


class ReleaseAssets(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        self.args = types.SimpleNamespace(manifest=root / "manifest.json", inventory=root / "inventory.json",
                                          archives=root, repository="example/repo", version="0.11.0",
                                          commit="a" * 40, tree="b" * 40)
        policy = Path(__file__).resolve().parents[2] / "policy/distribution.toml"
        self.assets = {}
        for row in tomllib.loads(policy.read_text())["target"]:
            target, ext = row["rust_target"], row["archive"]
            name = f"ripr-server-v0.11.0-{target}.{ext}"
            payload = target.encode()
            (root / name).write_bytes(payload)
            self.assets[target] = dict(subject=name, archive_format=ext, archive_size=len(payload),
                                       sha256=hashlib.sha256(payload).hexdigest())
        self.manifest = dict(schema_version="2", source_repository=self.args.repository,
                             product_version=self.args.version, target_set=dict(targets=sorted(self.assets)),
                             build_identity=dict(repository=self.args.repository, candidate_sha=self.args.commit,
                                                 candidate_tree=self.args.tree), assets=self.assets)
        self.inventory = [dict(name=row["subject"]) for row in self.assets.values()]
        self.args.inventory.write_text(json.dumps(dict(assets=self.inventory)))
        self.write_manifest(self.manifest)

    def write_manifest(self, manifest):
        raw = json.dumps(manifest).encode()
        self.args.manifest.write_bytes(raw)
        self.args.digest = hashlib.sha256(raw).hexdigest()

    def test_exact_archives(self):
        self.assertEqual(subjects(self.args), sorted(row["name"] for row in self.inventory))

    def test_manifest_identity(self):
        for field in ("schema_version", "source_repository", "product_version"):
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "identity mismatch"):
                self.write_manifest(dict(self.manifest, **{field: "other"}))
                subjects(self.args)
        self.write_manifest(self.manifest)
        for field in ("commit", "tree"):
            original = getattr(self.args, field)
            setattr(self.args, field, "c" * 40)
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "identity mismatch"):
                subjects(self.args)
            setattr(self.args, field, original)
        self.args.digest = "0" * 64
        with self.assertRaisesRegex(ValueError, "manifest digest"):
            subjects(self.args)

    def test_manifest_subject(self):
        target = next(iter(self.assets))
        for field, value in (("subject", "../outside.zip"), ("url", "https://example.org/asset"),
                             ("archive_format", "bad"), ("archive_size", True)):
            modified = copy.deepcopy(self.manifest)
            modified["assets"][target][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "invalid archive subject"):
                self.write_manifest(modified)
                subjects(self.args)

    def test_observed_inventory(self):
        for rows in (self.inventory[:-1], self.inventory + self.inventory[:1],
                     self.inventory + [dict(name="ripr-server-v0.11.0-other.zip")]):
            self.args.inventory.write_text(json.dumps(dict(assets=rows)))
            with self.assertRaisesRegex(ValueError, "release archive"):
                subjects(self.args)

    def test_downloaded_bytes(self):
        name = self.inventory[0]["name"]
        path = self.args.archives / name
        path.write_bytes(bytes(path.stat().st_size))
        with self.assertRaisesRegex(ValueError, "archive digest mismatch"):
            subjects(self.args)
        path.write_bytes(b"")
        with self.assertRaisesRegex(ValueError, "archive size"):
            subjects(self.args)


if __name__ == "__main__":
    unittest.main()
