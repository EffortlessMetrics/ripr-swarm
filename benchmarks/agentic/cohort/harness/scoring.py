"""Independent runtime quality evidence for the opt-in agent cohort.

Only complete stock-libtest pass/fail/pass observations establish a catch. These
are execution records, not static RIPR findings and not a hostile-code sandbox.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import tempfile
import time

PROTOCOL = "cargo-libtest-pass-fail-pass-v1"
SUMMARY = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out; finished in .+$"
)
RUNNING = re.compile(r"^running (\d+) tests?$")
MAX_TEST_BYTES = 10 * 1024 * 1024
MAX_TEST_FILES = 1000


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def canonical_hash(value):
    return sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode())


def _text(value):
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""


def _run_process(command, directory, env, timeout):
    """Bound an owned process tree; a Cargo timeout must not leave its test running."""
    options = {"start_new_session": True} if os.name == "posix" else {
        "creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
    try:
        with subprocess.Popen(command, cwd=directory, env=env, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, **options) as child:
            try:
                stdout, stderr = child.communicate(timeout=max(0, timeout))
                result = subprocess.CompletedProcess(command, child.returncode, stdout or b"", stderr or b"")
                result.cohort_timed_out = False
                return result
            except subprocess.TimeoutExpired as error:
                cleanup_error = None
                try:
                    if os.name == "posix":
                        os.killpg(child.pid, signal.SIGKILL)
                    else:
                        subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"],
                                       capture_output=True, timeout=10, check=True)
                except ProcessLookupError:
                    pass
                except (OSError, subprocess.SubprocessError) as cleanup:
                    cleanup_error = str(cleanup)
                    child.kill()
                try:
                    stdout, stderr = child.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    # Do not block indefinitely on an escaped descendant's inherited pipe.
                    stdout, stderr = error.stdout, error.stderr
                    child.stdout.close()
                    child.stderr.close()
                    cleanup_error = "descendant pipe remained open after termination"
                result = subprocess.CompletedProcess(command, child.poll(), stdout or b"", stderr or b"")
                result.cohort_timed_out = True
                result.cohort_cleanup_error = cleanup_error
                return result
    except OSError as error:
        result = subprocess.CompletedProcess(command, 127, "", str(error))
        result.cohort_timed_out = False
        result.cohort_launch_error = True
        return result


def run_cargo_tests(directory, target, timeout=60):
    """Compile separately so build failures can never masquerade as test catches."""
    env = dict(os.environ, CARGO_TARGET_DIR=str(Path(target).resolve()),
               CARGO_TERM_COLOR="never", CARGO_INCREMENTAL="0")
    start = time.monotonic()
    result = None
    for phase, flags in (("compile", ["--no-run", "--message-format=json"]),
                         ("test", ["--", "--color", "never", "--test-threads=1"])):
        command = ["cargo", "test", "--offline", "--tests", *flags]
        remaining = timeout - (time.monotonic() - start)
        result = _run_process(command, directory, env, remaining)
        result.cohort_phase = phase
        result.cohort_wall_s = time.monotonic() - start
        if result.returncode or getattr(result, "cohort_timed_out", False):
            break
    return result


def execution_receipt(result):
    """Project a native process result, requiring complete nonempty libtest output."""
    stdout, stderr = _text(result.stdout), _text(result.stderr)
    stdout_raw = result.stdout if isinstance(result.stdout, bytes) else stdout.encode()
    stderr_raw = result.stderr if isinstance(result.stderr, bytes) else stderr.encode()
    phase = getattr(result, "cohort_phase", "test")
    runs = [int(m[1]) for line in stdout.splitlines() if (m := RUNNING.fullmatch(line))]
    summaries = [m.groups() for line in stdout.splitlines() if (m := SUMMARY.fullmatch(line))]
    counts = [[int(v) for v in row[1:]] for row in summaries]
    passed = sum(row[0] for row in counts)
    failed = sum(row[1] for row in counts)
    ignored = sum(row[2] for row in counts)
    complete = (bool(runs) and len(runs) == len(counts)
                and all(n == p + f + i + m for n, (p, f, i, m, _) in zip(runs, counts))
                and all((label == "ok") == (row[1] == 0) for (label, *_), row in zip(summaries, counts)))
    compiler_errors = []
    if getattr(result, "cohort_cleanup_error", None):
        status = "cleanup_error"
    elif getattr(result, "cohort_timed_out", False):
        status = "timeout"
    elif (getattr(result, "cohort_launch_error", False) or type(result.returncode) is not int
          or result.returncode < 0):
        status = "process_error"
    elif phase == "compile":
        compiler_errors = []
        for line in stdout.splitlines():
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if (isinstance(message, dict) and message.get("reason") == "compiler-message"
                    and isinstance(message.get("message"), dict) and message["message"].get("level") == "error"):
                compiler_errors.append(message)
        status = ("compile_error" if compiler_errors else "build_error") if result.returncode else "incomplete"
    elif result.returncode not in (0, 101):
        status = "process_error"
    elif not complete:
        status = "incomplete"
    elif passed + failed == 0:
        status = "zero_tests"
    elif result.returncode == 0 and failed == 0:
        status = "passed"
    elif result.returncode == 101 and failed > 0:
        status = "test_failed"
    else:
        status = "incomplete"
    return dict(status=status, phase=phase, returncode=result.returncode,
                executed_tests=passed + failed, passed_tests=passed, failed_tests=failed,
                ignored_tests=ignored, suites=len(summaries),
                wall_s=getattr(result, "cohort_wall_s", None),
                cleanup_error=getattr(result, "cohort_cleanup_error", None),
                compiler_error_count=len(compiler_errors),
                stdout_sha256=sha256(stdout_raw), stderr_sha256=sha256(stderr_raw),
                stdout_bytes=len(stdout_raw), stderr_bytes=len(stderr_raw),
                stdout_tail=stdout[-1500:], stderr_tail=stderr[-1500:])


def _files(root, relative, *, bounded=False):
    """Read only regular files, refusing symlinks even when they stay in the tree."""
    start = root / relative
    if not start.exists() and not start.is_symlink():
        return {}
    paths, result, size = [start], {}, 0
    while paths:
        path = paths.pop()
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode) or not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
            raise ValueError(f"not a regular file/directory: {path.relative_to(root)}")
        if stat.S_ISDIR(mode):
            paths.extend(sorted(path.iterdir(), reverse=True))
            continue
        size += path.stat().st_size
        if bounded and (size > MAX_TEST_BYTES or len(result) >= MAX_TEST_FILES):
            raise ValueError("test transfer exceeds 1000 files or 10 MiB")
        result[path.relative_to(root).as_posix()] = path.read_bytes()
    return result


def _input_files(root, task):
    files = {}
    for relative in ("src", "Cargo.toml", "build.rs", ".cargo", *task.get("extra_files", {})):
        rel = Path(relative)
        if rel.is_absolute() or ".." in rel.parts:
            raise ValueError(f"unsafe task path: {relative}")
        files.update(_files(root, relative))
    return files


def _hash_files(files):
    return canonical_hash({name: sha256(data) for name, data in sorted(files.items())})


def mutate_source(source, mutation):
    if not isinstance(mutation, (list, tuple)) or len(mutation) != 2:
        raise ValueError("mutant must be a two-element replacement")
    old, new = mutation
    if not isinstance(new, str):
        raise ValueError("mutant replacement must be text")
    if type(old) is int:
        lines = source.split("\n")
        if not 0 <= old < len(lines) or "\n" in new:
            raise ValueError("line mutant is out of range or spans multiple lines")
        lines[old] = new
        changed = "\n".join(lines)
    elif isinstance(old, str) and old and source.count(old) == 1:
        changed = source.replace(old, new, 1)
    else:
        raise ValueError("text mutant must select exactly one source occurrence")
    if changed == source:
        raise ValueError("mutant does not change the source")
    return changed


def _same_pass(observation, baseline):
    return (observation["status"] == "passed"
            and all(observation[key] == baseline[key] for key in ("executed_tests", "ignored_tests", "suites")))


def score_task(task, repo, target, build_repo, cargo_test, archive_tests=None):
    """Rebuild from evaluator inputs, copy only tests, and retain every mutant row."""
    mutations = task.get("mutants")
    count = len(mutations) if isinstance(mutations, (list, tuple)) else 0
    result = dict(score_protocol=PROTOCOL, quality_status="incomplete", valid=False,
                  src_unchanged=False, edit_scope_valid=False, mutants=count, killed=0,
                  survived=[], unresolved=list(range(count)), mutant_results=[],
                  baseline=None, scope_violations=[], error=None)
    if not count:
        result["error"] = "empty_oracle: need at least one declared mutant"
        return result
    source_repo = Path(repo).resolve()
    try:
        for name, text in task.get("extra_files", {}).items():
            path = Path(name)
            if path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name:
                raise ValueError(f"unsafe task path: {name}")
            if not isinstance(text, str):
                raise ValueError(f"task file is not text: {name}")
        with tempfile.TemporaryDirectory(prefix="cohort-grade-") as directory:
            clean = Path(directory) / "repo"
            build_repo(task, str(clean))
            expected = _input_files(clean, task)
            supplied = _input_files(source_repo, task)
            violations = sorted(name for name in expected.keys() | supplied.keys()
                                if expected.get(name) != supplied.get(name))
            result["scope_violations"] = violations
            result["src_unchanged"] = not any(name.startswith("src/") for name in violations)
            result["edit_scope_valid"] = not violations
            if violations:
                result["error"] = "agent changed evaluator-owned source/configuration"
                return result
            tests = _files(source_repo, "tests", bounded=True)
            if not tests:
                result["error"] = "no transferred test files"
                return result
            shutil.rmtree(clean / "tests")
            for name, data in tests.items():
                path = clean / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            if archive_tests is not None:
                archive = Path(archive_tests)
                archive.mkdir(parents=True, exist_ok=False)
                for name, data in tests.items():
                    destination = archive / Path(name).relative_to("tests")
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_bytes(data)
            result["subject"] = dict(task_sha256=canonical_hash(task),
                                     production_sha256=_hash_files(expected), tests_sha256=_hash_files(tests))

            def execute(expected_inputs):
                try:
                    observed = execution_receipt(cargo_test(str(clean), target))
                    if _input_files(clean, task) != expected_inputs or _files(clean, "tests", bounded=True) != tests:
                        observed["status"] = "input_changed"
                    return observed
                except (OSError, ValueError, subprocess.SubprocessError) as error:
                    return dict(status="process_error", error=str(error))

            baseline = execute(expected)
            result["baseline"] = baseline
            if baseline["status"] != "passed":
                result["error"] = "pristine tests did not establish a nonempty pass: " + baseline["status"]
                return result
            result["valid"] = True
            original = expected["src/lib.rs"].decode("utf-8")
            changed_path = clean / "src/lib.rs"
            seen_sources = set()
            for index, mutation in enumerate(mutations):
                row = dict(id=index, outcome="inconclusive", mutation_sha256=canonical_hash(mutation),
                           test=None, restored=None)
                result["mutant_results"].append(row)
                try:
                    changed = mutate_source(original, mutation).encode("utf-8")
                    if sha256(changed) in seen_sources:
                        raise ValueError("duplicate mutated source")
                    seen_sources.add(sha256(changed))
                except ValueError as error:
                    row.update(outcome="invalid_mutant", error=str(error))
                    continue
                changed_path.write_bytes(changed)
                try:
                    row["test"] = execute(dict(expected, **{"src/lib.rs": changed}))
                finally:
                    changed_path.write_bytes(expected["src/lib.rs"])
                row["restored"] = execute(expected)
                if not _same_pass(row["restored"], baseline):
                    row["reason"] = "restoration_not_verified"
                    # A changed fixture or failing restoration contaminates later rows.
                    for pending in range(index + 1, count):
                        result["mutant_results"].append(dict(id=pending, outcome="not_run",
                            mutation_sha256=canonical_hash(mutations[pending]), test=None, restored=None,
                            reason="previous_restoration_failed"))
                    break
                if row["test"]["status"] == "test_failed":
                    row["outcome"] = "caught"
                elif _same_pass(row["test"], baseline):
                    row["outcome"] = "survived"
                else:
                    row["reason"] = row["test"]["status"]
            result["killed"] = sum(row["outcome"] == "caught" for row in result["mutant_results"])
            result["survived"] = [row["id"] for row in result["mutant_results"] if row["outcome"] == "survived"]
            result["unresolved"] = [row["id"] for row in result["mutant_results"]
                                    if row["outcome"] not in ("caught", "survived")]
            if not result["unresolved"] and len(result["mutant_results"]) == count:
                result["quality_status"] = "complete"
            else:
                result["error"] = "incomplete mutation evidence; inspect per-mutant outcomes"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result["error"] = f"scorer infrastructure: {error}"
    return result


def tests_digest(directory):
    files = _files(Path(directory), ".", bounded=True)
    if not files:
        raise ValueError("no archived test files")
    return _hash_files({"tests/" + name: data for name, data in files.items()})


def qualified_outcome(result, baseline):
    """Validate a retained row's execution evidence, not only its outcome label."""
    if not _execution_valid(baseline, "passed") or not isinstance(result, dict):
        return None
    if (not _digest(result.get("mutation_sha256"))
            or not _execution_valid(result.get("restored"), "passed")
            or not _same_pass(result["restored"], baseline)):
        return None
    observation = result.get("test")
    if result.get("outcome") == "caught" and _execution_valid(observation, "test_failed"):
        return "caught"
    if (result.get("outcome") == "survived" and _execution_valid(observation, "passed")
            and _same_pass(observation, baseline)):
        return "survived"
    return None


