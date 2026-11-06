"""Completed reference reuse cannot bypass producer or fixed-test admission."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from test_reference_qualification import load_oracle
from test_scoring import TASK, execution, FAIL, load_lib


class ReferenceProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.oracle = load_oracle()
        self.oracle.ORACLE_ROOT = self.root / "oracle"
        self.oracle.TASKS["fixture"] = copy.deepcopy(TASK)
        self.oracle.run_one.ALLOWED = "synthetic"
        self.lib = load_lib()
        self.oracle.build_repo = self.lib.build_repo
        # Actual selection writes stay inside this owned test fixture.
        self.oracle.__file__ = str(self.root / "layout" / "harness" / "oracle.py")
        self.mutants = self.root / "layout" / "mutants"
        self.mutants.mkdir(parents=True)
        self.selected = self.mutants / "fixture.json"
        self.selected.write_text("previous selected bytes\n")
        self.refilter = self.load_refilter()

    def load_refilter(self, arguments=()):
        path = Path(__file__).resolve().parent / "refilter.py"
        spec = importlib.util.spec_from_file_location("cohort_refilter_under_test", path)
        module = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, {"oracle": self.oracle}), \
             mock.patch.object(sys, "argv", ["importer", *arguments]):
            spec.loader.exec_module(module)
        return module

    def success(self, k, model="synthetic"):
        result = subprocess.CompletedProcess(
            [], 0, b'{"type":"result","subtype":"success","is_error":false}', b"")
        with mock.patch.object(self.oracle, "_run_process", return_value=result):
            return self.oracle.agent("fixture", k, model)

    def directory(self, k):
        return self.oracle.ORACLE_ROOT / "fixture" / f"agent{k}"

    def attempt(self, reference):
        row = reference.receipt
        return self.directory(row["agent"]) / "attempts" / row["run_id"]

    def native_boundary(self, task, repo, target):
        with mock.patch.object(self.lib, "cargo_test",
                               side_effect=[execution(), execution(101, FAIL), execution()]):
            return self.lib.score(task, repo, target)

    def test_import_does_not_filter_unrelated_command_arguments(self):
        with mock.patch.object(self.oracle, "filter_mutants") as filtering:
            self.load_refilter(["fixture"])
        filtering.assert_not_called()

    def test_unverified_legacy_directories_cannot_reach_filtering(self):
        for k in (0, 1):
            self.lib.build_repo(TASK, str(self.directory(k) / "repo"))
        with mock.patch.object(self.oracle, "filter_mutants") as filtering, \
             self.assertRaisesRegex(ValueError, "completion evidence"):
            self.refilter.main(["fixture"])
        filtering.assert_not_called()
        self.assertEqual(self.selected.read_text(), "previous selected bytes\n")

    def test_failed_replacement_model_invalidates_old_success(self):
        old = self.success(0, "old-model")
        self.success(1, "other-completed-model")
        failed = subprocess.CompletedProcess(
            [], 7, b'{"type":"result","subtype":"error_max_turns","is_error":true}', b"failed")
        with mock.patch.object(self.oracle, "_run_process", return_value=failed), \
             self.assertRaisesRegex(RuntimeError, "did not complete"):
            self.oracle.agent("fixture", 0, "replacement-model")
        self.assertFalse((self.directory(0) / "completed.json").exists())
        latest = json.loads((self.directory(0) / "latest.json").read_text())["run_id"]
        attempt = self.directory(0) / "attempts" / latest
        result = json.loads((attempt / "result.json").read_text())
        self.assertEqual((result["status"], result["model"], result["process"]["returncode"]),
                         ("failed", "replacement-model", 7))
        self.assertEqual((attempt / "stdout").read_bytes(), failed.stdout)
        self.assertEqual((attempt / "stderr").read_bytes(), failed.stderr)
        self.assertEqual(json.loads((self.attempt(old) / "result.json").read_text())["status"],
                         "completed")
        with mock.patch.object(self.oracle, "filter_mutants") as filtering, \
             self.assertRaisesRegex(ValueError, "completion evidence"):
            self.refilter.main(["fixture"])
        filtering.assert_not_called()
        self.assertEqual(self.selected.read_text(), "previous selected bytes\n")

    def test_build_failure_invalidates_before_any_provider_call(self):
        self.success(0)
        with mock.patch.object(self.oracle, "build_repo", side_effect=OSError("fixture build failed")), \
             mock.patch.object(self.oracle, "_run_process") as process, \
             self.assertRaisesRegex(OSError, "fixture build failed"):
            self.oracle.agent("fixture", 0, "replacement-model")
        process.assert_not_called()
        self.assertFalse((self.directory(0) / "completed.json").exists())

    def test_changed_task_source_or_configuration_refuses_reuse(self):
        self.success(0)
        original = self.oracle.TASKS["fixture"]
        for changes in (
            {"new_src": "pub fn limit(x:u32)->bool{x>=11}\n"},
            {"extra_files": {".cargo/config.toml": "[build]\nrustflags=[]\n"}},
        ):
            with self.subTest(changes=changes), \
                 mock.patch.dict(self.oracle.TASKS, {"fixture": dict(original, **changes)}), \
                 self.assertRaisesRegex(ValueError, "source/configuration changed"):
                self.oracle.completed_agent_tests("fixture", (0,))

    def test_successful_terminal_cannot_attest_modified_frozen_source(self):
        def changed_source(command, repo, env, timeout):
            Path(repo, "src/lib.rs").write_text("pub fn limit(_:u32)->bool{true}\n")
            return subprocess.CompletedProcess(
                command, 0, b'{"type":"result","subtype":"success","is_error":false}', b"")
        with mock.patch.object(self.oracle, "_run_process", side_effect=changed_source), \
             self.assertRaisesRegex(RuntimeError, "changed frozen source"):
            self.oracle.agent("fixture", 0, "synthetic")
        self.assertFalse((self.directory(0) / "completed.json").exists())

    def test_selected_mutant_output_does_not_redefine_reference_inputs(self):
        expected = self.success(0)
        self.oracle.TASKS["fixture"]["mutants"] = [("x>=10", "false")]
        observed = self.oracle.completed_agent_tests("fixture", (0,))[0]
        self.assertEqual(observed, expected)

    def test_changed_or_nonregular_test_files_refuse_reuse(self):
        reference = self.success(0)
        tests = self.attempt(reference) / "repo" / "tests"
        behavior = tests / "behavior.rs"
        original = behavior.read_bytes()
        behavior.write_bytes(original + b"\n// later edit\n")
        with self.assertRaisesRegex(ValueError, "snapshot changed"):
            self.oracle.completed_agent_tests("fixture", (0,))
        behavior.write_bytes(original)
        outside = self.root / "outside.rs"
        outside.write_text("// outside")
        (tests / "linked.rs").symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "not a regular"):
            self.oracle.completed_agent_tests("fixture", (0,))

    def test_existence_or_unsuccessful_terminal_data_is_not_completion(self):
        reference = self.success(0)
        marker = self.directory(0) / "completed.json"
        retained = self.attempt(reference) / "result.json"
        good = reference.receipt
        for data in ("{", "{}"):
            marker.write_text(data)
            with self.subTest(data=data), self.assertRaises(ValueError):
                self.oracle.completed_agent_tests("fixture", (0,))
        for mutate in (
            lambda row: row["terminal"].update(is_error=True),
            lambda row: row["terminal"].update(type="assistant"),
            lambda row: row["process"].update(timed_out=True),
            *(lambda row, value=value: row["process"].update(cleanup_error=value)
              for value in (False, 0, "", [], {})),
            lambda row: row.update(agent=1),
            lambda row: row.update(schema_version=True),
        ):
            bad = copy.deepcopy(good)
            mutate(bad)
            marker.write_text(json.dumps(bad))
            retained.write_text(json.dumps(bad))
            with self.subTest(record=bad), self.assertRaises(ValueError):
                self.oracle.completed_agent_tests("fixture", (0,))

    def test_superseded_success_cannot_be_reintroduced_as_latest(self):
        old = self.success(0, "first-model")
        self.success(0, "second-model")
        (self.directory(0) / "completed.json").write_text(old.receipt_json)
        with self.assertRaisesRegex(ValueError, "superseded"):
            self.oracle.completed_agent_tests("fixture", (0,))

    def test_completed_refilter_uses_frozen_bytes_and_retains_provenance(self):
        self.success(0, "first-model")
        self.success(1, "second-model")
        references = []
        admit = self.oracle.completed_agent_tests
        def admit_then_change_directory(task_id, ks):
            references.extend(admit(task_id, ks))
            # Change the live files between the real admission and consumer.
            (self.attempt(references[0]) / "repo/tests/behavior.rs").write_text("changed after admission")
            return references
        with mock.patch.object(self.oracle, "gen", return_value=TASK["mutants"]), \
             mock.patch.object(self.oracle, "score", side_effect=self.native_boundary), \
             mock.patch.object(self.oracle, "completed_agent_tests", side_effect=admit_then_change_directory):
            self.refilter.main(["fixture"])
        original = dict(references[0].files)["tests/behavior.rs"]
        copied = self.oracle.ORACLE_ROOT / "fixture/filter/tests/oracle0_behavior.rs"
        self.assertEqual(copied.read_bytes(), original)
        qualification = json.loads((self.mutants / "fixture.qualification.json").read_text())
        self.assertEqual(qualification["reference_agents"], [r.receipt for r in references])
        self.assertEqual(json.loads(self.selected.read_text()), [list(m) for m in TASK["mutants"]])
        with self.assertRaisesRegex(ValueError, "snapshot changed"):
            self.oracle.completed_agent_tests("fixture", (0,))

    def test_grading_subject_drift_preserves_prior_selection(self):
        references = [self.success(0)]
        def changed_subject(task, repo, target):
            with Path(repo, "tests/oracle0_behavior.rs").open("a") as stream:
                stream.write("\n// changed at grading boundary\n")
            return self.native_boundary(task, repo, target)
        with mock.patch.object(self.oracle, "gen", return_value=TASK["mutants"]), \
             mock.patch.object(self.oracle, "score", side_effect=changed_subject), \
             self.assertRaisesRegex(ValueError, "pristine pass"):
            self.oracle.filter_mutants("fixture", references)
        self.assertEqual(self.selected.read_text(), "previous selected bytes\n")
        qualification = json.loads((self.mutants / "fixture.qualification.json").read_text())
        self.assertFalse(qualification["valid"])
        self.assertIn("admitted snapshots", qualification["error"])
        self.assertEqual(qualification["reference_agents"], [r.receipt for r in references])

    def test_raw_paths_empty_or_duplicate_references_never_reach_build(self):
        reference = self.success(0)
        for references in ([], [str(self.attempt(reference) / "repo/tests")],
                           [reference, reference]):
            with self.subTest(references=references), \
                 mock.patch.object(self.oracle, "build_repo") as build, \
                 self.assertRaises(ValueError):
                self.oracle.filter_mutants("fixture", references)
            build.assert_not_called()

    def test_consistent_malformed_snapshots_refuse_before_any_filesystem_work(self):
        reference = self.success(0)
        cases = [
            (((name, b"payload"),), "normalized relative tests paths")
            for name in ("tests/sub/../../escaped.rs", "/tests/absolute.rs", "src/lib.rs",
                         "tests//repeated.rs", "tests/./dot.rs", "tests/trailing/",
                         "tests", "tests\\windows.rs", "tests/drive:stream.rs", "tests/nul\0.rs", 7)
        ]
        cases.extend([
            ((("tests/a.rs", "text"),), "payload must be bytes"),
            ((("tests/a.rs", bytearray(b"mutable")),), "payload must be bytes"),
            ((("tests/a.rs", b"one"), ("tests/a.rs", b"two")), "repeats a test path"),
            ([('tests/a.rs', b"mutable container")], "immutable file pairs"),
            ((["tests/a.rs", b"mutable pair"],), "immutable file pairs"),
            ((("tests/a.rs",),), "immutable file pairs"),
            (tuple((f"tests/{i}.rs", b"x") for i in range(self.oracle.MAX_TEST_FILES + 1)),
             "shared test transfer bounds"),
            ((("tests/large.rs", b"x" * (self.oracle.MAX_TEST_BYTES + 1)),),
             "shared test transfer bounds"),
        ])
        for snapshot, error in cases:
            receipt = reference.receipt
            try:
                # A matching digest must not launder malformed paths or payloads.
                receipt["tests_sha256"] = self.oracle._hash_files(dict(snapshot))
            except (TypeError, ValueError):
                pass
            forged = self.oracle.CompletedReference(json.dumps(receipt), snapshot)
            with self.subTest(error=error, first=snapshot[0][0]), \
                 mock.patch.object(self.oracle, "build_repo") as build, \
                 mock.patch.object(self.oracle, "score") as score, \
                 self.assertRaisesRegex(ValueError, error):
                self.oracle.filter_mutants("fixture", [forged])
            build.assert_not_called()
            score.assert_not_called()
            self.assertFalse((self.oracle.ORACLE_ROOT / "fixture/filter").exists())
        self.assertEqual(self.selected.read_text(), "previous selected bytes\n")

    def test_combined_snapshot_bounds_refuse_before_build(self):
        references = []
        for k in (0, 1):
            reference = self.success(k)
            snapshot = tuple((f"tests/{i}.rs", b"x")
                             for i in range(self.oracle.MAX_TEST_FILES // 2 + 1))
            receipt = reference.receipt
            receipt["tests_sha256"] = self.oracle._hash_files(dict(snapshot))
            references.append(self.oracle.CompletedReference(json.dumps(receipt), snapshot))
        with mock.patch.object(self.oracle, "build_repo") as build, \
             mock.patch.object(self.oracle, "score") as score, \
             self.assertRaisesRegex(ValueError, "shared test transfer bounds"):
            self.oracle.filter_mutants("fixture", references)
        build.assert_not_called()
        score.assert_not_called()
