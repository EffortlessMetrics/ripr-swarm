"""Tiny dependency-free Cargo controls. No agent, network, or paid evaluation.

Local discovery visibly skips these when Cargo is absent. The Rust-owned selftest
requires Cargo and rejects any skip, so native CI cannot silently omit this proof.
"""
from pathlib import Path
import shutil
import tempfile
import unittest

from test_scoring import TASK, load_lib


@unittest.skipUnless(shutil.which("cargo"), "native Cargo unavailable")
class NativeScoringTests(unittest.TestCase):
    def setUp(self):
        self.lib = load_lib()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = str(Path(self.tmp.name) / "agent")
        self.target = str(Path(self.tmp.name) / "target")
        self.lib.build_repo(TASK, self.repo)

    def test_exact_boundary_test_catches_mutant_and_passes_restoration(self):
        r = self.lib.score(TASK, self.repo, self.target)
        self.assertEqual(r["quality_status"], "complete", r)
        self.assertEqual(r["killed"], 1, r)
        self.assertEqual(r["mutant_results"][0]["test"]["status"], "test_failed", r)
        self.assertEqual(r["mutant_results"][0]["restored"]["status"], "passed", r)

    def test_smoke_test_survives_the_same_mutant(self):
        (Path(self.repo) / "tests/behavior.rs").write_text(TASK["weak_tests"].replace("limit(10)", "limit(11)"))
        r = self.lib.score(TASK, self.repo, self.target)
        self.assertEqual(r["quality_status"], "complete", r)
        self.assertEqual(r["killed"], 0, r)
        self.assertEqual(r["survived"], [0], r)

    def test_zero_tests_is_not_pristine_validation(self):
        (Path(self.repo) / "tests/behavior.rs").write_text("// no tests\n")
        r = self.lib.score(TASK, self.repo, self.target)
        self.assertFalse(r["valid"], r)
        self.assertEqual(r["baseline"]["status"], "zero_tests", r)

    def test_noncompiling_mutant_is_unresolved_not_caught(self):
        task = dict(TASK, mutants=[("x>=10", "missing_name(x)")])
        r = self.lib.score(task, self.repo, self.target)
        self.assertEqual(r["killed"], 0, r)
        self.assertEqual(r["unresolved"], [0], r)
        self.assertEqual(r["mutant_results"][0]["test"]["status"], "compile_error", r)
        self.assertEqual(r["mutant_results"][0]["restored"]["status"], "passed", r)

    def test_test_process_exit_is_not_a_caught_assertion(self):
        task = dict(TASK, mutants=[("x>=10", "{ std::process::exit(7); true }")])
        r = self.lib.score(task, self.repo, self.target)
        self.assertEqual(r["killed"], 0, r)
        self.assertEqual(r["unresolved"], [0], r)
        self.assertEqual(r["mutant_results"][0]["test"]["status"], "process_error", r)
