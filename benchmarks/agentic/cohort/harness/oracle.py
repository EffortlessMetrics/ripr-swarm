import sys, os, json, uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path, PurePosixPath
from typing import NamedTuple

from lib import *
from mutate import gen
from measurements import completed
from scoring import (MAX_TEST_BYTES, MAX_TEST_FILES, _files, _hash_files, _input_files, _run_process,
                     production_digest, select_qualified_mutants)
import run_one

PROMPT = """You are working in a small Rust crate (the current directory). Write an exhaustive, high-quality integration test suite in `tests/` for ALL the public behavior in `src/lib.rs`: every branch, boundary (both sides), error variant, overflow and edge input. Tests must pass on the current code (`cargo test --offline`). Do not modify `src/`. Think hard about inputs that distinguish subtly wrong implementations. When done reply with one sentence."""
ORACLE_ROOT = Path("/tmp/cohort-oracle")
COMPLETION_SCHEMA = 1


class CompletedReference(NamedTuple):
    """Admitted evidence and immutable test bytes, never a live directory alias."""
    receipt_json: str
    files: tuple

    @property
    def receipt(self):
        return json.loads(self.receipt_json)


def _agent_directory(task_id, k):
    if task_id not in TASKS or type(k) is not int or k < 0:
        raise ValueError("unknown reference task or agent slot")
    return Path(ORACLE_ROOT) / task_id / f"agent{k}"


def _write_record(path, record):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{uuid.uuid4().hex}.tmp")
    try:
        temporary.write_text(json.dumps(record, indent=2, allow_nan=False) + "\n", encoding="utf-8")
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def _read_record(path):
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"reference completion evidence is unavailable: {path.name}") from error
    if not isinstance(value, dict):
        raise ValueError("reference completion evidence is not an object")
    return value


def _clean_process(process):
    required = {"returncode", "timed_out", "cleanup_error", "launch_error"}
    return (isinstance(process, dict) and required <= process.keys()
            and type(process["returncode"]) is int and process["returncode"] == 0
            and process["timed_out"] is False and process["launch_error"] is False
            and process["cleanup_error"] is None)


def _successful_terminal(process, terminal):
    return (isinstance(terminal, dict) and completed(dict(
        rc=process["returncode"], result_observed=terminal.get("type") == "result",
        result_subtype=terminal.get("subtype"), is_error=terminal.get("is_error"))))


def _snapshot_files(entries):
    """Validate the serialized snapshot before any consumer filesystem work."""
    if type(entries) is not tuple:
        raise ValueError("reference snapshot must contain immutable file pairs")
    if len(entries) > MAX_TEST_FILES:
        raise ValueError("reference snapshot exceeds shared test transfer bounds")
    files, size = {}, 0
    for entry in entries:
        if type(entry) is not tuple or len(entry) != 2:
            raise ValueError("reference snapshot must contain immutable file pairs")
        name, data = entry
        if type(name) is not str or any(c in name for c in ("\\", ":", "\0")):
            raise ValueError("reference snapshot requires normalized relative tests paths")
        path = PurePosixPath(name)
        if (path.is_absolute() or path.as_posix() != name or ".." in path.parts
                or len(path.parts) < 2 or path.parts[0] != "tests"):
            raise ValueError("reference snapshot requires normalized relative tests paths")
        if type(data) is not bytes:
            raise ValueError("reference snapshot payload must be bytes")
        if name in files:
            raise ValueError("reference snapshot repeats a test path")
        size += len(data)
        if size > MAX_TEST_BYTES:
            raise ValueError("reference snapshot exceeds shared test transfer bounds")
        files[name] = data
    return files


