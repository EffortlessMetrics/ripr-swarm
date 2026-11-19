"""Run the real read-only observer against complete native API fixtures."""

import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("main_qualification", ROOT / "tools/ci/main-qualification.py")
OBSERVER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OBSERVER)
SHA = "a" * 40
OTHER = "b" * 40
REPO = "EffortlessMetrics/ripr-swarm"
RUN = 123
ATTEMPT = 1
RESULT = "Ripr Rust Small Result"
PREFIX = "Ripr Rust Small tests on GitHub Hosted"
RUNS = f"actions/runs?event=merge_group&head_sha={SHA}&per_page=100&page=1"
JOBS = f"actions/runs/{RUN}/attempts/{ATTEMPT}/jobs?per_page=100&page=1"
CHECKS = f"commits/{SHA}/check-runs?check_name=Ripr%20Rust%20Small%20Result&filter=all&per_page=100&page=1"
RUN_PATH = f"actions/runs/{RUN}"
MAIN = "git/ref/heads/main"


def fixture(docs=False, extension=False):
    branch = f"gh-readonly-queue/main/pr-7-{OTHER}"
    run = dict(id=RUN, run_attempt=ATTEMPT, run_number=99, check_suite_id=456,
               workflow_id=279777287, path=".github/workflows/routed-rust.yml",
               event="merge_group", head_sha=SHA, head_branch=branch,
               display_title=f"Merge queue {SHA}", status="completed", conclusion="success",
               repository=dict(id=1243918801, full_name=REPO),
               head_repository=dict(id=1243918801, full_name=REPO),
               referenced_workflows=[dict(path=f"{REPO}/.github/workflows/{name}@{SHA}",
                                          sha=SHA, ref=f"refs/heads/{branch}") for name in
                                     ("rust-gates.yml", "rust-test-shards.yml", "vscode-e2e.yml")])
    outcomes = {
        "Route Ripr Rust Small": "success", "Detect Docs-Only Surface": "success",
        "Admit PR Candidate": "success", "vscode surface": "success", RESULT: "success",
        "Ripr Rust Small on CX43": "skipped", "Ripr Rust Small on CPX42": "skipped",
        "Ripr Rust Small on CX53": "skipped", "Ripr Docs Gate": "success" if docs else "skipped",
    }
    outcomes["vscode extension e2e / vscode" if extension else "vscode extension e2e"] = "success" if extension else "skipped"
    if docs:
        outcomes.update({"Ripr Rust Small on GitHub Hosted": "skipped", PREFIX: "skipped"})
    else:
        outcomes.update({"Ripr Rust Small on GitHub Hosted / Rust gates": "success",
                         f"{PREFIX} / Rust tests shard 1 of 2": "success",
                         f"{PREFIX} / Rust tests shard 2 of 2": "success",
                         f"{PREFIX} / Rust test shards verify": "success"})
    jobs = []
    for job_id, (name, conclusion) in enumerate(outcomes.items(), start=1000):
        steps = []
        if name == RESULT:
            steps = [dict(name=step, status="completed", conclusion="success") for step in (
                "Check out trusted admission bundle for the review recheck",
                "Recheck source review disposition for the admitted head",
                "Recheck initially admitted eligibility",
                "Require the extension e2e result for the admitted head",
                "Summarize routed result",
            )]
        jobs.append(dict(id=job_id, name=name, run_id=RUN, run_attempt=ATTEMPT,
                         head_sha=SHA, head_branch=branch, status="completed", conclusion=conclusion,
                         check_run_url=f"https://api.github.com/repos/{REPO}/check-runs/{job_id}",
                         html_url=f"https://github.com/{REPO}/actions/runs/{RUN}/job/{job_id}", steps=steps))
    result = next(job for job in jobs if job["name"] == RESULT)
    check = dict(id=result["id"], name=RESULT, head_sha=SHA, status="completed", conclusion="success",
                 check_suite=dict(id=456), app=dict(id=15368, slug="github-actions"), details_url=result["html_url"])
    return {MAIN: dict(ref="refs/heads/main", object=dict(type="commit", sha=SHA)),
            RUNS: dict(total_count=1, workflow_runs=[run]), RUN_PATH: copy.deepcopy(run),
            JOBS: dict(total_count=len(jobs), jobs=jobs), CHECKS: dict(total_count=1, check_runs=[check])}


