import os, sys, json, time, shutil, subprocess
from lib import *

SHIM = "/tmp/cohort-shim"
os.makedirs(f"{SHIM}/ripr-off", exist_ok=True)
write(f"{SHIM}/ripr-off/ripr", "#!/bin/sh\necho 'bash: ripr: command not found' >&2\nexit 127\n")
os.chmod(f"{SHIM}/ripr-off/ripr", 0o755)

BRIEF = """You are working in a small Rust crate (the current directory). The most recent git commit (`git show HEAD`) changed the behavior of the library code in `src/` and added a thin test file in `tests/`.

Your job: improve the tests so that if someone later introduced a bug into the behavior that commit changed, the test suite would fail. Add or change tests under `tests/` only; do not modify anything under `src/`. After you finish, your final test suite will be run against a set of hidden injected bugs in `src/`, and it must also pass on the current code (check with `cargo test --offline`). Work efficiently; when done, reply with one short sentence."""

ARMS = {
 "A0-none": "",
 "A1-diagnostics": "\n\nThe `ripr` tool is installed. `ripr check --base HEAD~1` prints static findings about whether the existing tests would notice a wrong version of the changed lines. Run it before you edit any test and use what it says.",
 "A2-packet-loop": "\n\nThe `ripr` tool is installed. `ripr agent brief --base HEAD~1 --json` prints a bounded brief for the changed behavior (seams, boundary values to try, assertion shapes), and its `next` field names the follow-up commands, including `ripr agent verify` to check that your edit closed the gap. Run the brief before you edit any test and use what it says.",
}

# Narrow tool grant: file tools plus only cargo test, ripr and read-only git/ls/cat in a throwaway crate.
ALLOWED = "Read,Edit,Write,Glob,Grep,Bash(cargo test:*),Bash(cargo build:*),Bash(ripr:*),Bash(git show:*),Bash(git diff:*),Bash(git log:*),Bash(ls:*),Bash(cat:*)"

def run(task_id, arm, model, rep, work="/tmp/cohort-work", timeout=900):
    t = TASKS[task_id]
    name = f"{task_id}.{arm}.{model}.r{rep}"
    d = f"{work}/{name}"; os.makedirs(work, exist_ok=True)
    build_repo(t, f"{d}/repo")
    repo = f"{d}/repo"
    env = dict(os.environ)
    if arm == "A0-none":
        env["PATH"] = f"{SHIM}/ripr-off:" + env["PATH"]
    env["CARGO_TARGET_DIR"] = f"{d}/agent-target"
    prompt = BRIEF + ARMS[arm]
    cmd = ["claude", "-p", prompt, "--model", model, "--output-format", "stream-json", "--verbose",
           "--allowedTools", ALLOWED, "--permission-mode", "acceptEdits",
           "--strict-mcp-config", "--disable-slash-commands", "--max-turns", "40"]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=timeout)
        out, rc = p.stdout, p.returncode
    except subprocess.TimeoutExpired as e:
        out, rc = (e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")), -9
    wall = time.time() - t0
    open(f"{d}/transcript.jsonl", "w").write(out)
    tool_calls = ripr_calls = 0; final = {}
    for line in out.splitlines():
        try: ev = json.loads(line)
        except Exception: continue
        if ev.get("type") == "assistant":
            for c in ev.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    tool_calls += 1
                    if c.get("name") == "Bash" and "ripr" in json.dumps(c.get("input", {})): ripr_calls += 1
        if ev.get("type") == "result": final = ev
    usage = final.get("usage", {})
    tokens = sum(usage.get(k, 0) or 0 for k in ("input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
    sc = score(t, repo, f"{d}/score-target")
    numstat = sh("git diff HEAD --numstat -- tests", repo, GIT).stdout
    rec = dict(task=task_id, arm=arm, model=model, rep=rep, rc=rc, turns=final.get("num_turns"), tool_calls=tool_calls,
               ripr_calls=ripr_calls, tokens=tokens, output_tokens=usage.get("output_tokens"), cost_usd=final.get("total_cost_usd"),
               agent_wall_s=round(wall, 1), api_ms=final.get("duration_api_ms"), is_error=final.get("is_error"),
               valid=sc["valid"], src_unchanged=sc["src_unchanged"], mutants=sc["mutants"], killed=sc["killed"],
               survived=sc["survived"], score_error=sc["error"], test_numstat=numstat.strip())
    shutil.copytree(f"{repo}/tests", f"{d}/final-tests", dirs_exist_ok=True)
    open(f"{d}/record.json", "w").write(json.dumps(rec, indent=1))
    shutil.rmtree(f"{d}/agent-target", ignore_errors=True); shutil.rmtree(f"{d}/score-target", ignore_errors=True)
    return rec

if __name__ == "__main__":
    print(json.dumps(run(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])), indent=1))
