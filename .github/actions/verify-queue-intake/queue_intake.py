"""Fail-closed validation of Hermes v2 H intake observations.

This module performs no network I/O and grants no merge authority. A future
trusted caller must execute these bytes from an independently trusted source,
provide authenticated GitHub API readers scoped to the configured repository,
and join the returned generation and closure to actual G before allocation.
Inputs from a candidate file, a fabricated API response, or an unauthenticated
caller are not evidence. This module is intentionally not wired to a workflow.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import math
import re
from typing import Callable
from urllib.parse import urlencode

MAX_BYTES = 1024 * 1024
OID = re.compile(r"[0-9a-f]{40}\Z")
HEX256 = re.compile(r"[0-9a-f]{64}\Z")
NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}\Z")
PATH = re.compile(r"[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*\Z")
FIELDS = frozenset({"schema_id", "schema_version", "repository", "pull_number", "head_sha",
                    "entry_id", "admission_request_id", "enqueue_request_id", "base_sha",
                    "expires_at", "authority_sha256", "producer_generation_sha256",
                    "producer_closure"})


class Refused(ValueError):
    """The available observation cannot establish acknowledged intake."""


def require(value, message):
    if not value:
        raise Refused(message)


def positive(value):
    return type(value) is int and value > 0


def node(value):
    return (isinstance(value, str) and 0 < len(value) <= 512
            and all(33 <= ord(char) <= 126 for char in value))


def decode(raw):
    require(isinstance(raw, (str, bytes)), "JSON observation is missing")
    try:
        size = len(raw.encode("utf-8")) if isinstance(raw, str) else len(raw)
    except UnicodeError as error:
        raise Refused("invalid JSON encoding") from error
    require(size <= MAX_BYTES, "JSON observation exceeds its bound")
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate JSON field")
            result[key] = value
        return result
    def constant(_):
        raise Refused("nonfinite JSON")
    def finite_float(value):
        number = float(value)
        require(math.isfinite(number), "nonfinite JSON")
        return number
    try:
        text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
        return json.loads(text, object_pairs_hook=pairs, parse_constant=constant, parse_float=finite_float)
    except (ValueError, UnicodeError) as error:
        raise Refused("malformed or ambiguous JSON") from error


@dataclass(frozen=True)
class Binding:
    repository: str
    pull_number: int
    head_sha: str
    entry_id: str
    publisher_app_id: int
    publisher_app_name: str
    publisher_check_name: str
    producer_generation_sha256: str
    expected_closure: dict[str, dict[str, str]]

    def validate(self):
        require(self.repository == "EffortlessMetrics/ripr-swarm", "wrong repository")
        require(positive(self.pull_number) and isinstance(self.head_sha, str)
                and OID.fullmatch(self.head_sha) is not None and node(self.entry_id),
                "exact PR, H and native entry are required")
        require(positive(self.publisher_app_id) and self.publisher_app_id != 15368,
                "explicit non-Actions publisher App ID required")
        for value in (self.publisher_app_name, self.publisher_check_name):
            require(isinstance(value, str) and 0 < len(value) <= 100
                    and not any(ord(char) < 32 for char in value), "explicit publisher name required")
        require(isinstance(self.producer_generation_sha256, str)
                and HEX256.fullmatch(self.producer_generation_sha256) is not None,
                "trusted target producer generation is missing")
        require(isinstance(self.expected_closure, dict) and self.expected_closure,
                "trusted target producer closure is missing")
        closure(self.expected_closure)


def safe_path(path):
    return (isinstance(path, str) and PATH.fullmatch(path) is not None
            and not any(part in {".", "..", ".git"} for part in path.split("/")))


def closure(value):
    require(isinstance(value, dict) and value, "producer closure is missing")
    require(isinstance(value.get(".github"), dict) and value[".github"].get("kind") == "tree"
            and any(path != ".github" and not path.startswith(".github/") for path in value),
            "producer closure must contain the .github tree and an additional producer root")
    for path, item in value.items():
        require(safe_path(path) and isinstance(item, dict) and set(item) == {"kind", "sha", "mode"},
                "malformed producer closure entry")
        kind, sha, mode = item["kind"], item["sha"], item["mode"]
        require(isinstance(sha, str) and OID.fullmatch(sha) is not None
                and ((kind == "tree" and mode == "040000")
                     or (kind == "blob" and mode in {"100644", "100755"})),
                "invalid producer closure object or mode")


def check_current(raw, binding):
    """Check a trusted reader's independently observed live PR and queue entry."""
    data = decode(raw)
    require(isinstance(data, dict) and data.get("repository") == binding.repository
            and type(data.get("pull_number")) is int and data["pull_number"] == binding.pull_number
            and data.get("head_sha") == binding.head_sha and data.get("entry_id") == binding.entry_id
            and data.get("head_repository") == binding.repository and data.get("base_ref") == "main"
            and data.get("state") == "OPEN" and data.get("draft") is False,
            "current PR, head, or queue entry differs")
    return data


