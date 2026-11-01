"""One necessary exact-head check before an ordinary protected squash.

The caller must first complete substantive review and all other applicable gates.
This is not an approval, an all-check inventory, or an atomic check-and-merge.
"""

import argparse
import datetime
import json
import re
import subprocess
import sys

RESULT = "Ripr Rust Small Result"
RESULT_APP = 15368
SHA = re.compile(r"[0-9a-f]{40}\Z")
REPO = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")


class Refusal(Exception):
    pass


def timestamp(value):
    if not isinstance(value, str) or not value.endswith("Z"):
        raise Refusal("check start time is missing or ambiguous")
    try:
        return datetime.datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise Refusal("check start time is invalid") from error


def require_pr(pr, head):
    if not isinstance(pr, dict) or pr.get("state") != "open" or pr.get("draft") is not False:
        raise Refusal("PR is not open and ready")
    if not isinstance(pr.get("head"), dict) or pr["head"].get("sha") != head:
        raise Refusal("PR head does not match the expected head")
    if pr.get("mergeable") is not True or pr.get("mergeable_state") not in ("clean", "unstable"):
        raise Refusal("GitHub merge state is not eligible; inspect live gates")


def inventory(pages, key, *, cap=None):
    if not isinstance(pages, list) or not pages:
        raise Refusal(f"{key} inventory is missing")
    counts, items = [], []
    for page in pages:
        if not isinstance(page, dict) or type(page.get("total_count")) is not int or not isinstance(page.get(key), list):
            raise Refusal(f"{key} inventory is partial")
        counts.append(page["total_count"])
        items.extend(page[key])
    if len(set(counts)) != 1 or counts[0] != len(items) or (cap is not None and counts[0] >= cap):
        raise Refusal(f"{key} inventory is incomplete, capped or changed")
    if any(not isinstance(item, dict) or type(item.get("id")) is not int for item in items):
        raise Refusal(f"{key} identity is missing")
    if len({item["id"] for item in items}) != len(items):
        raise Refusal(f"{key} inventory has duplicate identities")
    return items


# Audited heavy pull-request route; changed workflow blobs require a new audit.
AUDITED_REPO = "EffortlessMetrics/ripr-swarm"
AUDITED_REPO_ID = 1243918801
AUDITED_WORKFLOW_ID = 279777287
WORKFLOW_NAMES = ("routed-rust.yml", "rust-gates.yml", "rust-test-shards.yml")
# Each complete caller/gates/shards tuple must be independently reviewed.
# Do not independently allow individual blobs or mix entries across tuples.
AUDITED_WORKFLOW_TUPLES = frozenset({
    (
        "c70f19a72e8c2d2a14386f3a705ad052fc5d49ab",
        "40f8019407d7906048577373e00dfec1e99d3c85",
        "a7b828c0f26d08c34c3a9b3b0e4f02c9427daac0",
    ),
    (
        "703237707f6270284f5aedaa1af9b174f950f697",
        "f6a3d011c7014bcc7fe53c176ac49541e06ec5a5",
        "a7b828c0f26d08c34c3a9b3b0e4f02c9427daac0",
    ),
})


