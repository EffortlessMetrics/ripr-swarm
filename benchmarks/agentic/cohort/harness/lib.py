import os, subprocess, shutil, json, sys
import importlib
from pathlib import Path
from tasks import TASKS
from mutate import gen
from scoring import canonical_hash, production_digest, select_qualified_mutants, _digest


# Admission diagnostics are not part of the task hashed by the scorer. Historical
# lists remain on disk, but cannot populate a changed or unqualified active task.
UNQUALIFIED_TASKS = {}


def load_qualified_mutants(task, path):
    path = Path(path)
    proof_path = path.with_name(path.stem + ".qualification.json")
    proof = json.loads(proof_path.read_text(encoding="utf-8"))
    if (not isinstance(proof, dict) or not isinstance(proof.get("subject"), dict)
            or not _digest(proof["subject"].get("tests_sha256"))):
        raise ValueError("qualification has no source-bound subject")
    candidates = gen(task["new_src"], cap=70)
    subject = proof["subject"]
    if subject.get("production_sha256") != production_digest(task):
        raise ValueError("qualification production source/configuration changed")
    if subject.get("task_sha256") != canonical_hash(dict(task, mutants=candidates)):
        raise ValueError("qualification candidate task changed")
    selected = select_qualified_mutants(candidates, proof)
    stored = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(stored, list) or canonical_hash(stored) != canonical_hash(selected):
        raise ValueError("stored mutant selection differs from qualified outcomes")
    return [tuple(mutation) for mutation in selected]


for _module_name, _catalog_name in (("tasks_big", "BIG"), ("tasks_real", "REAL")):
    try:
        _module = importlib.import_module(_module_name)
    except ImportError:
        continue
    UNQUALIFIED_TASKS.update(getattr(_module, "UNAVAILABLE", {}))
    for _key, _source_task in getattr(_module, _catalog_name).items():
        _task = dict(_source_task, mutants=[])
        _path = Path(__file__).resolve().parent.parent / "mutants" / f"{_key}.json"
        try:
            _task["mutants"] = load_qualified_mutants(_task, _path)
        except (OSError, ValueError, TypeError, KeyError) as _error:
            UNQUALIFIED_TASKS[_key] = f"current source-qualified mutant selection unavailable: {_error}"
        TASKS[_key] = _task


# The catalog keeps its historical imports; workspace/scoring own execution.
from workspace import GIT, build_repo, cargo_test, score, sh, write
