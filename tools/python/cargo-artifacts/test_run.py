from __future__ import annotations

import importlib.util
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("run.py")
spec = importlib.util.spec_from_file_location("cargo_artifacts", MODULE_PATH)
assert spec and spec.loader
cargo_artifacts = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = cargo_artifacts
spec.loader.exec_module(cargo_artifacts)


class CargoArtifactTests(unittest.TestCase):
    def tree(self, name: str, size: int, age: float, *, current=False, unsafe=()):
        return cargo_artifacts.BuildTree(
            Path("/root") / name,
            name,
            size,
            1,
            1_000.0 - age,
            age,
            current,
            tuple(unsafe),
        )

    def test_managed_root_is_repo_scoped(self):
        self.assertEqual(
            cargo_artifacts.managed_root({"CARGO_HOME": "/tmp/cargo"}),
            Path("/tmp/cargo/ripr-swarm-build"),
        )

    def test_inventory_accepts_only_cargo_hash_leaves_and_protects_current(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw) / "ripr-swarm-build"
            valid = root / "ab" / "0123456789abcd"
            invalid = root / "not-a-shard" / "0123456789abcd"
            valid.mkdir(parents=True)
            invalid.mkdir(parents=True)
            (valid / "artifact").write_bytes(b"x" * 12)
            entries = cargo_artifacts.inventory(root, valid, time.time())
            self.assertEqual([entry.relative for entry in entries], ["ab/0123456789abcd"])
            self.assertEqual(entries[0].bytes, 12)
            self.assertTrue(entries[0].current)

    def test_gc_never_selects_current_or_unsafe_tree(self):
        current = self.tree("aa/00000000000000", 50, 100, current=True)
        unsafe = self.tree("bb/00000000000000", 50, 100, unsafe=("link",))
        stale = self.tree("cc/00000000000000", 50, 100)
        selected = cargo_artifacts.select_gc([current, unsafe, stale], 1_000, 10, 0)
        self.assertEqual([tree.relative for tree in selected.trees], [stale.relative])
        self.assertEqual(selected.reasons[stale.relative], ("ttl",))

    def test_budget_uses_oldest_eligible_tree(self):
        oldest = self.tree("aa/00000000000000", 60, 900)
        newer = self.tree("bb/00000000000000", 60, 800)
        selected = cargo_artifacts.select_gc([newer, oldest], 70, 10_000, 10)
        self.assertEqual([tree.relative for tree in selected.trees], [oldest.relative])
        self.assertEqual(selected.projected_bytes, 60)
        self.assertEqual(selected.reasons[oldest.relative], ("budget",))

    def test_budget_refuses_young_tree_and_reports_residual(self):
        young = self.tree("aa/00000000000000", 100, 1)
        selected = cargo_artifacts.select_gc([young], 20, 10_000, 10)
        self.assertEqual(selected.trees, ())
        self.assertEqual(selected.unresolved_over_budget_bytes, 80)

    def test_current_target_preserves_ripr_only(self):
        with tempfile.TemporaryDirectory() as raw:
            workspace = Path(raw)
            target = workspace / "target"
            (target / "ripr" / "reports").mkdir(parents=True)
            (target / "debug").mkdir()
            (target / "temp-leak").mkdir()
            metadata = cargo_artifacts.CargoMetadata(
                workspace,
                target,
                workspace / "cargo/ripr-swarm-build/aa/0123456789abcd",
            )
            self.assertEqual(
                [path.name for path in cargo_artifacts.target_entries(metadata)],
                ["debug", "temp-leak"],
            )
            self.assertTrue((target / "ripr").exists())

    def test_remove_tree_refuses_outside_path(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw) / "ripr-swarm-build"
            root.mkdir()
            outside = Path(raw) / "outside"
            outside.mkdir()
            with self.assertRaises(cargo_artifacts.ArtifactError):
                cargo_artifacts.remove_tree(outside, root)
            self.assertTrue(outside.exists())

    @unittest.skipIf(os.name == "nt", "symlink creation may require developer mode")
    def test_measurement_marks_symlink_unsafe_without_following(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw) / "tree"
            outside = Path(raw) / "outside"
            root.mkdir()
            outside.mkdir()
            (outside / "secret").write_bytes(b"secret")
            (root / "link").symlink_to(outside, target_is_directory=True)
            measured = cargo_artifacts.measure_tree(root)
            self.assertEqual(measured.bytes, 0)
            self.assertEqual(len(measured.unsafe), 1)


if __name__ == "__main__":
    unittest.main()