class NativeAPI:
    def __init__(self, facts):
        self.facts = copy.deepcopy(facts)
        self.calls = []
        self.after = {}

    def get(self, path):
        self.calls.append(path)
        if path in self.after and self.calls.count(path) > 1:
            return copy.deepcopy(self.after[path])
        if path not in self.facts:
            raise OBSERVER.Unknown("fixture_read_unavailable")
        value = self.facts[path]
        if isinstance(value, Exception):
            raise value
        return copy.deepcopy(value)


def job(facts, name):
    return next(row for row in facts[JOBS]["jobs"] if row["name"] == name)


def all_runs(facts):
    return [facts[RUN_PATH], *facts[RUNS]["workflow_runs"]]


def workflow_step(name):
    source = (ROOT / ".github/workflows/main-health-sentinel.yml").read_text()
    selected = source.split(f"      - name: {name}\n", 1)[1].split("\n      - name:", 1)[0]
    script = selected.split("        run: |\n", 1)[1]
    return "\n".join(line[10:] if line.startswith("          ") else line for line in script.splitlines()).replace(
        "${{ github.server_url }}", "https://github.com").replace("${{ github.run_id }}", "99999")


FAKE_GH = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
root = pathlib.Path(os.environ["OBSERVER_FIXTURE"])
args = sys.argv[1:]
route = next((a for a in args if a.startswith("repos/") or a == "search/issues"), "")
calls = root / "calls.jsonl"
previous = [json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
is_read = "GET" in args or not any(arg in args for arg in ("-f", "-X", "--method"))
entry = {"route": route, "write": not is_read}
with calls.open("a") as output: output.write(json.dumps(entry) + "\n")
mode = os.environ.get("OBSERVER_MODE", "")
if route == "search/issues":
    print("" if mode == "no_tracker" else "7")
    raise SystemExit(0)
if "/issues" in route:
    print("{}" if not is_read else "")
    raise SystemExit(0)
suffix = route.split("repos/EffortlessMetrics/ripr-swarm/", 1)[-1]
facts = json.loads((root / "facts.json").read_text())
value = facts.get(suffix)
if value is None: raise SystemExit(1)
if suffix == "git/ref/heads/main":
    seen_main = sum(row["route"].endswith(suffix) for row in previous)
    if mode == "main_moved" or (mode == "main_during_recheck" and seen_main >= 1):
        value["object"]["sha"] = "b" * 40
if suffix.startswith("actions/runs?event=merge_group"):
    if mode == "competing_run" or (mode == "after_comment" and any(row["write"] for row in previous)):
        duplicate = dict(value["workflow_runs"][0]); duplicate["id"] += 1
        value["workflow_runs"].append(duplicate); value["total_count"] += 1
    if mode == "new_attempt": value["workflow_runs"][0]["run_attempt"] = 2
print(json.dumps(value))
'''


class MainQualificationTests(unittest.TestCase):
    def observe(self, facts=None, api=None):
        api = api or NativeAPI(facts if facts is not None else fixture())
        return OBSERVER.observe(REPO, SHA, api)

    def unknown(self, facts=None, api=None):
        observation = self.observe(facts, api)
        self.assertEqual("unknown", observation["verdict"], observation)
        return observation

    def test_hosted_and_docs_native_results_without_new_proof(self):
        for docs in (False, True):
            for extension in (False, True):
                with self.subTest(docs=docs, extension=extension):
                    api = NativeAPI(fixture(docs, extension))
                    result = self.observe(api=api)
                    self.assertEqual("green", result["verdict"], result)
                    self.assertEqual((RUN, ATTEMPT, SHA), (result["run_id"], result["run_attempt"], result["sha"]))
                    self.assertEqual([MAIN, RUNS, RUN_PATH, JOBS, CHECKS, RUN_PATH, RUNS, MAIN], api.calls)

    def test_no_landed_correspondence_never_selects_a_nearby_success_or_failure(self):
        facts = fixture()
        facts[RUNS] = dict(total_count=0, workflow_runs=[])
        self.unknown(facts)
        for row in all_runs(facts):
            row["head_sha"] = OTHER
            row["conclusion"] = "failure"
        self.unknown(facts)
        facts = fixture()
        for row in all_runs(facts):
            row["head_sha"] = OTHER
        self.unknown(facts)

    def test_multiple_run_ids_are_unknown_even_if_one_passed(self):
        for conclusion, status in (("failure", "completed"), ("cancelled", "completed"), (None, "in_progress"), ("success", "completed")):
            facts = fixture()
            duplicate = copy.deepcopy(facts[RUN_PATH])
            duplicate.update(id=RUN + 1, conclusion=conclusion, status=status)
            facts[RUNS]["workflow_runs"].append(duplicate)
            facts[RUNS]["total_count"] = 2
            self.unknown(facts)

    def test_current_attempt_cannot_borrow_old_success(self):
        for status, conclusion in (("in_progress", None), ("completed", "failure"), ("completed", "cancelled")):
            facts = fixture()
            for row in all_runs(facts):
                row.update(run_attempt=2, status=status, conclusion=conclusion)
            facts[JOBS.replace("attempts/1/", "attempts/2/")] = facts.pop(JOBS)
            self.unknown(facts)

    def test_run_attempt_main_or_new_run_moving_during_read_is_unknown(self):
        for path in (MAIN, RUN_PATH, RUNS):
            api = NativeAPI(fixture())
            changed = copy.deepcopy(api.facts[path])
            if path == MAIN:
                changed["object"]["sha"] = OTHER
            elif path == RUN_PATH:
                changed.update(run_attempt=2, status="in_progress", conclusion=None)
            else:
                changed["workflow_runs"].append(copy.deepcopy(changed["workflow_runs"][0]))
                changed["total_count"] = 2
            api.after[path] = changed
            self.unknown(api=api)

    def test_complete_collections_are_required(self):
        for path in (RUNS, JOBS, CHECKS):
            for count in (101, -1, True, None, 0):
                facts = fixture()
                facts[path]["total_count"] = count
                self.unknown(facts)
            facts = fixture()
            facts[path] = OBSERVER.Unknown("denied")
            self.unknown(facts)

    def test_self_consistent_foreign_producer_cannot_impersonate_native_authority(self):
        changes = {"workflow_id": 9, "path": ".github/workflows/ci.yml", "event": "push",
                   "head_sha": OTHER, "head_branch": "main", "display_title": "Merge queue " + OTHER,
                   "repository": dict(id=999, full_name=REPO),
                   "head_repository": dict(id=1243918801, full_name="other/ripr-swarm")}
        for key, value in changes.items():
            with self.subTest(key=key):
                facts = fixture()
                for row in all_runs(facts):
                    row[key] = value
                self.unknown(facts)

    def test_workflow_source_must_be_exact_landed_generation(self):
        for key, value in (("sha", OTHER), ("ref", "refs/heads/main"), ("path", f"{REPO}/.github/workflows/rust-gates.yml@{OTHER}")):
            facts = fixture()
            for row in all_runs(facts):
                row["referenced_workflows"][0][key] = value
            self.unknown(facts)
        facts = fixture()
        for row in all_runs(facts):
            row["referenced_workflows"].pop()
        self.unknown(facts)

    def test_result_check_identity_requires_ids_app_suite_subject_and_native_url(self):
        for key, value in (("id", 8), ("head_sha", OTHER), ("name", "a different context"),
                           ("app", dict(id=9, slug="github-actions")), ("check_suite", dict(id=9)),
                           ("details_url", f"https://github.com/{REPO}/actions/runs/999/job/1004"),
                           ("conclusion", "failure")):
            facts = fixture()
            facts[CHECKS]["check_runs"][0][key] = value
            self.unknown(facts)

    def test_result_cannot_pass_by_job_name_or_skipped_control(self):
        for name in tuple(step["name"] for step in job(fixture(), RESULT)["steps"]):
            for conclusion in ("skipped", "failure", "cancelled", None):
                facts = fixture()
                step = next(row for row in job(facts, RESULT)["steps"] if row["name"] == name)
                step["conclusion"] = conclusion
                self.unknown(facts)
        facts = fixture()
        job(facts, RESULT)["steps"] = []
        self.unknown(facts)

    def test_job_subject_attempt_and_unique_identity_are_required(self):
        for key, value in (("head_sha", OTHER), ("run_id", 999), ("run_attempt", 2),
                           ("head_branch", "main"), ("check_run_url", "https://github.com/other"),
                           ("html_url", "https://github.com/other")):
            facts = fixture()
            job(facts, RESULT)[key] = value
            self.unknown(facts)
        facts = fixture()
        facts[JOBS]["jobs"].append(copy.deepcopy(job(facts, RESULT)))
        facts[JOBS]["total_count"] += 1
        self.unknown(facts)

    def test_missing_failed_cancelled_and_extra_executed_producers_do_not_pass(self):
        for name in (RESULT, "Admit PR Candidate", "vscode surface", f"{PREFIX} / Rust tests shard 2 of 2", f"{PREFIX} / Rust test shards verify"):
            facts = fixture()
            facts[JOBS]["jobs"].remove(job(facts, name))
            facts[JOBS]["total_count"] -= 1  # A complete but insufficient inventory must still refuse.
            self.unknown(facts)
            for conclusion in ("failure", "cancelled", "skipped"):
                facts = fixture()
                job(facts, name)["conclusion"] = conclusion
                self.unknown(facts)
        facts = fixture()
        job(facts, f"{PREFIX} / Rust tests shard 2 of 2")["name"] = f"{PREFIX} / Rust tests shard 2 of 4"
        self.unknown(facts)
        facts = fixture()
        job(facts, "Ripr Rust Small on CX43")["conclusion"] = "success"
        self.unknown(facts)

    def test_docs_route_still_needs_actual_docs_proof(self):
        facts = fixture(docs=True)
        job(facts, "Ripr Docs Gate")["conclusion"] = "skipped"
        self.unknown(facts)

    def test_authentic_failed_qualification_of_current_main_is_red_not_a_product_diagnosis(self):
        facts = fixture()
        for row in all_runs(facts):
            row["conclusion"] = "failure"
        job(facts, RESULT)["conclusion"] = "failure"
        facts[CHECKS]["check_runs"][0]["conclusion"] = "failure"
        result = self.observe(facts)
        self.assertEqual("red", result["verdict"], result)
        self.assertEqual("landed_native_qualification_failed", result["reason"])
        facts[MAIN]["object"]["sha"] = OTHER
        self.unknown(facts)

    def test_entrypoint_writes_truthful_unknown_and_nonzero_exit(self):
        with tempfile.TemporaryDirectory(prefix="main-qualification-") as temporary:
            output = Path(temporary) / "qualification.json"
            api = NativeAPI(fixture())
            api.facts[RUNS] = dict(total_count=0, workflow_runs=[])
            argv = ["main-qualification.py", "--repository", REPO, "--sha", SHA, "--out", str(output)]
            with patch.object(sys, "argv", argv), patch.object(OBSERVER, "GitHub", return_value=api), patch("sys.stdout", new=io.StringIO()), patch.dict(OBSERVER.os.environ, {}, clear=True):
                self.assertEqual(1, OBSERVER.main())
            self.assertEqual("unknown", json.loads(output.read_text())["verdict"])

    def test_transport_is_fixed_host_get_bounded_and_sanitizes_errors(self):
        completed = subprocess.CompletedProcess([], 0, b'{"ok":true}', b"")
        with patch.object(OBSERVER.subprocess, "run", return_value=completed) as spawn:
            self.assertEqual({"ok": True}, OBSERVER.GitHub().get("git/ref/heads/main"))
            command = spawn.call_args.args[0]
            self.assertEqual(["gh", "api", "--hostname", "github.com", "--method", "GET",
                              f"repos/{REPO}/git/ref/heads/main"], command)
            self.assertLessEqual(spawn.call_args.kwargs["timeout"], 15)
        completed = subprocess.CompletedProcess([], 1, b"sensitive response", b"sensitive token")
        with patch.object(OBSERVER.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(OBSERVER.Unknown, "^native_read_failed$"):
                OBSERVER.GitHub().get("git/ref/heads/main")

    def tracker(self, verdict, mode=""):
        with tempfile.TemporaryDirectory(prefix="main-tracker-") as temporary:
            root = Path(temporary)
            bindir = root / "bin"
            bindir.mkdir()
            gh = bindir / "gh"
            gh.write_text(FAKE_GH)
            gh.chmod(0o700)
            facts = fixture()
            if verdict == "red":
                for row in all_runs(facts): row["conclusion"] = "failure"
                job(facts, RESULT)["conclusion"] = "failure"
                facts[CHECKS]["check_runs"][0]["conclusion"] = "failure"
            (root / "facts.json").write_text(json.dumps(facts))
            (root / "main-health-sentinel").mkdir()
            (root / "main-health-sentinel/qualification.json").write_text(json.dumps(
                dict(verdict=verdict, sha=SHA, run_id=RUN, run_attempt=ATTEMPT)))
            environment = {**os.environ, "PATH": f"{bindir}:{os.environ['PATH']}",
                           "OBSERVER_FIXTURE": str(root), "OBSERVER_MODE": mode,
                           "RUNNER_TEMP": str(root), "GITHUB_SHA": SHA,
                           "REPOSITORY": REPO, "VERDICT": verdict}
            environment.pop("GITHUB_STEP_SUMMARY", None)
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", workflow_step("Reconcile the main-red tracker")],
                                    cwd=ROOT, env=environment, capture_output=True, text=True, timeout=20)
            calls = root / "calls.jsonl"
            records = [json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
            return result, records

    def test_actual_tracker_unknown_performs_no_api_or_issue_mutation(self):
        result, calls = self.tracker("unknown")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual([], calls)

    def test_actual_tracker_recovery_reobserves_before_each_write(self):
        result, calls = self.tracker("green")
        self.assertEqual(0, result.returncode, result.stderr)
        writes = [row["route"] for row in calls if row["write"]]
        self.assertEqual([f"repos/{REPO}/issues/7/comments", f"repos/{REPO}/issues/7"], writes)
        self.assertEqual(4, sum(row["route"].endswith(MAIN) for row in calls))

    def test_actual_tracker_refuses_late_ambiguity_attempt_or_main_movement(self):
        for mode in ("competing_run", "new_attempt", "main_moved", "main_during_recheck"):
            with self.subTest(mode=mode):
                result, calls = self.tracker("green", mode)
                self.assertNotEqual(0, result.returncode)
                self.assertFalse(any(row["write"] for row in calls), calls)
        result, calls = self.tracker("green", "after_comment")
        self.assertNotEqual(0, result.returncode)
        self.assertEqual([f"repos/{REPO}/issues/7/comments"], [row["route"] for row in calls if row["write"]])

    def test_actual_tracker_records_authentic_failure_without_rebuilding(self):
        result, calls = self.tracker("red", "no_tracker")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual([f"repos/{REPO}/issues"], [row["route"] for row in calls if row["write"]])

    def test_actual_judge_fails_unknown_missing_wrong_subject_and_red(self):
        for payload, expected in ((None, "unknown"), ({"verdict": "green", "sha": OTHER}, "unknown"),
                                  ({"verdict": "unknown", "sha": SHA}, "unknown"),
                                  ({"verdict": "red", "sha": SHA}, "red"),
                                  ({"verdict": "green", "sha": SHA}, "green")):
            with tempfile.TemporaryDirectory(prefix="main-judge-") as temporary:
                root = Path(temporary)
                (root / "main-health-sentinel").mkdir()
                if payload is not None:
                    (root / "main-health-sentinel/qualification.json").write_text(json.dumps(payload))
                output = root / "output"
                result = subprocess.run(["bash", "-euo", "pipefail", "-c", workflow_step("Judge native observation")],
                                        env={**os.environ, "RUNNER_TEMP": str(root), "GITHUB_OUTPUT": str(output), "GITHUB_SHA": SHA},
                                        capture_output=True, text=True, timeout=5)
                self.assertEqual(0 if expected == "green" else 1, result.returncode, result.stderr)
                self.assertEqual(f"verdict={expected}\n", output.read_text())


if __name__ == "__main__":
    unittest.main()
