import os, sys, json, time, shutil, subprocess
from lib import *
from pathlib import Path
import tempfile
import uuid
from measurements import digest, terminal_usage


BRIEF = """You are working in a small Rust crate (the current directory). The most recent git commit (`git show HEAD`) changed the behavior of the library code in `src/` and added a thin test file in `tests/`.

Your job: improve the tests so that if someone later introduced a bug into the behavior that commit changed, the test suite would fail. Add or change tests under `tests/` only; do not modify anything under `src/`. After you finish, your final test suite will be run against a set of hidden injected bugs in `src/`, and it must also pass on the current code (check with `cargo test --offline`). Work efficiently; when done, reply with one short sentence."""

# A config is one point in the ripr-usage matrix. Fields:
#   surface: none | check | brief | mcp      use: optional | mandatory
#   extra:   flags appended to the ripr command (verbosity / mode / gap cap / format)
#   max_turns: agent turn budget (small budgets create headroom)
CONFIGS = {
 "none":                  dict(surface="none"),
 "check-optional":        dict(surface="check", use="optional", extra=""),
 "check-mandatory":       dict(surface="check", use="mandatory", extra=""),
 "check-json-mandatory":  dict(surface="check", use="mandatory", extra="--json"),
 "check-fast-mandatory":  dict(surface="check", use="mandatory", extra="--mode fast"),
 "brief-optional":        dict(surface="brief", use="optional", extra=""),
 "brief-mandatory":       dict(surface="brief", use="mandatory", extra=""),
 "brief-1seam-mandatory": dict(surface="brief", use="mandatory", extra="--max-seams 1"),
 "brief-10seam-mandatory":dict(surface="brief", use="mandatory", extra="--max-seams 10"),
 "mcp-mandatory":         dict(surface="mcp", use="mandatory", extra=""),
 "mcp-base-mandatory":    dict(surface="mcp", use="mandatory", extra="base"),
}

def tool_paragraph(c):
    s = c["surface"]
    if s == "none": return ""
    use = "Run it before you edit any test and use what it says." if c["use"] == "mandatory" else "You may use it."
    if s == "check":
        cmd = f"ripr check --base HEAD~1 {c['extra']}".strip()
        return f"\n\nThe `ripr` tool is installed. `{cmd}` prints static findings about whether the existing tests would notice a wrong version of the changed lines. {use}"
    if s == "brief":
        cmd = f"ripr agent brief --base HEAD~1 --json {c['extra']}".strip()
        return f"\n\nThe `ripr` tool is installed. `{cmd}` prints a bounded brief for the changed behavior (seams, boundary values to try, assertion shapes), and its `next` field names the follow-up commands, including `ripr agent verify` to check that your edit closed the gap. {use}"
    if s == "mcp":
        hint = " Pass `base` set to `HEAD~1` when you call `ripr_refresh`." if c["extra"] == "base" else ""
        return f"\n\nA `ripr` MCP server is connected and exposes static test-exposure tools for this repository.{hint} {use}"
    raise ValueError(s)

# Narrow tool grant: file tools plus only cargo test, ripr and read-only git/ls/cat in a throwaway crate.
ALLOWED = "Read,Edit,Write,Glob,Grep,Bash(cargo test:*),Bash(cargo build:*),Bash(ripr:*),Bash(git show:*),Bash(git diff:*),Bash(git log:*),Bash(ls:*),Bash(cat:*)"

def run_identity(task_id, arm, model, rep, max_turns, cohort_id, timeout=900):
    """Bind both arms to the same task/oracle, harness and experiment budget."""
    source = Path(__file__).resolve().parent
    protocol = dict(brief=BRIEF, allowed_tools=ALLOWED, configs=CONFIGS, timeout=timeout,
                    harness={name: (source / name).read_text(encoding="utf-8")
                             for name in ("run_one.py", "lib.py", "measurements.py", "mutate.py")})
    return dict(receipt_schema_version=2, task=task_id, arm=arm, config=CONFIGS[arm],
                model=model, rep=rep, max_turns=max_turns, cohort_id=cohort_id,
                task_sha256=digest(TASKS[task_id]), protocol_sha256=digest(protocol),
                mutants=len(TASKS[task_id]["mutants"]))


