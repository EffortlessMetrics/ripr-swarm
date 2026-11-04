import sys, os, json, shutil, subprocess
from concurrent.futures import ThreadPoolExecutor
from lib import *
from mutate import gen, apply
import run_one
PROMPT = """You are working in a small Rust crate (the current directory). Write an exhaustive, high-quality integration test suite in `tests/` for ALL the public behavior in `src/lib.rs`: every branch, boundary (both sides), error variant, overflow and edge input. Tests must pass on the current code (`cargo test --offline`). Do not modify `src/`. Think hard about inputs that distinguish subtly wrong implementations. When done reply with one sentence."""
def agent(task_id, k, model):
    t = TASKS[task_id]; d = f"/tmp/cohort-oracle/{task_id}/agent{k}"
    build_repo(t, d + "/repo")
    cmd = ["claude","-p",PROMPT,"--model",model,"--output-format","json","--allowedTools",run_one.ALLOWED,"--permission-mode","acceptEdits","--strict-mcp-config","--disable-slash-commands","--max-turns","50"]
    env = dict(os.environ, CARGO_TARGET_DIR=d+"/tgt")
    subprocess.run(cmd, cwd=d+"/repo", env=env, capture_output=True, text=True, timeout=1500)
    return d + "/repo/tests"
def filter_mutants(task_id, test_dirs):
    t = TASKS[task_id]; d = f"/tmp/cohort-oracle/{task_id}/filter"
    build_repo(t, d)
    for i, td in enumerate(test_dirs):
        for f in os.listdir(td): shutil.copy(f"{td}/{f}", f"{d}/tests/oracle{i}_{f}")
    tgt = f"/tmp/cohort-oracle/{task_id}/ftgt"
    assert cargo_test(d, tgt).returncode == 0, "oracle suite must pass on pristine code"
    orig = open(f"{d}/src/lib.rs").read(); keep = []
    cands = gen(orig, cap=70)
    for ln, new in cands:
        open(f"{d}/src/lib.rs", "w").write(apply(orig, ln, new))
        r = cargo_test(d, tgt); out = r.stdout + r.stderr
        if r.returncode == 0: continue                      # unkilled by every oracle suite: drop (equivalent or exotic)
        if "could not compile" in out and "src/lib.rs" in out: continue  # invalid mutant
        keep.append([ln, new])
    open(f"{d}/src/lib.rs", "w").write(orig)
    md=os.path.join(os.path.dirname(os.path.abspath(__file__)),"..","mutants"); os.makedirs(md, exist_ok=True)
    json.dump(keep, open(f"{md}/{task_id}.json", "w"))
    print(task_id, "candidates", len(cands), "killable kept", len(keep), flush=True)
if __name__ == "__main__":
    ids = ["calendar", "expr", "textfmt"]
    jobs = [(i, k, m) for i in ids for k, m in enumerate(["sonnet", "sonnet"])]
    with ThreadPoolExecutor(3) as ex: dirs = list(ex.map(lambda j: (j[0], agent(*j)), jobs))
    for i in ids: filter_mutants(i, [d for (ti, d) in dirs if ti == i])