def require_native_pr_binding(api, latest, head):
    if api.repo != AUDITED_REPO:
        raise Refusal("native PR binding is unsupported for this repository")
    detail = api.attempt(latest["id"], latest["run_attempt"])
    if not isinstance(detail, dict):
        raise Refusal("workflow attempt details missing")
    if (detail.get("id") != latest["id"] or detail.get("run_attempt") != latest["run_attempt"] or
        detail.get("head_sha") != head or detail.get("event") != "pull_request" or
        detail.get("status") != "completed" or detail.get("check_suite_id") != latest.get("check_suite_id") or
        detail.get("workflow_id") != AUDITED_WORKFLOW_ID or
        detail.get("path") != ".github/workflows/routed-rust.yml" or
        not isinstance(detail.get("repository"), dict) or detail["repository"].get("id") != AUDITED_REPO_ID):
        raise Refusal("workflow attempt identity does not match the audited route")
    refs = detail.get("referenced_workflows")
    if not isinstance(refs, list) or len(refs) != 2 or any(not isinstance(ref, dict) for ref in refs):
        raise Refusal("reusable workflow references missing or ambiguous")
    if any(not isinstance(ref.get("path"), str) or not isinstance(ref.get("ref"), str) or not isinstance(ref.get("sha"), str) for ref in refs):
        raise Refusal("reusable workflow reference identity missing")
    shas = {ref["sha"] for ref in refs}
    if len(shas) != 1:
        raise Refusal("reusable workflow execution subjects differ")
    merge = next(iter(shas))
    if not SHA.fullmatch(merge):
        raise Refusal("reusable workflow execution subject is invalid")
    expected = {f"{AUDITED_REPO}/.github/workflows/{name}@{merge}" for name in ("rust-gates.yml", "rust-test-shards.yml")}
    if ({ref.get("path") for ref in refs} != expected or
        any(ref.get("ref") != f"refs/pull/{api.number}/merge" or ref.get("sha") != merge for ref in refs)):
        raise Refusal("reusable workflow references do not bind this PR")
    commit = api.commit(merge)
    if not isinstance(commit, dict) or commit.get("sha") != merge or not isinstance(commit.get("parents"), list):
        raise Refusal("merge commit identity missing")
    parents = commit["parents"]
    if (len(parents) != 2 or any(not isinstance(p, dict) or not isinstance(p.get("sha"), str) or not SHA.fullmatch(p["sha"]) for p in parents)
        or parents[1]["sha"] != head):
        raise Refusal("merge commit parents do not bind the expected head")
    configurations = []
    for ref in (parents[0]["sha"], merge):
        blobs = []
        for name in WORKFLOW_NAMES:
            item = api.workflow_file(name, ref)
            if not isinstance(item, dict) or not isinstance(item.get("sha"), str):
                raise Refusal("workflow source identity is missing")
            blobs.append(item["sha"])
        configurations.append(tuple(blobs))
    if configurations[0] != configurations[1] or configurations[0] not in AUDITED_WORKFLOW_TUPLES:
        raise Refusal("workflow source differs from audited configuration")


def require_result(api, head):
    # created_at orders initial requests, not check execution. A rerun of an
    # older request has no reliable request-order timestamp here, so refuse it
    # when more than one run exists instead of guessing its recency.
    runs = inventory(api.runs(head), "workflow_runs", cap=1000)
    if not runs:
        raise Refusal("qualification request is missing")
    if any(r.get("head_sha") != head or r.get("event") != "pull_request" or
           type(r.get("run_attempt")) is not int or r["run_attempt"] < 1 for r in runs):
        raise Refusal("qualification request identity is ambiguous")
    requests, metadata = [], []
    for run in runs:
        title = run.get("display_title")
        if title == f"Merge-ready request {head}":
            requests.append(run)
        elif title == f"Label metadata {run['id']}":
            metadata.append(run)
        else:
            raise Refusal("routed workflow run purpose is ambiguous")
    runs = requests
    if not runs:
        raise Refusal("qualification request is missing")
    if len(runs) > 1 and any(r["run_attempt"] != 1 for r in runs):
        raise Refusal("rerun request ordering is ambiguous")
    ordered = sorted(((timestamp(r.get("created_at")), r) for r in runs), key=lambda pair: pair[0])
    if len(ordered) > 1 and ordered[-1][0] == ordered[-2][0]:
        raise Refusal("latest qualification request is ambiguous")
    latest = ordered[-1][1]
    # A title alone does not authenticate a metadata event: a changed caller
    # can disguise a newer qualification request using the same title.
    # A metadata rerun also has no reliable request-order timestamp here.
    if any(run["run_attempt"] != 1 or timestamp(run.get("created_at")) >= ordered[-1][0] for run in metadata):
        raise Refusal("a newer or rerun metadata-labelled workflow has unverified origin")
    if latest.get("status") != "completed":
        raise Refusal("latest qualification request is not completed")
    prs = latest.get("pull_requests")
    if not isinstance(prs, list) or (prs and (len(prs) != 1 or not isinstance(prs[0], dict) or prs[0].get("number") != api.number)):
        raise Refusal("qualification request is not authoritatively bound to this PR")
    # A PR association is not proof that the caller and reusable workflows are
    # the audited qualification route. Apply the same provenance check to both
    # empty and nonempty associations.
    require_native_pr_binding(api, latest, head)
    suite = latest.get("check_suite_id")
    if type(suite) is not int:
        raise Refusal("qualification check suite identity is missing")
    jobs = inventory(api.jobs(latest["id"], latest["run_attempt"]), "jobs")
    matches = [j for j in jobs if j.get("name") == RESULT]
    if len(matches) != 1:
        raise Refusal("qualification Result job is missing or ambiguous")
    job = matches[0]
    if (job.get("run_id") != latest["id"] or job.get("run_attempt") != latest["run_attempt"] or
        job.get("head_sha") != head or job.get("status") != "completed" or job.get("conclusion") != "success"):
        raise Refusal("qualification Result job is not a successful matching run and attempt")
    checks = inventory(api.suite_checks(suite), "check_runs")
    matches = [c for c in checks if c.get("id") == job["id"] and c.get("name") == RESULT]
    if len(matches) != 1:
        raise Refusal("Result check is missing or ambiguous in the selected suite")
    check = matches[0]
    expected_url = f"/actions/runs/{latest['id']}/job/{job['id']}"
    check_suite, app = check.get("check_suite"), check.get("app")
    if (check.get("head_sha") != head or not isinstance(check_suite, dict) or check_suite.get("id") != suite or
        not isinstance(app, dict) or app.get("id") != RESULT_APP or
        check.get("status") != "completed" or check.get("conclusion") != "success" or
        not isinstance(check.get("details_url"), str) or
        not check["details_url"].endswith(expected_url)):
        raise Refusal("Result check does not match the successful request, suite and app")