def qualified_compile_exclusion(result, baseline):
    """Qualification may exclude a compiler-rejected candidate, never a timeout."""
    if (not isinstance(result, dict) or not _execution_valid(baseline, "passed")
            or not _execution_valid(result.get("restored"), "passed")
            or not _same_pass(result["restored"], baseline)):
        return False
    observation = result.get("test")
    return (isinstance(observation, dict) and observation.get("status") == "compile_error"
            and observation.get("phase") == "compile" and type(observation.get("returncode")) is int
            and observation["returncode"] == 101 and type(observation.get("compiler_error_count")) is int
            and observation["compiler_error_count"] > 0
            and _digest(observation.get("stdout_sha256")) and _digest(observation.get("stderr_sha256")))


def quality_verified(row):
    """Check supplied record consistency, not the authenticity of referenced runs."""
    score = row.get("score")
    if row.get("error") or not isinstance(score, dict) or score.get("score_protocol") != PROTOCOL:
        return False
    n = score.get("mutants")
    if (score.get("quality_status") != "complete" or score.get("valid") is not True
            or score.get("src_unchanged") is not True or score.get("edit_scope_valid") is not True
            or score.get("scope_violations") != [] or score.get("error") is not None
            or type(n) is not int or n <= 0 or score.get("unresolved") != []):
        return False
    subject = score.get("subject")
    if not isinstance(subject, dict) or any(not _digest(subject.get(k))
            for k in ("task_sha256", "production_sha256", "tests_sha256")):
        return False
    if (row.get("task_sha256") != subject["task_sha256"]
            or row.get("final_tests_sha256") != subject["tests_sha256"]
            or row.get("artifact_error") is not None):
        return False
    baseline = score.get("baseline")
    if not _execution_valid(baseline, "passed"):
        return False
    results = score.get("mutant_results")
    if not isinstance(results, list) or len(results) != n:
        return False
    ids, caught, survived = [], 0, []
    for result in results:
        if not isinstance(result, dict) or type(result.get("id")) is not int:
            return False
        ids.append(result["id"])
        outcome = qualified_outcome(result, baseline)
        if outcome == "caught":
            caught += 1
        elif outcome == "survived":
            survived.append(result["id"])
        else:
            return False
    if sorted(ids) != list(range(n)) or type(score.get("killed")) is not int:
        return False
    if any(not isinstance(value, list) or any(type(i) is not int for i in value)
           for value in (score.get("survived"), row.get("survived"))):
        return False
    if score["killed"] != caught or score.get("survived") != sorted(survived):
        return False
    return (row.get("valid") is True and row.get("src_unchanged") is True
            and row.get("score_error") is None and type(row.get("mutants")) is int
            and type(row.get("killed")) is int and row["mutants"] == n
            and row["killed"] == caught and row.get("survived") == sorted(survived))


def _digest(value):
    return isinstance(value, str) and re.fullmatch("[0-9a-f]{64}", value) is not None


def _execution_valid(value, status):
    if not isinstance(value, dict) or value.get("status") != status or value.get("phase") != "test":
        return False
    keys = ("executed_tests", "passed_tests", "failed_tests", "ignored_tests", "suites")
    if any(type(value.get(k)) is not int or value[k] < 0 for k in keys):
        return False
    if (not value["executed_tests"] or not value["suites"]
            or value["executed_tests"] != value["passed_tests"] + value["failed_tests"]
            or type(value.get("returncode")) is not int):
        return False
    return (value["returncode"] == (0 if status == "passed" else 101)
            and (value["failed_tests"] == 0 if status == "passed" else value["failed_tests"] > 0)
            and _digest(value.get("stdout_sha256")) and _digest(value.get("stderr_sha256")))
