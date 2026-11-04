import sys, os, json, itertools
from concurrent.futures import ThreadPoolExecutor
from run_one import run
from tasks import TASKS
tasks = sys.argv[1].split(",") if sys.argv[1] != "all" else list(TASKS)
arms = sys.argv[2].split(","); models = sys.argv[3].split(","); reps = range(int(sys.argv[4]), int(sys.argv[5]))
jobs = list(itertools.product(tasks, arms, models, reps))
out = open(sys.argv[6], "a")
def go(j):
    try: r = run(*j, max_turns=int(os.environ.get("COHORT_MAX_TURNS", "40")))
    except Exception as e: r = dict(task=j[0], arm=j[1], model=j[2], rep=j[3], error=repr(e))
    out.write(json.dumps(r) + "\n"); out.flush()
    return r
with ThreadPoolExecutor(int(sys.argv[7]) if len(sys.argv) > 7 else 3) as ex:
    for r in ex.map(go, jobs):
        print(r.get("task"), r.get("arm"), r.get("model"), r.get("rep"), "killed", r.get("killed"), "/", r.get("mutants"), "valid", r.get("valid"), "turns", r.get("turns"), "err", r.get("error"), flush=True)