def run(task_id, arm, model, rep, work="/tmp/cohort-work", timeout=900, ripr_bin=None, max_turns=40, cohort_id=None):
    t = TASKS[task_id]
    c = CONFIGS[arm]
    cohort_id = cohort_id or os.environ.get("COHORT_ID") or uuid.uuid4().hex
    identity = run_identity(task_id, arm, model, rep, max_turns, cohort_id, timeout)
    ripr_bin = ripr_bin or os.environ.get("COHORT_RIPR_BIN") or shutil.which("ripr")
    tag = os.environ.get("COHORT_TAG", "")
    name = f"{task_id}.{arm}.{model}.t{max_turns}.r{rep}{tag}"
    os.makedirs(work, exist_ok=True)
    d = tempfile.mkdtemp(prefix=name + ".", dir=work)
    build_repo(t, f"{d}/repo")
    repo = f"{d}/repo"
    env = dict(os.environ)
    if c["surface"] == "none":
        shim = f"{d}/ripr-off"
        write(f"{shim}/ripr", "#!/bin/sh\necho 'bash: ripr: command not found' >&2\nexit 127\n")
        os.chmod(f"{shim}/ripr", 0o755)
        env["PATH"] = f"{shim}:" + env["PATH"]
    else:
        os.makedirs(f"{d}/bin", exist_ok=True)
        if not os.path.exists(f"{d}/bin/ripr"): os.symlink(ripr_bin, f"{d}/bin/ripr")
        env["PATH"] = f"{d}/bin:" + env["PATH"]
    env["CARGO_TARGET_DIR"] = f"{d}/agent-target"
    prompt = BRIEF + tool_paragraph(c)
    cmd = ["claude", "-p", prompt, "--model", model, "--output-format", "stream-json", "--verbose",
           "--allowedTools", ALLOWED + (",mcp__ripr" if c["surface"] == "mcp" else ""), "--permission-mode", "acceptEdits",
           "--strict-mcp-config", "--disable-slash-commands", "--max-turns", str(max_turns)]
    if c["surface"] == "mcp":
        write(f"{d}/mcp.json", json.dumps({"mcpServers": {"ripr": {"command": ripr_bin, "args": ["mcp", "--stdio"]}}}))
        cmd += ["--mcp-config", f"{d}/mcp.json"]
    ripr_version = sh([ripr_bin, "--version"]).stdout.strip() if c["surface"] != "none" else None
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=timeout)
        out, rc = p.stdout, p.returncode
    except subprocess.TimeoutExpired as e:
        out, rc = (e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")), -9
    wall = time.time() - t0
    Path(d, "transcript.jsonl").write_text(out, encoding="utf-8")
    tool_calls = ripr_calls = 0; final = {}
    for line in out.splitlines():
        try: ev = json.loads(line)
        except json.JSONDecodeError: continue
        if not isinstance(ev, dict): continue
        if ev.get("type") == "assistant":
            for blk in ev.get("message", {}).get("content", []):
                if blk.get("type") == "tool_use":
                    tool_calls += 1
                    if "ripr" in json.dumps(blk.get("input", {})) or str(blk.get("name", "")).startswith("mcp__ripr"): ripr_calls += 1
        if ev.get("type") == "result": final = ev
    measured = terminal_usage(final)
    try:
        sc = score(t, repo, f"{d}/score-target")
    except Exception as error:
        # Scorer failure must not erase the already-incurred agent cost.
        sc = dict(valid=False, src_unchanged=None, mutants=len(t["mutants"]),
                  killed=None, survived=None, error=f"scorer exception: {error!r}")
    metadata_error = None
    try:
        numstat = sh("git diff HEAD --numstat -- tests", repo, GIT).stdout.strip()
    except Exception as error:
        numstat, metadata_error = None, repr(error)
    rec = dict(task=task_id, arm=arm, config=c, model=model, rep=rep, max_turns=max_turns, ripr_bin=ripr_bin, ripr_version=ripr_version, rc=rc, turns=final.get("num_turns"),
               tool_calls=tool_calls if measured["result_observed"] else None, observed_tool_calls=tool_calls,
               ripr_calls=ripr_calls if measured["result_observed"] else None, observed_ripr_calls=ripr_calls, output_tokens=measured["usage"]["output_tokens"],
               agent_wall_s=round(wall, 1), api_ms=final.get("duration_api_ms"), is_error=final.get("is_error"),
               valid=sc["valid"], src_unchanged=sc["src_unchanged"], mutants=sc["mutants"], killed=sc["killed"],
               survived=sc["survived"], score_error=sc["error"], test_numstat=numstat, metadata_error=metadata_error)
    rec.update(identity)
    rec.update(measured)
    try:
        shutil.copytree(f"{repo}/tests", f"{d}/final-tests", dirs_exist_ok=True)
    except OSError as error:
        rec["artifact_error"] = repr(error)
    Path(d, "record.json").write_text(json.dumps(rec, indent=1, allow_nan=False), encoding="utf-8")
    shutil.rmtree(f"{d}/agent-target", ignore_errors=True); shutil.rmtree(f"{d}/score-target", ignore_errors=True)
    return rec

if __name__ == "__main__":
    print(json.dumps(run(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])), indent=1))
