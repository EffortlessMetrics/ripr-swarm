#!/usr/bin/env python3
"""Read-only Hermes v2 intake before candidate checkout/cache allocation.

Protocol: hermes-deploy 60a7e753, scripts/app-publisher.py; read-consumer
semantics adapted from perl-lsp-swarm 2858e8c9, native_queue_title.py.
Trust configuration and this evaluator come from protected B, never G. The
App-authenticated receipt authorizes the exact target producer closure at G;
requiring that target to equal B would prevent reviewed CI changes.
No enqueue, check publication, credential minting or settings operation exists.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tomllib
from urllib.parse import urlencode
from urllib.request import HTTPRedirectHandler, Request, build_opener

MAX_BYTES = 1024 * 1024
REPOSITORY = "EffortlessMetrics/ripr-swarm"
ROOTS = {".github": "tree", ".cargo": "tree", ".config": "tree", "Cargo.toml": "blob",
         "Cargo.lock": "blob", "rust-toolchain.toml": "blob", "policy": "tree",
         "scripts": "tree", "xtask": "tree", "tools/repo-policy": "tree"}
INTAKE_KEYS = {"schema_id", "schema_version", "repository", "pull_number", "head_sha",
               "entry_id", "admission_request_id", "enqueue_request_id", "base_sha",
               "expires_at", "authority_sha256", "producer_closure", "producer_generation_sha256"}
OBSERVE = """query { repository(owner:"EffortlessMetrics",name:"ripr-swarm") {
  id nameWithOwner mergeQueue(branch:"main") { id entries(first:100) {
    totalCount pageInfo { hasNextPage } nodes {
      id position baseCommit { oid } headCommit { oid }
      pullRequest { id number state isDraft headRefOid baseRefName
        headRepository { nameWithOwner } mergeQueueEntry { id } }
    }
  } }
} }"""


class Refused(ValueError):
    pass


def require(condition, reason):
    if not condition:
        raise Refused(reason)


def positive(value):
    return type(value) is int and value > 0


def oid(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}", value) is not None


def node(value):
    return isinstance(value, str) and 0 < len(value) <= 512 and all(32 < ord(c) < 127 for c in value)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def decode(raw):
    require(isinstance(raw, (str, bytes)) and len(raw) <= MAX_BYTES, "JSON observation exceeds bound")
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate JSON field")
            result[key] = value
        return result
    def constant(_):
        raise Refused("nonfinite JSON")
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=constant)


def configuration(value):
    require(isinstance(value, dict) and type(value.get("schema_version")) is int
            and value["schema_version"] == 1 and value.get("enabled") is True,
            "protected publisher binding is not configured")
    require(value.get("repository") == REPOSITORY and value.get("base_branch") == "main"
            and positive(value.get("publisher_app_id")) and value["publisher_app_id"] != 15368
            and value.get("publisher_check_name") == "CI Lifecycle Qualification"
            and value.get("producer_closure") == ROOTS, "invalid protected publisher binding")
    return value


class GitHub:
    """Bounded reads only; fixed GitHub origin, no candidate URL or redirects."""
    def __init__(self, token):
        require(isinstance(token, str) and bool(token), "Checks-read token is absent")
        self.token = token
        self.reads = 0

    def read(self, path, body=None):
        self.reads += 1
        require(self.reads <= 128 and path.startswith("/"), "API observation bound exhausted")
        data = None if body is None else canonical(body).encode()
        request = Request("https://api.github.com" + path, data=data,
                          headers={"Authorization": "Bearer " + self.token,
                                   "Accept": "application/vnd.github+json",
                                   "Content-Type": "application/json"})
        class NoRedirect(HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl):
                raise Refused("API redirects are not accepted")
        with build_opener(NoRedirect()).open(request, timeout=10) as response:
            require(response.status == 200 and response.geturl() == request.full_url,
                    "API response identity changed")
            return decode(response.read(MAX_BYTES + 1))

    def get(self, suffix):
        return self.read("/repos/" + REPOSITORY + suffix)

    def queue(self):
        # Fixed query, no mutation is accepted by this interface.
        return self.read("/graphql", {"query": OBSERVE})


def event_subject(event, env):
    require(env.get("GITHUB_EVENT_NAME") == "merge_group"
            and event.get("action") == "checks_requested", "native merge_group event required")
    repository, group = event.get("repository", {}), event.get("merge_group", {})
    require(repository.get("full_name") == REPOSITORY and env.get("GITHUB_REPOSITORY") == REPOSITORY
            and positive(repository.get("id")) and node(repository.get("node_id")), "repository identity mismatch")
    base, head, ref = group.get("base_sha"), group.get("head_sha"), group.get("head_ref")
    require(oid(base) and oid(head) and base != head and head == env.get("GITHUB_SHA")
            and group.get("base_ref") == "refs/heads/main" and ref == env.get("GITHUB_REF")
            and isinstance(ref, str) and ref.startswith("refs/heads/gh-readonly-queue/main/")
            and re.fullmatch(r"[A-Za-z0-9_./-]{1,250}", ref) and ".." not in ref and "//" not in ref,
            "native B/G/ref identity mismatch")
    require(all(re.fullmatch(r"[1-9][0-9]*", env.get(key, ""))
                for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT")), "run identity absent")
    return {"repository": REPOSITORY, "repository_node_id": repository["node_id"],
            "base_sha": base, "group_sha": head, "head_ref": ref,
            "request": env["GITHUB_RUN_ID"] + "-" + env["GITHUB_RUN_ATTEMPT"]}


def member(api, subject):
    frame = api.queue()
    require(isinstance(frame, dict) and frame.get("errors") is None, "native queue observation failed")
    repository = frame["data"]["repository"]
    require(repository["nameWithOwner"] == REPOSITORY and repository["id"] == subject["repository_node_id"],
            "native queue repository mismatch")
    entries = repository["mergeQueue"]["entries"]
    rows = entries["nodes"]
    require(type(entries["totalCount"]) is int and isinstance(rows, list)
            and entries["totalCount"] == len(rows) <= 100
            and entries["pageInfo"]["hasNextPage"] is False, "native queue observation incomplete")
    require(all(isinstance(row, dict) and node(row.get("id")) and positive(row.get("position")) for row in rows)
            and sorted(row["position"] for row in rows) == list(range(1, len(rows) + 1))
            and len({row["id"] for row in rows}) == len(rows), "invalid native queue ordering")
    matches = [row for row in rows if row.get("headCommit", {}).get("oid") == subject["group_sha"]
               and row.get("baseCommit", {}).get("oid") == subject["base_sha"]]
    require(len(matches) == 1 and matches[0]["position"] == 1,
            "native singleton B/G join is not proven")
    row, pr = matches[0], matches[0]["pullRequest"]
    require(positive(pr["number"]) and node(pr["id"]) and oid(pr["headRefOid"])
            and pr["state"] == "OPEN" and pr["isDraft"] is False and pr["baseRefName"] == "main"
            and pr["headRepository"]["nameWithOwner"] == REPOSITORY
            and pr["mergeQueueEntry"]["id"] == row["id"], "current queued PR identity changed")
    return {"pull_number": pr["number"], "pr_id": pr["id"], "head_sha": pr["headRefOid"], "entry_id": row["id"]}


def closure(value, configured):
    require(isinstance(value, dict) and set(ROOTS) <= set(value) and len(value) <= 100,
            "admitted producer closure omits required sources or exceeds bound")
    for path, entry in value.items():
        require(isinstance(path, str) and re.fullmatch(r"[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*", path)
                and not any(part in {".", "..", ".git"} for part in path.split("/"))
                and isinstance(entry, dict) and set(entry) == {"kind", "sha", "mode"} and oid(entry["sha"])
                and ((entry["kind"] == "tree" and entry["mode"] == "040000")
                     or (entry["kind"] == "blob" and entry["mode"] in {"100644", "100755"}))
                and (path not in ROOTS or entry["kind"] == configured["producer_closure"][path]),
                "invalid producer closure path/kind/object/mode")
    return value


def intake(api, current, configured, now):
    result = api.get("/commits/" + current["head_sha"] + "/check-runs?" + urlencode({
        "check_name": configured["publisher_check_name"], "filter": "all", "per_page": 100}))
    require(isinstance(result, dict) and type(result.get("total_count")) is int
            and isinstance(result.get("check_runs"), list)
            and result["total_count"] == len(result["check_runs"]) <= 100, "H checks observation incomplete")
    matches = []
    for check in result["check_runs"]:
        if (check.get("name") != configured["publisher_check_name"]
                or not positive(check.get("app", {}).get("id"))
                or check["app"]["id"] != configured["publisher_app_id"]):
            continue
        raw = check.get("output", {}).get("text")
        if not isinstance(raw, str):
            continue
        data = decode(raw)
        if not isinstance(data, dict) or data.get("entry_id") != current["entry_id"]:
            continue
        require(set(data) == INTAKE_KEYS and data["schema_id"] == "hermes.queue-intake.v2"
                and type(data["schema_version"]) is int and data["schema_version"] == 2
                and data["repository"] == REPOSITORY and type(data["pull_number"]) is int
                and data["pull_number"] == current["pull_number"] and data["head_sha"] == current["head_sha"]
                and oid(data["base_sha"])
                and all(isinstance(data[key], str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}", data[key])
                        for key in ("admission_request_id", "enqueue_request_id"))
                and all(isinstance(data[key], str) and re.fullmatch(r"[0-9a-f]{64}", data[key])
                        for key in ("authority_sha256", "producer_generation_sha256"))
                and positive(check.get("id")) and check.get("head_sha") == current["head_sha"]
                and check.get("external_id") == "hermes-queue:" + data["admission_request_id"] + ":H"
                and check.get("status") == "completed" and check.get("conclusion") == "success",
                "App-authenticated exact-H intake is invalid")
        closure(data["producer_closure"], configured)
        require(isinstance(data["expires_at"], str), "missing intake expiry")
        expiry = datetime.fromisoformat(data["expires_at"].replace("Z", "+00:00"))
        require(expiry.tzinfo is not None and expiry > now, "intake expired")
        matches.append({"check_run_id": check["id"], "app_id": check["app"]["id"], "intake": data})
    require(len(matches) == 1, "unique authenticated intake is not proven")
    return matches[0]


def verify_target(api, group, expected):
    commit = api.get("/git/commits/" + group)
    require(commit["sha"] == group and oid(commit["tree"]["sha"]), "group commit unavailable")
    trees = {}
    for path, bound in expected.items():
        tree = commit["tree"]["sha"]
        for index, part in enumerate(path.split("/")):
            if tree not in trees:
                value = api.get("/git/trees/" + tree)
                require(value["sha"] == tree and value.get("truncated") is False
                        and isinstance(value["tree"], list), "producer tree observation incomplete")
                trees[tree] = value["tree"]
            matches = [entry for entry in trees[tree] if entry.get("path") == part]
            require(len(matches) == 1, "producer path absent or ambiguous")
            entry = matches[0]
            if index + 1 < len(path.split("/")):
                require(entry.get("type") == "tree" and entry.get("mode") == "040000" and oid(entry.get("sha")),
                        "producer path traverses a non-tree")
                tree = entry["sha"]
        require({"kind": entry.get("type"), "sha": entry.get("sha"), "mode": entry.get("mode")} == bound,
                "actual G differs from authenticated producer closure")


def observe(api, subject, configured, now, expected=None):
    first = member(api, subject)
    admitted = intake(api, first, configured, now())
    verify_target(api, subject["group_sha"], admitted["intake"]["producer_closure"])
    # Membership and authority can move during source reads. Never adopt a new H.
    require(member(api, subject) == first and intake(api, first, configured, now()) == admitted,
            "native intake changed during observation")
    result = {"subject": subject, "member": first, "admission": admitted}
    require(expected is None or result == expected, "initial native intake was superseded")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", required=True)
    parser.add_argument("--expected-env")
    args = parser.parse_args()
    configured = configuration(tomllib.loads(Path(args.policy).read_text(encoding="utf-8")))
    subject = event_subject(decode(Path(os.environ["GITHUB_EVENT_PATH"]).read_bytes()), os.environ)
    expected = decode(os.environ[args.expected_env]) if args.expected_env else None
    result = observe(GitHub(os.environ.get("GH_TOKEN")), subject, configured,
                     lambda: datetime.now(timezone.utc), expected)
    current, admitted = result["member"], result["admission"]["intake"]
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in {"pr": current["pull_number"], "pr_id": current["pr_id"], "head": current["head_sha"],
                           "queue_entry": current["entry_id"], "admission_id": admitted["admission_request_id"],
                           "intake_json": canonical(result), "intake_identity": digest(result)}.items():
            output.write(f"{key}={value}\n")
    print("Authenticated exact-H native intake and reviewed G producer closure verified")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Never print API bodies, tokens or provider errors into candidate logs.
        print("queue intake refused: " + (str(error) if isinstance(error, Refused) else type(error).__name__), file=sys.stderr)
        sys.exit(1)