class Gh:
    def __init__(self, repo, number):
        self.pr_path = f"repos/{repo}/pulls/{number}"
        self.repo = repo
        self.number = number

    def api(self, args):
        try:
            result = subprocess.run(["gh", "api", *args], capture_output=True, text=True, timeout=45, check=False)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise Refusal(f"GitHub request unavailable: {error}") from error
        if result.returncode:
            raise Refusal(f"GitHub request failed (exit {result.returncode}): {result.stderr.strip()}")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise Refusal("GitHub response is not valid JSON") from error

    def pr(self):
        return self.api([self.pr_path])

    def runs(self, head):
        return self.api(["--paginate", "--slurp", f"repos/{self.repo}/actions/workflows/routed-rust.yml/runs?event=pull_request&head_sha={head}&per_page=100"])

    def attempt(self, run, attempt):
        return self.api([f"repos/{self.repo}/actions/runs/{run}/attempts/{attempt}"])

    def commit(self, sha):
        return self.api([f"repos/{self.repo}/git/commits/{sha}"])

    def workflow_file(self, name, ref):
        return self.api([f"repos/{self.repo}/contents/.github/workflows/{name}?ref={ref}"])

    def jobs(self, run, attempt):
        return self.api(["--paginate", "--slurp", f"repos/{self.repo}/actions/runs/{run}/attempts/{attempt}/jobs?per_page=100"])

    def suite_checks(self, suite):
        return self.api(["--paginate", "--slurp", f"repos/{self.repo}/check-suites/{suite}/check-runs?filter=all&per_page=100"])

    def merge(self, head):
        try:
            return self.api(["--method", "PUT", self.pr_path + "/merge", "-f", "merge_method=squash", "-f", f"sha={head}"])
        except Refusal as error:
            raise Refusal(f"merge outcome may be unknown; read PR state before retry: {error}") from error


def guarded_merge(api, head):
    if not SHA.fullmatch(head):
        raise Refusal("expected head must be a full lowercase SHA")
    # Refresh both observations immediately before the conditional mutation.
    for _ in range(2):
        require_pr(api.pr(), head)
        require_result(api, head)
    result = api.merge(head)
    if not isinstance(result, dict) or result.get("merged") is not True:
        raise Refusal("merge was not confirmed; read PR state before retry")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="owner/repository")
    parser.add_argument("--pr", required=True, type=int)
    parser.add_argument("--expected-head", required=True)
    args = parser.parse_args()
    if not REPO.fullmatch(args.repo) or args.pr <= 0:
        parser.error("invalid repository or PR number")
    try:
        print(json.dumps(guarded_merge(Gh(args.repo, args.pr), args.expected_head)))
    except Refusal as error:
        print(f"guarded merge refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
