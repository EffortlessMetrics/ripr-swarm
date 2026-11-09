"""Regression controls for the cohort's real scoring entrypoint; no model calls."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock


TASK = dict(id="oracle_fixture", msg="feat: threshold", base_src="pub fn limit(x:u32)->bool{x>10}\n",
            new_src="pub fn limit(x:u32)->bool{x>=10}\n", weak_tests="#[test]\nfn observed(){assert!(oracle_fixture::limit(10));}\n",
            mutants=[("x>=10", "x>10")])
PASS = "running 1 test\ntest observed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
FAIL = "running 1 test\ntest observed ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
ZERO = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"


def execution(code=0, stdout=PASS, stderr="", phase="test", timed_out=False):
    result = subprocess.CompletedProcess(["cargo", "test"], code, stdout, stderr)
    result.cohort_phase = phase
    result.cohort_timed_out = timed_out
    return result


def load_lib():
    path = Path(__file__).resolve().parent / "workspace.py"
    spec = importlib.util.spec_from_file_location("cohort_lib_under_test", path)
    module = importlib.util.module_from_spec(spec)
    task_modules = {}
    for name, key, value in (("tasks", "TASKS", {}), ("tasks_big", "BIG", {}), ("tasks_real", "REAL", {})):
        stub = types.ModuleType(name)
        setattr(stub, key, value)
        # Old optional-import logic uses `BIG or REAL`.
        if name == "tasks_big": stub.REAL = {}
        task_modules[name] = stub
    with mock.patch.dict(sys.modules, task_modules):
        spec.loader.exec_module(module)
    return module


class ScoringRegressionTests(unittest.TestCase):
    def setUp(self):
        self.lib = load_lib()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = str(Path(self.tmp.name) / "agent")
        self.target = str(Path(self.tmp.name) / "target")
        self.lib.build_repo(TASK, self.repo)

    def score(self, results):
        with mock.patch.object(self.lib, "cargo_test", side_effect=results):
            return self.lib.score(TASK, self.repo, self.target)

    def test_timeout_is_not_a_caught_mutant(self):
        result = self.score([execution(), execution(124, "", "timeout", timed_out=True), execution()])
        self.assertEqual(result["killed"], 0)

    def test_test_compilation_error_is_not_a_caught_mutant(self):
        result = self.score([execution(), execution(101, "", "error[E0308]: tests/behavior.rs\ncould not compile crate", phase="compile"), execution()])
        self.assertEqual(result["killed"], 0)

    def test_process_failure_is_not_a_caught_mutant(self):
        result = self.score([execution(), execution(127, "", "missing executable"), execution()])
        self.assertEqual(result["killed"], 0)

    def test_zero_test_pristine_run_is_not_valid(self):
        result = self.score([execution(0, ZERO), execution(0, ZERO)])
        self.assertFalse(result["valid"])

    def test_committing_a_production_edit_cannot_redefine_pristine(self):
        source = Path(self.repo) / "src/lib.rs"
        source.write_text(TASK["new_src"] + "// agent edit\n", encoding="utf-8")
        self.lib.sh(["git", "add", "src/lib.rs"], self.repo, self.lib.GIT, check=True)
        self.lib.sh(["git", "commit", "-qm", "agent change"], self.repo, self.lib.GIT, check=True)
        result = self.score([execution(), execution(101, FAIL), execution()])
        self.assertFalse(result["src_unchanged"])

    def test_manifest_edits_cannot_change_the_grading_environment(self):
        manifest = Path(self.repo) / "Cargo.toml"
        manifest.write_text(manifest.read_text() + "\n[profile.test]\nopt-level=3\n", encoding="utf-8")
        result = self.score([execution(), execution(101, FAIL), execution()])
        self.assertFalse(result["valid"])

    def test_failed_pristine_restoration_revokes_catch(self):
        result = self.score([execution(), execution(101, FAIL), execution(101, FAIL)])
        self.assertEqual(result["killed"], 0)


class ExecutionReceiptTests(unittest.TestCase):
    def receipt(self, *args, **kwargs):
        from scoring import execution_receipt
        return execution_receipt(execution(*args, **kwargs))

    def test_stock_libtest_positive_and_negative_results(self):
        self.assertEqual(self.receipt()["status"], "passed")
        self.assertEqual(self.receipt(101, FAIL)["status"], "test_failed")
        self.assertEqual(self.receipt(0, ZERO)["status"], "zero_tests")

    def test_no_partial_or_contradictory_summary_can_supply_success(self):
        cases = [(0, ""), (101, PASS), (0, FAIL), (0, PASS + "running 2 tests\n"),
                 (0, PASS.replace("running 1 test", "running 2 tests")),
                 (0, PASS.replace("1 passed", "0 passed")),
                 (101, "running 1 test\ntest observed ... ")]
        for code, out in cases:
            with self.subTest(code=code, out=out):
                self.assertEqual(self.receipt(code, out)["status"], "incomplete")

    def test_ignored_only_tests_are_not_execution(self):
        out = PASS.replace("test observed ... ok", "test observed ... ignored").replace("1 passed; 0 failed; 0 ignored", "0 passed; 0 failed; 1 ignored")
        self.assertEqual(self.receipt(0, out)["status"], "zero_tests")

    def test_complete_multiple_suites_are_counted(self):
        r = self.receipt(0, ZERO + PASS + PASS)
        self.assertEqual(r["status"], "passed")
        self.assertEqual(r["executed_tests"], 2)
        self.assertEqual(r["suites"], 3)

    def test_timeout_signal_and_launch_failure_do_not_count_as_test_failures(self):
        for result in (execution(124, FAIL, timed_out=True), execution(-9, FAIL), execution(127, FAIL)):
            from scoring import execution_receipt
            self.assertNotEqual(execution_receipt(result)["status"], "test_failed")

    def test_compiler_json_error_is_distinct_from_build_infrastructure_error(self):
        import json
        compiler = json.dumps(dict(reason="compiler-message", message=dict(level="error")))
        self.assertEqual(self.receipt(101, compiler, phase="compile")["status"], "compile_error")
        self.assertEqual(self.receipt(101, "", "offline dependency absent", phase="compile")["status"], "build_error")

    def test_human_output_is_not_interpreted_as_a_compiler_receipt(self):
        self.assertEqual(self.receipt(101, FAIL, "could not compile", phase="test")["status"], "test_failed")

    def test_compile_is_followed_by_execution_only_after_success(self):
        import scoring
        with mock.patch.object(scoring, "_run_process", side_effect=[execution(101, "", phase="compile")]) as run:
            r = scoring.run_cargo_tests("repo", "target")
            self.assertEqual(run.call_count, 1)
            self.assertEqual(r.cohort_phase, "compile")
            self.assertIn("--no-run", run.call_args.args[0])
        with mock.patch.object(scoring, "_run_process", side_effect=[execution(0, ""), execution()]) as run:
            r = scoring.run_cargo_tests("repo", "target")
            self.assertEqual(run.call_count, 2)
            self.assertEqual(r.cohort_phase, "test")
            self.assertIn("--offline", run.call_args.args[0])
            self.assertIn("--test-threads=1", run.call_args.args[0])


class ScoringPositiveTests(unittest.TestCase):
    def setUp(self):
        ScoringRegressionTests.setUp(self)

    def grade(self, sequence, task=TASK, archive=False):
        self.archive = Path(self.tmp.name) / "retained-tests"
        with mock.patch.object(self.lib, "cargo_test", side_effect=sequence) as run:
            r = self.lib.score(task, self.repo, self.target, archive_tests=str(self.archive) if archive else None)
            self.calls = run.call_args_list
            return r

    def test_real_entry_keeps_verified_pass_fail_pass_and_pristine_agent_tree(self):
        from scoring import tests_digest
        r = self.grade([execution(), execution(101, FAIL), execution()], archive=True)
        self.assertEqual(r["quality_status"], "complete")
        self.assertEqual(r["killed"], 1)
        self.assertEqual(r["mutant_results"][0]["outcome"], "caught")
        self.assertEqual(r["subject"]["tests_sha256"], tests_digest(self.archive))
        self.assertEqual((Path(self.repo) / "src/lib.rs").read_text(), TASK["new_src"])
        self.assertTrue(all(call.args[0] != self.repo for call in self.calls))

    def test_surviving_mutant_is_a_complete_quality_observation_not_success(self):
        r = self.grade([execution(), execution(), execution()])
        self.assertEqual(r["quality_status"], "complete")
        self.assertEqual(r["killed"], 0)
        self.assertEqual(r["survived"], [0])
        self.assertEqual(r["unresolved"], [])

    def test_invalid_and_duplicate_mutants_keep_the_denominator(self):
        task = dict(TASK, mutants=[TASK["mutants"][0], TASK["mutants"][0], ("absent", "new")])
        r = self.grade([execution(), execution(101, FAIL), execution()], task)
        self.assertEqual(r["mutants"], 3)
        self.assertEqual(r["killed"], 1)
        self.assertEqual(r["unresolved"], [1, 2])
        self.assertEqual(len(r["mutant_results"]), 3)
        self.assertEqual(r["quality_status"], "incomplete")

    def test_missing_test_tree_does_not_retain_weak_fixture_tests(self):
        import shutil
        shutil.rmtree(Path(self.repo) / "tests")
        r = self.grade([])
        self.assertFalse(r["valid"])
        self.assertEqual(self.calls, [])

    def test_symlink_test_is_not_followed(self):
        test = Path(self.repo) / "tests/behavior.rs"
        test.unlink()
        test.symlink_to(Path(self.repo) / "src/lib.rs")
        r = self.grade([])
        self.assertFalse(r["valid"])
        self.assertIn("not a regular", r["error"])
        self.assertEqual(self.calls, [])

    def test_added_production_file_is_a_scope_violation(self):
        (Path(self.repo) / "src/extra.rs").write_text("// not allowed\n")
        r = self.grade([])
        self.assertIn("src/extra.rs", r["scope_violations"])
        self.assertFalse(r["src_unchanged"])

    def test_new_cargo_configuration_is_a_scope_violation(self):
        config = Path(self.repo) / ".cargo/config.toml"
        config.parent.mkdir()
        config.write_text('[build]\nrustflags=["--cfg", "hide_tests"]\n')
        r = self.grade([])
        self.assertIn(".cargo/config.toml", r["scope_violations"])
        self.assertFalse(r["valid"])

    def test_mutation_cannot_disappear_tests_and_look_surviving(self):
        r = self.grade([execution(), execution(0, ZERO), execution()])
        self.assertEqual(r["killed"], 0)
        self.assertEqual(r["survived"], [])
        self.assertEqual(r["unresolved"], [0])

    def test_restoration_failure_marks_remaining_mutants_not_run(self):
        task = dict(TASK, mutants=[TASK["mutants"][0], ("x>=10", "x>=11")])
        r = self.grade([execution(), execution(101, FAIL), execution(101, FAIL)], task)
        self.assertEqual(r["unresolved"], [0, 1])
        self.assertEqual(r["mutant_results"][1]["outcome"], "not_run")
        self.assertEqual(len(self.calls), 3)

    def test_execution_cannot_change_the_inputs_it_claims_to_verify(self):
        def changing(directory, _target):
            (Path(directory) / "tests/behavior.rs").write_text("// wiped by test\n")
            return execution()
        r = self.grade(changing)
        self.assertFalse(r["valid"])
        self.assertEqual(r["baseline"]["status"], "input_changed")

    def test_transfer_is_bounded_and_does_not_run_oversized_tests(self):
        import scoring
        with mock.patch.object(scoring, "MAX_TEST_BYTES", 1):
            r = self.grade([])
        self.assertFalse(r["valid"])
        self.assertIn("transfer exceeds", r["error"])
        self.assertEqual(self.calls, [])

    def test_unsafe_task_path_is_rejected_before_builder_writes(self):
        task = dict(TASK, extra_files={"../escape": "no"})
        with mock.patch.object(self.lib, "build_repo") as builder:
            r = self.grade([], task)
        builder.assert_not_called()
        self.assertIn("unsafe task path", r["error"])

    def test_byte_identical_grade_is_repeatable_apart_from_process_timing(self):
        a = self.grade([execution(), execution(101, FAIL), execution()])
        b = self.grade([execution(), execution(101, FAIL), execution()])
        self.assertEqual(a, b)


if __name__ == "__main__":
    unittest.main()