def _admit_reference(task_id, k, record, snapshot):
    files = _snapshot_files(snapshot)
    if (not isinstance(record, dict) or type(record.get("schema_version")) is not int
            or record["schema_version"] != COMPLETION_SCHEMA or type(k) is not int or k < 0
            or record.get("status") != "completed" or record.get("task") != task_id
            or type(record.get("agent")) is not int or record["agent"] != k
            or not isinstance(record.get("model"), str) or not record["model"]
            or not isinstance(record.get("run_id"), str)
            or len(record["run_id"]) != 32
            or any(c not in "0123456789abcdef" for c in record["run_id"])):
        raise ValueError("reference completion identity is invalid")
    if (not _clean_process(record.get("process"))
            or not _successful_terminal(record["process"], record.get("terminal"))):
        raise ValueError("reference completion requires a successful terminal result")
    if record.get("production_sha256") != production_digest(TASKS[task_id]):
        raise ValueError("reference task source/configuration changed; rerun oracle.py")
    if not files or record.get("tests_sha256") != _hash_files(files):
        raise ValueError("reference test snapshot changed; rerun oracle.py")
    return CompletedReference(json.dumps(record, sort_keys=True, allow_nan=False),
                              tuple(sorted(files.items())))


def completed_agent_tests(task_id, ks):
    """Reuse only the latest completed attempt, capturing its exact bounded files."""
    references = []
    for k in ks:
        directory = _agent_directory(task_id, k)
        record = _read_record(directory / "completed.json")
        # Validate the run identifier before using it as a path component.
        run_id = record.get("run_id")
        if (not isinstance(run_id, str) or len(run_id) != 32
                or any(c not in "0123456789abcdef" for c in run_id)):
            raise ValueError("reference completion run identity is invalid")
        attempt = directory / "attempts" / run_id
        if _read_record(attempt / "result.json") != record:
            raise ValueError("reference completion differs from its retained attempt")
        files = _files(attempt / "repo", "tests", bounded=True)
        admitted = _admit_reference(task_id, k, record, tuple(files.items()))
        if _read_record(directory / "latest.json").get("run_id") != run_id:
            raise ValueError("reference completion was superseded by another attempt")
        references.append(admitted)
    if not references:
        raise ValueError("no completed reference agents selected")
    return references


def agent(task_id, k, model):
    t = TASKS[task_id]
    directory = _agent_directory(task_id, k)
    marker = directory / "completed.json"
    # Invalidate before source preparation, including a failed replacement model.
    marker.unlink(missing_ok=True)
    run_id = uuid.uuid4().hex
    attempt = directory / "attempts" / run_id
    attempt.mkdir(parents=True, exist_ok=False)
    _write_record(directory / "latest.json", dict(run_id=run_id))
    record = dict(schema_version=COMPLETION_SCHEMA, task=task_id, agent=k,
                  model=model, run_id=run_id, status="started")
    _write_record(attempt / "result.json", record)
    try:
        record["production_sha256"] = production_digest(t)
        repo = attempt / "repo"
        build_repo(t, str(repo))
        cmd = ["claude", "-p", PROMPT, "--model", model, "--output-format", "json",
               "--allowedTools", run_one.ALLOWED, "--permission-mode", "acceptEdits",
               "--strict-mcp-config", "--disable-slash-commands", "--max-turns", "50"]
        env = dict(os.environ, CARGO_TARGET_DIR=str(directory / "tgt"))
        result = _run_process(cmd, str(repo), env, 1500)
        for name in ("stdout", "stderr"):
            value = getattr(result, name, b"")
            data = value.encode("utf-8") if isinstance(value, str) else value
            if isinstance(data, bytes):
                (attempt / name).write_bytes(data)
        process = dict(returncode=result.returncode,
                       timed_out=bool(getattr(result, "cohort_timed_out", False)),
                       cleanup_error=getattr(result, "cohort_cleanup_error", None),
                       launch_error=bool(getattr(result, "cohort_launch_error", False)))
        record["process"] = process
        if not _clean_process(process):
            raise RuntimeError("reference agent did not complete cleanly")
        try:
            terminal = json.loads(result.stdout)
        except (json.JSONDecodeError, UnicodeDecodeError, TypeError) as error:
            raise RuntimeError("reference agent did not complete cleanly: invalid JSON result") from error
        if not _successful_terminal(process, terminal):
            raise RuntimeError("reference agent did not complete cleanly: successful terminal result required")
        record["terminal"] = {name: terminal.get(name) for name in ("type", "subtype", "is_error")}
        if _hash_files(_input_files(repo, t)) != record["production_sha256"]:
            raise RuntimeError("reference agent changed frozen source/configuration")
        files = _files(repo, "tests", bounded=True)
        record.update(status="completed", tests_sha256=_hash_files(files))
        admitted = _admit_reference(task_id, k, record, tuple(files.items()))
        if _read_record(directory / "latest.json").get("run_id") != run_id:
            raise RuntimeError("reference attempt was superseded; completion withheld")
        _write_record(attempt / "result.json", record)
        _write_record(marker, record)
        return admitted
    except Exception as error:
        record.update(status="failed", error=str(error))
        _write_record(attempt / "result.json", record)
        raise


