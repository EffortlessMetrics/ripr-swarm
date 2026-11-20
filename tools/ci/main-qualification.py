"""Observe prior native qualification of exact landed main; never execute proof.

This advisory reader accepts only G=M, the installed Actions producer, and its
current complete attempt. The existing required Result owns applicability and
proof aggregation. These checks bind that result to native facts and recognize
the current hosted two-shard/docs layouts; they are not another proof planner.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time


REPOSITORY = "EffortlessMetrics/ripr-swarm"
REPOSITORY_ID = 1243918801
WORKFLOW_ID = 279777287
WORKFLOW_PATH = ".github/workflows/routed-rust.yml"
APP_ID = 15368
RESULT = "Ripr Rust Small Result"
GATES = "Ripr Rust Small on GitHub Hosted / Rust gates"
TESTS = "Ripr Rust Small tests on GitHub Hosted"
SHARDS = {f"{TESTS} / Rust tests shard {i} of 2" for i in (1, 2)}
VERIFY = f"{TESTS} / Rust test shards verify"
DOCS = "Ripr Docs Gate"
CORE = {"Route Ripr Rust Small", "Detect Docs-Only Surface", "Admit PR Candidate", "vscode surface"}
UNSELECTED = {f"Ripr Rust Small on {runner}" for runner in ("CX43", "CPX42", "CX53")}
RESULT_STEPS = {
    "Check out trusted admission bundle for the review recheck",
    "Recheck source review disposition for the admitted head",
    "Recheck initially admitted eligibility",
    "Require the extension e2e result for the admitted head",
    "Summarize routed result",
}
REUSABLES = {"rust-gates.yml", "rust-test-shards.yml", "vscode-e2e.yml"}


class Unknown(ValueError):
    pass


def require(condition, reason):
    if not condition:
        raise Unknown(reason)


def positive(value):
    return type(value) is int and value > 0


def collection(payload, key):
    require(type(payload) is dict, "malformed_collection")
    rows, count = payload.get(key), payload.get("total_count")
    require(type(rows) is list and type(count) is int and 0 <= count <= 100
            and count == len(rows) and all(type(row) is dict for row in rows),
            "incomplete_or_oversized_collection")
    return rows


class GitHub:
    """Fixed-host, read-only, bounded transport; errors never expose tokens."""

    def __init__(self):
        self.deadline = time.monotonic() + 120

    def get(self, suffix):
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, "read_budget_exhausted")
        try:
            completed = subprocess.run(
                ["gh", "api", "--hostname", "github.com", "--method", "GET",
                 f"repos/{REPOSITORY}/{suffix}"],
                capture_output=True, check=False, timeout=min(15, remaining),
            )
            require(completed.returncode == 0, "native_read_failed")
            require(len(completed.stdout) <= 4 * 1024 * 1024, "native_response_too_large")
            return json.loads(completed.stdout)
        except (OSError, subprocess.TimeoutExpired, UnicodeDecodeError, json.JSONDecodeError) as error:
            raise Unknown("native_read_unavailable") from error


def current_main(api, sha):
    ref = api.get("git/ref/heads/main")
    require(type(ref) is dict and ref.get("ref") == "refs/heads/main"
            and ref.get("object", {}).get("type") == "commit"
            and ref.get("object", {}).get("sha") == sha, "main_moved_or_unreadable")


def inventory(api, sha):
    rows = collection(api.get(
        f"actions/runs?event=merge_group&head_sha={sha}&per_page=100&page=1"), "workflow_runs")
    # No success/completed filter: a later failed, cancelled or running attempt
    # cannot disappear behind an earlier success. Multiple run IDs are unknown;
    # creation ordering does not establish authority after an old run is rerun.
    matches = [row for row in rows if row.get("path") == WORKFLOW_PATH
               or row.get("workflow_id") == WORKFLOW_ID]
    require(len(matches) == 1, "missing_or_ambiguous_native_run")
    return matches[0]


def run_identity(run, sha):
    require(type(run) is dict, "malformed_run")
    require(positive(run.get("id")) and positive(run.get("run_attempt"))
            and positive(run.get("run_number")) and positive(run.get("check_suite_id")), "invalid_run_identity")
    require(run.get("workflow_id") == WORKFLOW_ID and run.get("path") == WORKFLOW_PATH
            and run.get("event") == "merge_group" and run.get("head_sha") == sha
            and run.get("display_title") == f"Merge queue {sha}", "wrong_native_producer")
    for key in ("repository", "head_repository"):
        owner = run.get(key, {})
        require(type(owner) is dict and owner.get("id") == REPOSITORY_ID
                and owner.get("full_name") == REPOSITORY, "wrong_run_repository")
    branch = run.get("head_branch", "")
    require(type(branch) is str and re.fullmatch(
        r"gh-readonly-queue/main/pr-[1-9][0-9]*-[0-9a-f]{40}", branch), "unbound_queue_ref")
    references = run.get("referenced_workflows")
    require(type(references) is list and len(references) == len(REUSABLES), "unsupported_workflow_sources")
    expected = {f"{REPOSITORY}/.github/workflows/{name}@{sha}" for name in REUSABLES}
    require(all(type(row) is dict and row.get("sha") == sha
                and row.get("ref") == f"refs/heads/{branch}" for row in references)
            and {row.get("path") for row in references} == expected, "wrong_workflow_source")
    return (run["id"], run["run_number"], run["run_attempt"], run["check_suite_id"],
            branch, run.get("status"), run.get("conclusion"))


def job_inventory(api, run, sha):
    run_id, attempt = run["id"], run["run_attempt"]
    rows = collection(api.get(
        f"actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100&page=1"), "jobs")
    jobs, ids = {}, set()
    for row in rows:
        job_id, name = row.get("id"), row.get("name")
        require(positive(job_id) and type(name) is str and name
                and job_id not in ids and name not in jobs, "ambiguous_job_inventory")
        require(row.get("run_id") == run_id and row.get("run_attempt") == attempt
                and row.get("head_sha") == sha and row.get("head_branch") == run["head_branch"]
                and row.get("check_run_url") == f"https://api.github.com/repos/{REPOSITORY}/check-runs/{job_id}"
                and row.get("html_url") == f"https://github.com/{REPOSITORY}/actions/runs/{run_id}/job/{job_id}",
                "wrong_job_subject_or_attempt")
        ids.add(job_id)
        jobs[name] = row
    require(RESULT in jobs, "missing_native_result")
    return jobs


def native_result(api, run, jobs, sha):
    result = jobs[RESULT]
    rows = collection(api.get(
        f"commits/{sha}/check-runs?check_name=Ripr%20Rust%20Small%20Result&filter=all&per_page=100&page=1"), "check_runs")
    matches = [row for row in rows if row.get("id") == result["id"]]
    require(len(matches) == 1, "missing_or_ambiguous_result_check")
    check = matches[0]
    require(check.get("name") == RESULT and check.get("head_sha") == sha
            and check.get("check_suite", {}).get("id") == run["check_suite_id"]
            and check.get("app", {}).get("id") == APP_ID
            and check.get("app", {}).get("slug") == "github-actions"
            and check.get("details_url") == result["html_url"]
            and check.get("status") == result.get("status")
            and check.get("conclusion") == result.get("conclusion"), "wrong_result_provenance")
    return result


def judge(run, jobs, result):
    require(run.get("status") == "completed" and result.get("status") == "completed", "qualification_incomplete")
    if result.get("conclusion") in {"failure", "timed_out", "action_required", "startup_failure"}:
        require(run.get("conclusion") in {"failure", "timed_out", "action_required", "startup_failure"},
                "inconsistent_native_conclusions")
        return "red", "landed_native_qualification_failed"
    require(run.get("conclusion") == "success" and result.get("conclusion") == "success", "qualification_not_successful")
    require(all(row.get("status") == "completed" and row.get("conclusion") in {"success", "skipped"}
                for row in jobs.values()), "incomplete_or_failed_native_job")
    steps = result.get("steps")
    require(type(steps) is list and all(type(step) is dict for step in steps), "missing_result_steps")
    for name in RESULT_STEPS:
        matches = [step for step in steps if step.get("name") == name]
        require(len(matches) == 1 and matches[0].get("status") == "completed"
                and matches[0].get("conclusion") == "success", "missing_or_failed_result_control")
    require(all(name in jobs and jobs[name].get("conclusion") == "success" for name in CORE), "missing_admission_or_selection")
    require(all(name in jobs and jobs[name].get("conclusion") == "skipped" for name in UNSELECTED), "unsupported_execution_route")
    # Recognize the native producer layout, not changed-file applicability.
    # Result has already enforced which route and optional extension were due.
    if jobs.get(DOCS, {}).get("conclusion") == "success":
        required = {DOCS}
        skipped = {"Ripr Rust Small on GitHub Hosted", TESTS}
    else:
        required = {GATES, VERIFY, *SHARDS}
        skipped = {DOCS}
    require(all(name in jobs and jobs[name].get("conclusion") == "success" for name in required)
            and all(name in jobs and jobs[name].get("conclusion") == "skipped" for name in skipped),
            "incomplete_or_unsupported_producer_layout")
    extension = {name for name in jobs if name.startswith("vscode extension e2e")}
    require(extension in ({"vscode extension e2e"}, {"vscode extension e2e / vscode"}), "unsupported_extension_layout")
    extension_name = next(iter(extension))
    require(jobs[extension_name]["conclusion"] == ("skipped" if extension_name == "vscode extension e2e" else "success"),
            "incomplete_extension_producer")
    require(set(jobs) == CORE | UNSELECTED | required | skipped | extension | {RESULT}, "unsupported_or_extra_job_layout")
    return "green", "exact_landed_native_qualification_observed"


def observe(repository, sha, api):
    observation = {"verdict": "unknown", "reason": "observation_unavailable", "sha": sha,
                   "run_id": None, "run_attempt": None, "run_url": None}
    try:
        require(repository == REPOSITORY and type(sha) is str
                and re.fullmatch(r"[0-9a-f]{40}", sha), "invalid_observation_subject")
        current_main(api, sha)
        selected = inventory(api, sha)
        identity = run_identity(selected, sha)
        run = api.get(f"actions/runs/{selected['id']}")
        require(run_identity(run, sha) == identity, "run_changed_during_observation")
        observation.update(run_id=run["id"], run_attempt=run["run_attempt"],
                           run_url=f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}")
        jobs = job_inventory(api, run, sha)
        result = native_result(api, run, jobs, sha)
        verdict, reason = judge(run, jobs, result)
        # Fence this read-only observation against main movement, another run,
        # or a new attempt. No receipt grants admission or mutation authority.
        require(run_identity(api.get(f"actions/runs/{run['id']}"), sha) == identity
                and run_identity(inventory(api, sha), sha) == identity, "run_superseded_during_observation")
        current_main(api, sha)
        observation.update(verdict=verdict, reason=reason)
    except (Unknown, KeyError, TypeError, AttributeError) as error:
        observation["reason"] = str(error) if isinstance(error, Unknown) else "malformed_native_evidence"
    return observation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    observation = observe(args.repository, args.sha, GitHub())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(observation, sort_keys=True) + "\n")
    summary = ("### Landed native qualification\n\n"
               f"- Observed main: `{observation['sha']}`\n"
               f"- Verdict: **{observation['verdict']}** ({observation['reason']})\n"
               f"- Native run: {observation['run_url'] or 'unestablished'}; attempt {observation['run_attempt']}\n\n"
               "This reads prior exact-G qualification. It runs no product proof and grants no merge authority.\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as output:
            output.write(summary)
    print(summary, end="")
    return 0 if observation["verdict"] == "green" else 1


if __name__ == "__main__":
    raise SystemExit(main())
