"""The public reference validator must propagate missing or failed proof."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import types
import unittest
from unittest import mock


class ValidationTests(unittest.TestCase):
    def setUp(self):
        lib = types.ModuleType("lib")
        lib.TASKS = {"fixture": dict(id="fixture", ref_tests="authored")}
        lib.build_repo = mock.Mock()
        lib.write = mock.Mock()
        lib.score = mock.Mock(return_value=dict(quality_status="complete", mutants=2, killed=2))
        path = Path(__file__).resolve().parent / "validate.py"
        spec = importlib.util.spec_from_file_location("cohort_validate_under_test", path)
        self.module = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, {"lib": lib}):
            spec.loader.exec_module(self.module)

    def run_validator(self):
        with contextlib.redirect_stdout(io.StringIO()):
            return self.module.main(["fixture"])

    def test_complete_reference_returns_success(self):
        self.assertEqual(self.run_validator(), 0)
        self.assertEqual(self.module.score.call_count, 2)

    def test_incomplete_pristine_or_reference_is_not_a_pass(self):
        good = dict(quality_status="complete", mutants=2, killed=2)
        for results in ([dict(good, quality_status="incomplete"), good],
                        [good, dict(good, quality_status="incomplete")],
                        [good, dict(good, killed=1)],
                        [good, dict(good, mutants=0, killed=0)]):
            with self.subTest(results=results):
                self.module.score.side_effect = results
                self.assertEqual(self.run_validator(), 1)

    def test_missing_reference_is_explicit_not_applicable_not_success(self):
        self.module.TASKS["fixture"]["ref_tests"] = ""
        self.assertEqual(self.run_validator(), 1)
        self.module.score.assert_not_called()

    def test_empty_population_and_unknown_tasks_cannot_validate(self):
        self.module.TASKS = {}
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.module.main([]), 2)
            self.assertEqual(self.module.main(["absent"]), 2)
