"""Exercise real collection code with only process/task/oracle boundaries stubbed.

No Claude, Cargo, network, or real mutation experiment is invoked by these tests.
"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

from measurements import TOKEN_FIELDS, completed


TASK = dict(id="fixture", mutants=[("before", "after")], new_src="after", base_src="before")


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write(path, text):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def build_repo(task, directory):
    write(str(Path(directory) / "tests" / "behavior.rs"), "// synthetic task\n")


class CollectionTests(unittest.TestCase):
    def setUp(self):
        fake_lib = types.ModuleType("lib")
        fake_lib.TASKS = {"fixture": TASK.copy()}
        fake_lib.GIT = {}
        fake_lib.write = write
        fake_lib.build_repo = build_repo
        fake_lib.sh = mock.Mock(return_value=subprocess.CompletedProcess([], 0, "", ""))
        fake_lib.score = mock.Mock(return_value=dict(valid=True, src_unchanged=True,
                                                   mutants=1, killed=1, survived=[], error=None))
        root = Path(__file__).resolve().parent
        with mock.patch.dict(sys.modules, {"lib": fake_lib}):
            self.runner = load_module("test_run_one", root / "run_one.py")
        with mock.patch.dict(sys.modules, {"run_one": self.runner}):
            self.sweep = load_module("test_sweep", root / "sweep.py")
        self.identity = dict(cohort_id="synthetic", max_turns=7, task="fixture", arm="none",
                             model="model", rep=1, task_sha256="a" * 64,
                             protocol_sha256="b" * 64, mutants=1)
        self.event = dict(type="result", subtype="success", is_error=False, num_turns=3,
                          total_cost_usd=0.25, usage=dict(zip(TOKEN_FIELDS, (10, 20, 30, 40))))

    def collect(self, directory, stdout=None, side_effect=None):
        proc = subprocess.CompletedProcess([], 0, stdout or json.dumps(self.event), "")
        with mock.patch.object(self.runner, "run_identity", return_value=self.identity), \
             mock.patch.object(self.runner.subprocess, "run", return_value=proc, side_effect=side_effect):
            return self.runner.run("fixture", "none", "model", 1, work=directory,
                                   max_turns=7, cohort_id="synthetic")

    def test_real_record_keeps_terminal_usage_once_not_assistant_echoes(self):
        assistant = dict(type="assistant", message=dict(content=[], usage=self.event["usage"]))
        transcript = "\n".join(json.dumps(x) for x in (assistant, assistant, self.event))
        with tempfile.TemporaryDirectory() as directory:
            row = self.collect(directory, transcript)
            self.assertEqual(row["tokens"], 100)
            self.assertEqual(row["cost_usd"], 0.25)
            self.assertTrue(completed(row))
            stored = json.loads(next(Path(directory).glob("*/record.json")).read_text())
            self.assertEqual(stored, row)

    def test_timeout_retains_partial_trace_without_inventing_complete_cost(self):
        timeout = subprocess.TimeoutExpired("claude", 1, output=b'{"type":"assistant","message":{"content":[]}}')
        with tempfile.TemporaryDirectory() as directory:
            row = self.collect(directory, side_effect=timeout)
            self.assertEqual(row["rc"], -9)
            self.assertFalse(completed(row))
            self.assertIsNone(row["tokens"])
            self.assertIsNone(row["tool_calls"])
            self.assertEqual(row["observed_tool_calls"], 0)
            self.assertIsNone(row["cost_usd"])
            self.assertTrue(next(Path(directory).glob("*/transcript.jsonl")).read_text())

    def test_scorer_exception_keeps_agent_cost_and_identity(self):
        self.runner.score.side_effect = RuntimeError("scorer broke")
        with tempfile.TemporaryDirectory() as directory:
            row = self.collect(directory)
            self.assertEqual(row["tokens"], 100)
            self.assertEqual(row["cost_usd"], 0.25)
            self.assertFalse(row["valid"])
            self.assertIn("scorer broke", row["score_error"])
            self.assertEqual(row["cohort_id"], "synthetic")

    def test_post_run_metadata_error_keeps_cost(self):
        self.runner.sh.side_effect = RuntimeError("git failed")
        with tempfile.TemporaryDirectory() as directory:
            row = self.collect(directory)
            self.assertEqual(row["tokens"], 100)
            self.assertIn("git failed", row["metadata_error"])

    def test_repeated_job_does_not_overwrite_prior_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            self.collect(directory)
            self.collect(directory)
            self.assertEqual(len(list(Path(directory).glob("*/record.json"))), 2)
            self.assertEqual(len(list(Path(directory).glob("*/ripr-off/ripr"))), 2)

    def test_actual_identity_changes_with_task_or_oracle_not_arm(self):
        with mock.patch.object(self.runner.Path, "read_text", return_value="synthetic source"):
            a = self.runner.run_identity("fixture", "none", "model", 1, 7, "cohort")
            b = self.runner.run_identity("fixture", "check-mandatory", "model", 1, 7, "cohort")
            self.assertEqual(a["task_sha256"], b["task_sha256"])
            self.assertEqual(a["protocol_sha256"], b["protocol_sha256"])
            self.runner.TASKS["fixture"]["mutants"] = [("other", "after")]
            c = self.runner.run_identity("fixture", "none", "model", 1, 7, "cohort")
            self.assertNotEqual(a["task_sha256"], c["task_sha256"])
            d = self.runner.run_identity("fixture", "none", "model", 1, 7, "cohort", timeout=1)
            self.assertNotEqual(a["protocol_sha256"], d["protocol_sha256"])

    def test_sweep_exception_preserves_budget_and_missing_cost(self):
        with mock.patch.object(self.sweep, "run_identity", return_value=self.identity), \
             mock.patch.object(self.sweep, "run", side_effect=RuntimeError("worker failed")):
            row = self.sweep.run_job(("fixture", "none", "model", 1), 7, "synthetic")
        self.assertEqual(row["max_turns"], 7)
        self.assertEqual(row["cohort_id"], "synthetic")
        self.assertEqual(row["task_sha256"], "a" * 64)
        self.assertIsNone(row["tokens"])
        self.assertIn("worker failed", row["error"])

    def test_identity_failure_still_retains_attempt(self):
        with mock.patch.object(self.sweep, "run_identity", side_effect=OSError("missing source")):
            row = self.sweep.run_job(("fixture", "none", "model", 1), 7, "synthetic")
        self.assertEqual(row["max_turns"], 7)
        self.assertIn("missing source", row["error"])

    def test_sweep_writes_every_success_and_exception_with_one_cohort(self):
        def fake_run(*job, **kwargs):
            if job[1] != "none":
                raise RuntimeError("synthetic failure")
            return dict(task=job[0], arm=job[1], model=job[2], rep=job[3], **kwargs)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "out.jsonl"
            with mock.patch.object(self.sweep, "run", side_effect=fake_run), \
                 mock.patch.object(self.sweep, "run_identity", return_value={}), \
                 mock.patch.dict(self.sweep.os.environ, {"COHORT_ID": "test", "COHORT_MAX_TURNS": "7"}), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.sweep.main(["fixture", "none,check-mandatory", "model", "1", "3", str(path), "2"])
            rows = [json.loads(line) for line in path.read_text().splitlines()]
            self.assertEqual(len(rows), 4)
            self.assertEqual(sum("error" in r for r in rows), 2)
            self.assertTrue(all(r["cohort_id"] == "test" and r["max_turns"] == 7 for r in rows))


if __name__ == "__main__":
    unittest.main()
