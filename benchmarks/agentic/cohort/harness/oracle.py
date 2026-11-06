import sys, os, json, shutil
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from lib import *
from mutate import gen
from measurements import completed
from scoring import PROTOCOL, canonical_hash, qualified_outcome, qualified_compile_exclusion, _run_process
import run_one
PROMPT = """You are working in a small Rust crate (the current directory). Write an exhaustive, high-quality integration test suite in `tests/` for ALL the public behavior in `src/lib.rs`: every branch, boundary (both sides), error variant, overflow and edge input. Tests must pass on the current code (`cargo test --offline`). Do not modify `src/`. Think hard about inputs that distinguish subtly wrong implementations. When done reply with one sentence."""
def agent(task_id, k, model):
    t = TASKS[task_id]; d = f"/tmp/cohort-oracle/{task_id}/agent{k}"
    build_repo(t, d + "/repo")
    cmd = ["claude","-p",PROMPT,"--model",model,"--output-format","json","--allowedTools",run_one.ALLOWED,"--permission-mode","acceptEdits","--strict-mcp-config","--disable-slash-commands","--max-turns","50"]
    env = dict(os.environ, CARGO_TARGET_DIR=d+"/tgt")
    result = _run_process(cmd, d+"/repo", env, 1500)
    if (result.returncode != 0 or getattr(result, "cohort_timed_out", False)
            or getattr(result, "cohort_cleanup_error", None)
            or getattr(result, "cohort_launch_error", False)):
        raise RuntimeError("reference agent did not complete cleanly")
    try:
        terminal = json.loads(result.stdout)
    except (json.JSONDecodeError, UnicodeDecodeError, TypeError) as error:
        raise RuntimeError("reference agent did not complete cleanly: invalid JSON result") from error
    if not isinstance(terminal, dict) or not completed(dict(
            rc=result.returncode, result_observed=terminal.get("type") == "result",
            result_subtype=terminal.get("subtype"), is_error=terminal.get("is_error"))):
        raise RuntimeError("reference agent did not complete cleanly: successful terminal result required")
    return d + "/repo/tests"
def filter_mutants(task_id, test_dirs):
    t = TASKS[task_id]; d = f"/tmp/cohort-oracle/{task_id}/filter"
    build_repo(t, d)
    for i, td in enumerate(test_dirs):
        for f in os.listdir(td): shutil.copy(f"{td}/{f}", f"{d}/tests/oracle{i}_{f}")
    tgt = f"/tmp/cohort-oracle/{task_id}/ftgt"
    cands = gen(t["new_src"], cap=70)
    qualified = score(dict(t, mutants=cands), d, tgt)
    md = Path(__file__).resolve().parent.parent / "mutants"
    md.mkdir(parents=True, exist_ok=True)
    # Preserve all candidate outcomes, including excluded and unresolved cases.
    (md / f"{task_id}.qualification.json").write_text(
        json.dumps(qualified, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    keep = select_qualified_mutants(cands, qualified)
    (md / f"{task_id}.json").write_text(json.dumps(keep) + "\n", encoding="utf-8")
    print(task_id, "candidates", len(cands), "pass/fail/pass kept", len(keep), flush=True)


def select_qualified_mutants(candidates, result):
    if (result.get("score_protocol") != PROTOCOL or result.get("valid") is not True
            or result.get("edit_scope_valid") is not True or result.get("src_unchanged") is not True
            or result.get("scope_violations") != []):
        raise ValueError("reference suite did not establish a pristine pass")
    rows = result.get("mutant_results", [])
    if (not candidates or result.get("mutants") != len(candidates) or len(rows) != len(candidates)
            or any(not isinstance(r, dict) or type(r.get("id")) is not int for r in rows)
            or [r["id"] for r in rows] != list(range(len(candidates)))):
        raise ValueError("reference qualification omitted candidates")
    keep = []
    for candidate, row in zip(candidates, rows):
        if row.get("mutation_sha256") != canonical_hash(candidate):
            raise ValueError("reference candidate identity mismatch")
        outcome = qualified_outcome(row, result.get("baseline"))
        if outcome == "caught":
            keep.append(candidate)
        elif outcome == "survived" or qualified_compile_exclusion(row, result.get("baseline")):
            continue
        else:
            # A timeout, malformed mutant or process failure is
            # unresolved qualification, not a candidate silently filtered away.
            raise ValueError("reference qualification incomplete; preserve the prior mutant list")
    if not keep:
        raise ValueError("no candidate has a verified pass/fail/pass reference")
    return keep


if __name__ == "__main__":
    ids = sys.argv[1:] or ["calendar", "expr", "textfmt"]
    jobs = [(i, k, m) for i in ids for k, m in enumerate(["sonnet", "sonnet"])]
    with ThreadPoolExecutor(3) as ex: dirs = list(ex.map(lambda j: (j[0], agent(*j)), jobs))
    for i in ids: filter_mutants(i, [d for (ti, d) in dirs if ti == i])