def parse_intake(raw, binding, now):
    data = decode(raw)
    require(isinstance(data, dict) and set(data) == FIELDS
            and data["schema_id"] == "hermes.queue-intake.v2"
            and type(data["schema_version"]) is int and data["schema_version"] == 2
            and data["repository"] == binding.repository
            and type(data["pull_number"]) is int and data["pull_number"] == binding.pull_number
            and data["head_sha"] == binding.head_sha and data["entry_id"] == binding.entry_id,
            "intake schema or exact H/entry identity differs")
    require(isinstance(data["base_sha"], str) and OID.fullmatch(data["base_sha"]) is not None,
            "admission base is malformed")
    for key in ("admission_request_id", "enqueue_request_id"):
        require(isinstance(data[key], str) and NAME.fullmatch(data[key]) is not None,
                "intake request identity is malformed")
    for key in ("authority_sha256", "producer_generation_sha256"):
        require(isinstance(data[key], str) and HEX256.fullmatch(data[key]) is not None,
                "intake authority or generation digest is malformed")
    require(data["producer_generation_sha256"] == binding.producer_generation_sha256,
            "intake producer generation differs from the trusted target")
    closure(data["producer_closure"])
    require(data["producer_closure"] == binding.expected_closure,
            "intake producer closure differs from the trusted target")
    require(isinstance(data["expires_at"], str), "intake expiry is missing")
    try:
        expiry = datetime.fromisoformat(data["expires_at"].replace("Z", "+00:00"))
    except ValueError as error:
        raise Refused("intake expiry is malformed") from error
    require(expiry.tzinfo is not None and expiry.utcoffset() is not None and expiry > now,
            "intake expired or has no timezone")
    return data


def select(raw, binding, now):
    result = decode(raw)
    require(isinstance(result, dict) and type(result.get("total_count")) is int
            and isinstance(result.get("check_runs"), list)
            and result["total_count"] == len(result["check_runs"]) <= 100,
            "complete bounded H check observation is unavailable")
    matches = []
    for check in result["check_runs"]:
        require(isinstance(check, dict), "malformed check run")
        app = check.get("app")
        if (check.get("name") != binding.publisher_check_name or not isinstance(app, dict)
                or not positive(app.get("id")) or app.get("id") != binding.publisher_app_id):
            continue
        require(app.get("name") == binding.publisher_app_name, "publisher App name differs")
        output = check.get("output")
        text = output.get("text") if isinstance(output, dict) else None
        if not isinstance(text, str):
            continue  # An admission check without acknowledged enqueue is not intake.
        candidate = decode(text)
        if not isinstance(candidate, dict) or candidate.get("entry_id") != binding.entry_id:
            continue  # A prior acknowledged entry for the same H is not this entry.
        data = parse_intake(text, binding, now)
        require(positive(check.get("id")) and check.get("head_sha") == binding.head_sha
                and check.get("external_id") == f'hermes-queue:{data["admission_request_id"]}:H'
                and check.get("status") == "completed" and check.get("conclusion") == "success",
                "publisher check identity or conclusion differs")
        matches.append({"check_run_id": check["id"], "intake": data})
    require(len(matches) == 1, "unique matching H intake observation is not proven")
    return matches[0]


def observe(binding: Binding, read_checks: Callable[[str], str | bytes],
            read_current: Callable[[], str | bytes], now: Callable[[], datetime],
            between_reads: Callable[[], None]):
    """Re-read both trusted observations; no supplied record authenticates itself.

    ``read_checks`` must issue the exact path below to GitHub's authenticated
    repository API. ``read_current`` must independently observe the live PR and
    its native queue entry. Callers must not substitute files or candidate data.
    ``between_reads`` performs the trusted caller's intervening metadata
    observation; it does not turn supplied values into authenticated evidence.
    A successful return establishes structural compatibility against supplied
    observations only. It is not authentication, verified G, or merge evidence.
    """
    binding.validate()
    supplied_binding = binding
    binding = deepcopy(binding)
    path = f'/commits/{binding.head_sha}/check-runs?' + urlencode({
        "check_name": binding.publisher_check_name, "filter": "all", "per_page": 100})
    current_before = check_current(read_current(), binding)
    observations = []
    previous_time = None
    for index in range(2):
        if index == 1:
            between_reads()
        moment = now()
        require(isinstance(moment, datetime) and moment.tzinfo is not None
                and moment.utcoffset() is not None, "trusted current time is missing")
        moment = moment.astimezone(timezone.utc)
        require(previous_time is None or moment >= previous_time, "trusted clock moved backwards")
        previous_time = moment
        intake = select(read_checks(path), binding, moment)
        current = check_current(read_current(), binding)
        observations.append((current, intake))
    require(supplied_binding == binding, "trusted binding changed during observation")
    require(current_before == observations[0][0] == observations[1][0]
            and observations[0][1] == observations[1][1], "intake or current native entry changed")
    completed = now()
    require(isinstance(completed, datetime) and completed.tzinfo is not None
            and completed.utcoffset() is not None, "trusted completion time is missing")
    completed = completed.astimezone(timezone.utc)
    require(previous_time is not None and completed >= previous_time, "trusted clock moved backwards")
    # Revalidate the expiry after the final potentially slow observation.
    parse_intake(json.dumps(observations[0][1]["intake"]), binding, completed)
    require(supplied_binding == binding, "trusted binding changed during observation")
    # The check remains only an observation; the protected publisher must still
    # revalidate authority and actual G before a final success can be published.
    return {"status": "VALID_AGAINST_SUPPLIED_OBSERVATIONS", **observations[0][1]}
