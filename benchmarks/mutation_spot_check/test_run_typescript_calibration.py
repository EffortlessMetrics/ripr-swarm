#!/usr/bin/env python3
"""Unit tests for run_typescript_calibration helpers (#12901)."""
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "run_typescript_calibration", HERE / "run_typescript_calibration.py")
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class RunTests(unittest.TestCase):
    def test_timeout_returns_caught_sentinel(self):
        code, out = MOD.run(
            [sys.executable, "-c", "import time; time.sleep(10)"],
            Path("."), timeout=1)
        self.assertEqual(code, 124)
        self.assertEqual(out, "timeout")

    def test_utf8_output_is_not_mojibake(self):
        code, out = MOD.run(
            [sys.executable, "-c", "import sys; sys.stdout.buffer.write(chr(0x2139).encode())"], Path("."))
        self.assertEqual(code, 0)
        self.assertIn(chr(0x2139), out)


class VerdictTests(unittest.TestCase):
    def test_null_analysis_outcome_is_incomplete_not_a_crash(self):
        token, grip, disposition = MOD.verdict_for(
            {"analysis_outcome": None, "findings": []}, "src/x.ts", 1)
        self.assertEqual((token, grip, disposition), ("", "", "analysis_incomplete"))


class PathTests(unittest.TestCase):
    def test_relative_executable_resolves_from_repo_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertEqual(
                MOD.resolve_executable("target/debug/ripr", root),
                str((root / "target/debug/ripr").resolve()))

    def test_bare_and_absolute_executables_are_unchanged(self):
        root = Path("repo")
        self.assertEqual(MOD.resolve_executable("node", root), "node")
        absolute = str(Path(root).resolve() / "ripr")
        self.assertEqual(MOD.resolve_executable(absolute, root), absolute)

    def test_copy_subject_is_reusable(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            (src / "sub").mkdir(parents=True)
            (src / "a.txt").write_text("a", encoding="utf-8")
            dst = Path(tmp) / "dst"
            MOD.copy_subject(src, dst)
            MOD.copy_subject(src, dst)
            self.assertEqual((dst / "a.txt").read_text(encoding="utf-8"), "a")


if __name__ == "__main__":
    unittest.main()

