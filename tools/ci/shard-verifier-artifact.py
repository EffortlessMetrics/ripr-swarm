"""Bind the unprivileged shard verifier to one immutable same-run artifact.

This is transport validation, not trusted finalization or proof that a newer
producer did not run without uploading. That authority remains with #14500.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import zipfile

LIMIT = 512 * 1024 * 1024
MEMBERS = {"xtask", "identity.json"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def positive(value):
    require(re.fullmatch(r"[1-9][0-9]*", str(value)) is not None, "noncanonical positive identity")
    return int(value)


def digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest()


def identity(repository, run_id, attempt, sha):
    require(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) is not None, "invalid repository")
    require(re.fullmatch(r"[0-9a-f]{40}", sha) is not None, "invalid checkout SHA")
    return dict(repository=repository, run_id=positive(run_id), run_attempt=positive(attempt), checkout_sha=sha)


def retain(binary, destination, expected):
    require(stat.S_ISREG(binary.lstat().st_mode), "producer binary must be a regular file")
    require(0 < binary.stat().st_size <= LIMIT, "producer binary size outside bound")
    destination.mkdir(mode=0o700)  # Refuse preexisting state, including symlinks.
    shutil.copyfile(binary, destination / "xtask")
    (destination / "identity.json").write_text(json.dumps({
        **expected, "binary_sha256": digest(destination / "xtask"), "schema": 1,
    }) + "\n")


class GitHub:
    def __init__(self, repository):
        self.root = f"repos/{repository}/actions"

    def api(self, suffix, output=None, paginate=False):
        args = ["gh", "api", "--hostname", "github.com", f"{self.root}/{suffix}"]
        if paginate:
            args += ["--paginate", "--slurp"]
        result = subprocess.run(args, stdout=output or subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=120)
        # gh errors can contain signed artifact redirect URLs. Never forward
        # those bearer credentials to workflow logs.
        require(result.returncode == 0, f"GitHub artifact read failed (exit {result.returncode})")
        return None if output else json.loads(result.stdout)


def select(pages, expected, prefix, run):
    require(isinstance(pages, list) and pages, "missing artifact inventory")
    require(run["id"] == expected["run_id"] and run["repository"]["full_name"] == expected["repository"], "wrong run/repository")
    require(run["run_attempt"] == expected["run_attempt"], "verification attempt is no longer current")
    artifacts = [artifact for page in pages for artifact in page["artifacts"]]
    require(all(page["total_count"] == len(artifacts) for page in pages), "incomplete or changing inventory")
    ids = [positive(a["id"]) for a in artifacts]
    require(len(set(ids)) == len(ids), "duplicate artifact ID")
    candidates = {}
    for artifact in artifacts:
        name = artifact["name"]
        if not name.startswith(prefix + "-xtask-"):
            continue
        attempt = positive(name[len(prefix + "-xtask-"):])
        require(attempt <= expected["run_attempt"], "future executable artifact")
        require(attempt not in candidates, "ambiguous executable artifact attempt")
        candidates[attempt] = artifact
    require(candidates, "no executable artifact")
    attempt = max(candidates)
    artifact = candidates[attempt]
    require(artifact["expired"] is False, "selected executable artifact expired")
    require(type(artifact["size_in_bytes"]) is int and 0 < artifact["size_in_bytes"] <= LIMIT, "artifact size outside bound")
    require(re.fullmatch(r"sha256:[0-9a-f]{64}", artifact["digest"] or "") is not None, "missing artifact digest")
    producer = artifact["workflow_run"]
    require(producer["id"] == expected["run_id"] and producer["head_sha"] == run["head_sha"]
            and producer["repository_id"] == run["repository"]["id"]
            and producer["head_repository_id"] == run["head_repository"]["id"], "wrong artifact run/source")
    return artifact, attempt


def unpack(archive, destination, artifact, expected):
    require(archive.stat().st_size <= LIMIT, "download exceeds bound")
    require("sha256:" + digest(archive) == artifact["digest"], "artifact digest mismatch")
    with zipfile.ZipFile(archive) as payload:
        members = payload.infolist()
        require(len(members) == len(MEMBERS) and {m.filename for m in members} == MEMBERS,
                "archive must contain exactly xtask and identity.json")
        for member in members:
            mode = member.external_attr >> 16
            # Unix mode may omit type bits (normal upload-artifact ZIPs do).
            require(not member.is_dir() and stat.S_IFMT(mode) in (0, stat.S_IFREG), "nonregular archive member")
            require(not member.flag_bits & 1, "encrypted archive member")
            require(0 < member.file_size <= (16384 if member.filename == "identity.json" else LIMIT), "member size outside bound")
        receipt = json.loads(payload.read("identity.json"))
        require(type(receipt) is dict and type(receipt.get("schema")) is int and receipt["schema"] == 1, "invalid executable identity schema")
        require(all(type(receipt.get(k)) is type(v) and receipt[k] == v for k, v in expected.items()), "executable producer identity mismatch")
        require(re.fullmatch(r"[0-9a-f]{64}", receipt.get("binary_sha256", "")) is not None, "missing binary digest")
        destination.mkdir(mode=0o700)  # Never extract into an old or candidate-owned tree.
        binary = destination / "xtask"
        with payload.open("xtask") as source, binary.open("xb") as output:
            shutil.copyfileobj(source, output)
        require(digest(binary) == receipt["binary_sha256"], "binary digest mismatch")
        return binary


def launch(expected, prefix, runner_temp, provenance, command, github=None):
    require(prefix and len(prefix) < 200 and re.fullmatch(r"[A-Za-z0-9_.-]+", prefix), "invalid artifact prefix")
    require(runner_temp.is_absolute() and runner_temp.is_dir() and not runner_temp.is_symlink(), "invalid runner temporary directory")
    temp_root = runner_temp.resolve()
    checkout = Path.cwd().resolve()
    require(not temp_root.is_relative_to(checkout), "runner temp must be outside checkout")
    github = github or GitHub(expected["repository"])
    run = github.api(f"runs/{expected['run_id']}")
    artifact, attempt = select(github.api(f"runs/{expected['run_id']}/artifacts?per_page=100", paginate=True), expected, prefix, run)
    artifact_id = positive(artifact["id"])
    require(github.api(f"artifacts/{artifact_id}") == artifact, "artifact metadata changed")
    with tempfile.TemporaryDirectory(prefix="ripr-shard-verifier-", dir=temp_root) as temporary:
        root = Path(temporary)
        archive = root / "payload.zip"
        with archive.open("xb") as output:
            github.api(f"artifacts/{artifact_id}/zip", output=output)
        binary = unpack(archive, root / "executable", artifact, {**expected, "run_attempt": attempt})
        current_run = github.api(f"runs/{expected['run_id']}")
        require(current_run["id"] == run["id"] and current_run["run_attempt"] == expected["run_attempt"]
                and current_run["head_sha"] == run["head_sha"], "run superseded before launch")
        provenance.parent.mkdir(parents=True, exist_ok=True)
        provenance.write_text(json.dumps({**expected, "artifact_id": artifact_id, "artifact_name": artifact["name"],
            "artifact_digest": artifact["digest"], "producer_attempt": attempt, "binary_sha256": digest(binary),
            "authority": "unprivileged-transport-only"}, sort_keys=True) + "\n")
        binary.chmod(0o500)
        child_env = {k: v for k, v in os.environ.items() if k not in {"GH_TOKEN", "GITHUB_TOKEN"}}
        return subprocess.run([str(binary), *command], env=child_env, check=False).returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["retain", "launch"])
    parser.add_argument("--repository", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--attempt", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--prefix")
    parser.add_argument("--runner-temp", type=Path)
    parser.add_argument("--provenance", type=Path)
    arguments = sys.argv[1:]
    split = arguments.index("--") if "--" in arguments else len(arguments)
    command = arguments[split + 1:]
    args = parser.parse_args(arguments[:split])
    expected = identity(args.repository, args.run_id, args.attempt, args.sha)
    if args.mode == "retain":
        require(args.binary is not None and args.destination is not None, "missing retain paths")
        retain(args.binary, args.destination, expected)
        return 0
    require(args.runner_temp is not None and args.provenance is not None, "missing launch paths")
    require(command, "missing verifier command")
    return launch(expected, args.prefix, args.runner_temp, args.provenance, command)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, KeyError, TypeError, OSError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(f"shard verifier refused: {error}", file=sys.stderr)
        sys.exit(1)
