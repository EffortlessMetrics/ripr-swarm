"""Reference filtering uses the same executed quality authority as run scoring."""
import copy
import importlib.util
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

from test_scoring import TASK, execution, FAIL, load_lib


def load_oracle():
    path = Path(__file__).resolve().parent / "oracle.py"
    spec = importlib.util.spec_from_file_location("cohort_oracle_under_test", path)
    module = importlib.util.module_from_spec(spec)
    lib = load_lib()
    lib.TASKS = {}
    mutate = types.ModuleType("mutate")
    mutate.gen = lambda *a, **k: []
    runner = types.ModuleType("run_one")
    with mock.patch.dict(sys.modules, {"lib": lib, "mutate": mutate, "run_one": runner}):
        spec.loader.exec_module(module)
    return module


class ReferenceQualificationTests(unittest.TestCase):
    def setUp(self):
        self.oracle = load_oracle()
        lib = load_lib()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        lib.build_repo(TASK, str(root / "agent"))
        with mock.patch.object(lib, "cargo_test", side_effect=[execution(), execution(101, FAIL), execution()]):
            self.result = lib.score(TASK, str(root / "agent"), str(root / "target"))

    def select(self, result):
        return self.oracle.select_qualified_mutants(TASK["mutants"], result)

    def test_complete_native_evidence_selects_the_exact_mutant(self):
        self.assertEqual(self.select(self.result), TASK["mutants"])

    def test_timeout_and_compilation_labels_cannot_qualify_or_disappear(self):
        for status in ("timeout", "compile_error", "process_error", "zero_tests", "passed"):
            r = copy.deepcopy(self.result)
            r["mutant_results"][0]["test"]["status"] = status
            with self.subTest(status=status), self.assertRaisesRegex(ValueError, "incomplete"):
                self.select(r)

    def test_restoration_failures_and_synthetic_labels_are_rejected(self):
        for changes in ({"status": "test_failed"}, {"executed_tests": 0}, {"returncode": 101}):
            r = copy.deepcopy(self.result)
            r["mutant_results"][0]["restored"].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.select(r)

    def test_missing_or_wrong_identity_keeps_existing_mutant_list(self):
        r = copy.deepcopy(self.result)
        r["mutant_results"][0]["mutation_sha256"] = "a" * 64
        with self.assertRaisesRegex(ValueError, "identity"):
            self.select(r)
        r["mutant_results"] = []
        with self.assertRaisesRegex(ValueError, "omitted"):
            self.select(r)

    def test_invalid_mutants_are_not_silently_filtered(self):
        r = copy.deepcopy(self.result)
        r["mutant_results"][0].update(outcome="invalid_mutant", test=None, restored=None)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.select(r)

    def test_actual_compiler_rejection_is_explicit_qualification_exclusion(self):
        from scoring import execution_receipt, canonical_hash
        import json
        r = copy.deepcopy(self.result)
        bad = ("x>=10", "unknown_symbol")
        row = copy.deepcopy(r["mutant_results"][0])
        row.update(id=1, mutation_sha256=canonical_hash(bad), outcome="inconclusive", reason="compile_error")
        compiler_output = json.dumps({"reason": "compiler-message", "message": {"level": "error"}})
        row["test"] = execution_receipt(execution(101, compiler_output, phase="compile"))
        r["mutants"] = 2
        r["mutant_results"].append(row)
        r.update(quality_status="incomplete", unresolved=[1], error="incomplete mutation evidence")
        candidates = [*TASK["mutants"], bad]
        self.assertEqual(self.oracle.select_qualified_mutants(candidates, r), TASK["mutants"])
        row["test"]["compiler_error_count"] = 0
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.oracle.select_qualified_mutants(candidates, r)

    def test_unqualified_baseline_cannot_supply_reference_truth(self):
        for changes in ({"score_protocol": "old"}, {"valid": False}, {"edit_scope_valid": False}):
            r = copy.deepcopy(self.result)
            r.update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.select(r)