def filter_mutants(task_id, references):
    t = TASKS[task_id]
    admitted = []
    for reference in references:
        if not isinstance(reference, CompletedReference):
            raise ValueError("reference filtering requires completed test snapshots, not directories")
        receipt = reference.receipt
        if not isinstance(receipt, dict):
            raise ValueError("reference completion evidence is not an object")
        admitted.append(_admit_reference(task_id, receipt.get("agent"), receipt, reference.files))
    if not admitted or len({r.receipt["agent"] for r in admitted}) != len(admitted):
        raise ValueError("reference filtering requires distinct completed agents")
    transferred = {}
    for i, reference in enumerate(admitted):
        for name, data in reference.files:
            relative = PurePosixPath(name).relative_to("tests")
            transferred[f"tests/oracle{i}_{relative.as_posix()}"] = data
    # Bound the combined payload as well as each individual reference.
    transferred = _snapshot_files(tuple(transferred.items()))
    d = Path(ORACLE_ROOT) / task_id / "filter"
    build_repo(t, str(d))
    expected_tests = _files(d, "tests", bounded=True)
    expected_tests.update(transferred)
    _snapshot_files(tuple(expected_tests.items()))
    for name, data in transferred.items():
        destination = d / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    if _files(d, "tests", bounded=True) != expected_tests:
        raise ValueError("copied reference tests differ from the admitted snapshots")
    cands = gen(t["new_src"], cap=70)
    qualified = score(dict(t, mutants=cands), str(d), str(Path(ORACLE_ROOT) / task_id / "ftgt"))
    qualified["reference_agents"] = [r.receipt for r in admitted]
    subject = qualified.get("subject") or {}
    if (subject.get("tests_sha256") != _hash_files(expected_tests)
            or subject.get("production_sha256") != production_digest(t)):
        previous = qualified.get("error")
        qualified.update(valid=False, quality_status="incomplete",
                         error=(f"{previous}; " if previous else "") +
                         "graded reference subject differs from the admitted snapshots")
    md = Path(__file__).resolve().parent.parent / "mutants"
    md.mkdir(parents=True, exist_ok=True)
    # Preserve incomplete evidence, but never replace selected mutants on refusal.
    (md / f"{task_id}.qualification.json").write_text(
        json.dumps(qualified, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    keep = select_qualified_mutants(cands, qualified)
    (md / f"{task_id}.json").write_text(json.dumps(keep) + "\n", encoding="utf-8")
    print(task_id, "candidates", len(cands), "pass/fail/pass kept", len(keep), flush=True)


if __name__ == "__main__":
    ids = sys.argv[1:] or ["calendar", "expr", "textfmt"]
    jobs = [(i, k, m) for i in ids for k, m in enumerate(["sonnet", "sonnet"])]
    with ThreadPoolExecutor(3) as ex:
        references = list(ex.map(lambda j: (j[0], agent(*j)), jobs))
    for i in ids:
        filter_mutants(i, [reference for task_id, reference in references if task_id == i])
