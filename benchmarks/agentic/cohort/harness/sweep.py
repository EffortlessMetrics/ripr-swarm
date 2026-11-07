"""Opt-in cohort collection. Each invocation has one explicit comparison identity."""
import itertools
import json
import os
import sys
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed

from run_one import TASKS, CONFIGS, run, run_identity


def run_job(job, max_turns, cohort_id):
    identity = dict(task=job[0], arm=job[1], model=job[2], rep=job[3],
                    max_turns=max_turns, cohort_id=cohort_id)
    try:
        identity.update(run_identity(*job, max_turns, cohort_id))
        return run(*job, max_turns=max_turns, cohort_id=cohort_id)
    except Exception as error:
        # Unknown spend remains unknown, never a zero-cost failed trial.
        return dict(identity, error=repr(error), result_observed=False,
                    tokens=None, cost_usd=None)


def main(args):
    tasks = args[0].split(",") if args[0] != "all" else list(TASKS)
    arms, models = args[1].split(","), args[2].split(",")
    reps = range(int(args[3]), int(args[4]))
    max_turns = int(os.environ.get("COHORT_MAX_TURNS", "40"))
    cohort_id = os.environ.get("COHORT_ID") or uuid.uuid4().hex
    jobs = list(itertools.product(tasks, arms, models, reps))
    if (not jobs or max_turns <= 0 or len(set(jobs)) != len(jobs)
            or any(rep < 0 for rep in reps) or any(not model.strip() for model in models)
            or not cohort_id.strip()):
        raise ValueError("need a nonempty, unique job matrix and positive turn cap")
    if any(t not in TASKS for t in tasks) or any(a not in CONFIGS for a in arms):
        raise ValueError("unknown task or config")
    with open(args[5], "a", encoding="utf-8") as out:
        with ThreadPoolExecutor(int(args[6]) if len(args) > 6 else 3) as executor:
            pending = [executor.submit(run_job, job, max_turns, cohort_id) for job in jobs]
            # One writer retains completed receipts promptly, including failures.
            for future in as_completed(pending):
                row = future.result()
                out.write(json.dumps(row, allow_nan=False) + "\n")
                out.flush()
                print(row.get("task"), row.get("arm"), row.get("model"), row.get("rep"),
                      "killed", row.get("killed"), "/", row.get("mutants"),
                      "valid", row.get("valid"), "turns", row.get("turns"),
                      "err", row.get("error"), flush=True)


if __name__ == "__main__":
    main(sys.argv[1:])
