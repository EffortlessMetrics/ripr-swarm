"""No-provider admission controls for source-bound mechanical mutant selections."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest import mock

import lib
from mutate import gen
import run_one
from scoring import _hash_files, _input_files, production_digest
from test_scoring import TASK, PASS, FAIL, execution
import workspace


class TaskQualificationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.task = dict(TASK, mutants=[])
        self.candidates = gen(self.task["new_src"], cap=70)
        self.assertGreater(len(self.candidates), 1)
        workspace.build_repo(self.task, str(self.root / "repo"))
        observations = [execution()]
        for index in range(len(self.candidates)):
            observations.extend([execution(101, FAIL) if index == 0 else execution(0, PASS), execution()])
        # Exercise the real scorer and workspace, with controlled process output;
        # these unit observations are not a claim of native Rust qualification.
        with mock.patch.object(workspace, "cargo_test", side_effect=observations):
            self.proof = workspace.score(dict(self.task, mutants=self.candidates),
                                         str(self.root / "repo"), str(self.root / "target"))
        self.assertEqual(self.proof["quality_status"], "complete")
        self.selected = [self.candidates[0]]
        self.path = self.root / "fixture.json"
        self.proof_path = self.root / "fixture.qualification.json"
        self.path.write_text(json.dumps(self.selected) + "\n", encoding="utf-8")
        self.proof_path.write_text(json.dumps(self.proof) + "\n", encoding="utf-8")
        self.original_bytes = self.path.read_bytes()

    def test_exact_source_qualification_admits_only_its_selected_subset(self):
        self.assertEqual(lib.load_qualified_mutants(self.task, self.path), self.selected)
        self.assertEqual(production_digest(self.task), self.proof["subject"]["production_sha256"])
        self.assertGreater(len(self.candidates), len(self.selected))

    def test_changed_source_rejects_old_line_indices_even_when_still_in_range(self):
        changed = dict(self.task, new_src=self.task["new_src"].replace(">=10", ">=11"))
        self.assertNotEqual(changed["new_src"], self.task["new_src"])
        self.assertTrue(all(0 <= mutation[0] < len(changed["new_src"].split("\n"))
                            for mutation in self.selected))
        with self.assertRaisesRegex(ValueError, "production source/configuration changed"):
            lib.load_qualified_mutants(changed, self.path)
        self.assertEqual(self.path.read_bytes(), self.original_bytes)

    def test_changed_extra_source_or_manifest_is_not_the_qualified_production(self):
        cases = [dict(self.task, extra_files={"src/helper.rs": "pub const VALUE: u8 = 1;\n"}),
                 dict(self.task, cargo_toml='[package]\nname="oracle_fixture"\nversion="0.2.0"\nedition="2021"\n')]
        for task in cases:
            with self.subTest(task=task), self.assertRaisesRegex(ValueError, "source/configuration changed"):
                lib.load_qualified_mutants(task, self.path)
        self.assertEqual(self.path.read_bytes(), self.original_bytes)

    def test_task_context_and_stored_selection_must_match_the_qualification(self):
        changed = dict(self.task, weak_tests=self.task["weak_tests"] + "// different starting tests\n")
        self.assertEqual(production_digest(changed), production_digest(self.task))
        with self.assertRaisesRegex(ValueError, "candidate task changed"):
            lib.load_qualified_mutants(changed, self.path)
        self.path.write_text(json.dumps(self.candidates) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "selection differs"):
            lib.load_qualified_mutants(self.task, self.path)

    def test_missing_or_wrong_subject_cannot_admit_a_historical_list(self):
        for key in ("task_sha256", "production_sha256"):
            proof = copy.deepcopy(self.proof)
            proof["subject"][key] = "a" * 64
            self.proof_path.write_text(json.dumps(proof), encoding="utf-8")
            with self.subTest(key=key), self.assertRaises(ValueError):
                lib.load_qualified_mutants(self.task, self.path)
        proof = copy.deepcopy(self.proof)
        del proof["subject"]["tests_sha256"]
        self.proof_path.write_text(json.dumps(proof), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "source-bound subject"):
            lib.load_qualified_mutants(self.task, self.path)
        self.proof_path.unlink()
        with self.assertRaises(FileNotFoundError):
            lib.load_qualified_mutants(self.task, self.path)
        self.assertEqual(self.path.read_bytes(), self.original_bytes)


class TaskAdmissionTests(unittest.TestCase):
    def test_unqualified_catalog_entries_do_not_disable_authored_tasks(self):
        modules = {}
        entries = (("tasks", "TASKS", {"authored": dict(TASK)}),
                   ("tasks_big", "BIG", {"unqualified_generated_fixture": dict(TASK)}),
                   ("tasks_real", "REAL", {"unqualified_real_fixture": dict(TASK)}))
        for name, attribute, value in entries:
            module = types.ModuleType(name)
            setattr(module, attribute, value)
            modules[name] = module
        modules["tasks_real"].UNAVAILABLE = {"unsupported_real_fixture": "unsupported test item"}
        path = Path(__file__).resolve().parent / "lib.py"
        spec = importlib.util.spec_from_file_location("cohort_catalog_admission_test", path)
        catalog = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, modules):
            spec.loader.exec_module(catalog)
        self.assertEqual(catalog.TASKS["authored"]["mutants"], TASK["mutants"])
        for name in ("unqualified_generated_fixture", "unqualified_real_fixture"):
            self.assertEqual(catalog.TASKS[name]["mutants"], [])
            self.assertIn(name, catalog.UNQUALIFIED_TASKS)
            self.assertNotIn("qualification_error", catalog.TASKS[name])
        self.assertIn("unsupported_real_fixture", catalog.UNQUALIFIED_TASKS)

    def test_unavailable_task_is_refused_before_workspace_or_provider_launch(self):
        with mock.patch.dict(run_one.TASKS, {"unqualified": dict(TASK, mutants=[])}, clear=True), \
             mock.patch.object(run_one, "build_repo") as build, \
             mock.patch.object(run_one, "_run_process") as process:
            for task_id in ("unqualified", "missing"):
                with self.subTest(task_id=task_id), self.assertRaisesRegex(ValueError, "source-qualified"):
                    run_one.run(task_id, "none", "synthetic", 1)
            build.assert_not_called()
            process.assert_not_called()


class ProductionProjectionTests(unittest.TestCase):
    def test_workspace_writer_preserves_input_bytes_under_text_newline_translation(self):
        original_open = open
        def windows_default(*args, **kwargs):
            kwargs.setdefault("newline", "\r\n")
            return original_open(*args, **kwargs)
        with tempfile.TemporaryDirectory() as directory, \
             mock.patch.object(workspace, "open", side_effect=windows_default, create=True):
            path = Path(directory) / "source.rs"
            text = "// 雪\npub fn value() -> u8 { 3 }\n"
            workspace.write(str(path), text)
            self.assertEqual(path.read_bytes(), text.encode("utf-8"))

    def test_pure_projection_matches_files_written_by_real_workspace_builder(self):
        task = dict(TASK, extra_files={
            "./src/helper.rs": "pub const UNICODE: &str = \"雪\";\n",
            "build.rs": "fn main() {}\n",
            ".cargo/config.toml": "[net]\noffline = true\n",
            "fixtures/settings.json": '{"value": 3}\n',
        })
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            workspace.build_repo(task, str(root))
            names = {"Cargo.toml", "src/lib.rs", "src/helper.rs", "build.rs",
                     ".cargo/config.toml", "fixtures/settings.json"}
            actual = {name: (root / name).read_bytes() for name in names}
            self.assertEqual({name: text.encode("utf-8") for name, text in workspace.production_files(task).items()}, actual)
            self.assertEqual(_input_files(root, task), actual)
            self.assertEqual(production_digest(task), _hash_files(actual))
            self.assertIn(b"\n[workspace]\n", actual["Cargo.toml"])
            self.assertNotIn("tests/behavior.rs", actual)
            self.assertEqual(production_digest(dict(task, mutants=[], weak_tests="different suite")),
                             production_digest(task))
