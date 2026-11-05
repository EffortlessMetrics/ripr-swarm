"""Qualify authored reference suites without models; incomplete evidence exits nonzero."""
import json
from pathlib import Path
import sys
import tempfile

from lib import TASKS, build_repo, score, write


def validate_task(task):
    if not task.get("ref_tests"):
        return dict(status="not_applicable", reason="no authored reference suite")
    with tempfile.TemporaryDirectory(prefix="cohort-validate-") as directory:
        repo = str(Path(directory) / "repo")
        target = str(Path(directory) / "target")
        build_repo(task, repo)
        weak = score(task, repo, target)
        write(f"{repo}/tests/behavior.rs", task["ref_tests"])
        reference = score(task, repo, target)
    complete = (weak.get("quality_status") == "complete"
                and reference.get("quality_status") == "complete"
                and type(reference.get("mutants")) is int and reference["mutants"] > 0
                and reference.get("killed") == reference["mutants"])
    return dict(status="pass" if complete else "fail", weak=weak, reference=reference)


def main(args=None):
    selected = list(sys.argv[1:] if args is None else args)
    if not selected:
        selected = [name for name, task in TASKS.items() if task.get("ref_tests")]
    if not selected or len(set(selected)) != len(selected) or any(name not in TASKS for name in selected):
        print("validate: select nonempty, unique task IDs with authored references", file=sys.stderr)
        return 2
    results = {name: validate_task(TASKS[name]) for name in selected}
    print(json.dumps(results, indent=2, allow_nan=False))
    return 0 if all(result["status"] == "pass" for result in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
