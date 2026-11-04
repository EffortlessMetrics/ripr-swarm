import os, subprocess, shutil, json, sys
from tasks import TASKS
import json as _j, os as _o
for _f in ('tasks_big', 'tasks_real'):
    try:
        BIG = __import__(_f).__dict__.get('BIG') or __import__(_f).__dict__.get('REAL')
        for _k,_t in BIG.items():
            _m=_o.path.join(_o.path.dirname(_o.path.abspath(__file__)),'..','mutants',_k+'.json')
            if _o.path.exists(_m): _t['mutants']=[tuple(x) for x in _j.load(open(_m))]
            TASKS[_k]=_t
    except ImportError: pass

def sh(cmd, cwd=None, env=None, timeout=300, check=False):
    r = subprocess.run(cmd, cwd=cwd, env=env, shell=isinstance(cmd, str), capture_output=True, text=True, timeout=timeout)
    if check and r.returncode:
        raise RuntimeError(f"{cmd} failed: {r.stdout}{r.stderr}")
    return r

GIT = dict(os.environ, GIT_AUTHOR_NAME="x", GIT_AUTHOR_EMAIL="x@x", GIT_COMMITTER_NAME="x", GIT_COMMITTER_EMAIL="x@x")

def write(p, s):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, "w").write(s)

def build_repo(t, dest):
    """Base commit, then a change commit with new src + weak tests."""
    if os.path.exists(dest): shutil.rmtree(dest)
    os.makedirs(dest)
    sh("git init -q -b main", dest, GIT, check=True)
    write(f"{dest}/Cargo.toml", t.get("cargo_toml") or f'[package]\nname = "{t["id"]}"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n')
    write(f"{dest}/.gitignore", "target\n")
    write(f"{dest}/src/lib.rs", t["base_src"])
    sh("git add -A && git commit -q -m base", dest, GIT, check=True)
    write(f"{dest}/src/lib.rs", t["new_src"])
    for rel, content in (t.get("extra_files") or {}).items(): write(f"{dest}/{rel}", content)
    write(f"{dest}/tests/behavior.rs", t["weak_tests"])
    sh(["git", "add", "-A"], dest, GIT, check=True)
    sh(["git", "commit", "-q", "-m", t["msg"]], dest, GIT, check=True)

def cargo_test(d, target, timeout=240):
    env = dict(os.environ, CARGO_TARGET_DIR=target, CARGO_TERM_COLOR="never", CARGO_INCREMENTAL="0")
    try:
        return sh("cargo test -q --offline", d, env, 60 if timeout == 240 else timeout)
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess("cargo test", 124, "", "TIMEOUT (counted as killed)")

def score(t, repo, target):
    """Hidden oracle: src must be unchanged; tests must pass on the real code; count surviving mutants."""
    res = dict(valid=False, src_unchanged=True, mutants=len(t["mutants"]), killed=0, survived=[], error=None)
    diff = sh("git diff HEAD --stat -- src", repo, GIT).stdout.strip()
    if diff:
        res["src_unchanged"] = False
        sh("git checkout HEAD -- src", repo, GIT)  # score the tests against the pristine implementation
    r = cargo_test(repo, target)
    if r.returncode:
        res["error"] = "tests fail/compile on pristine src: " + (r.stdout + r.stderr)[-400:]
        return res
    res["valid"] = True
    orig = open(f"{repo}/src/lib.rs").read()
    try:
        for i, (old, new) in enumerate(t["mutants"]):
            if isinstance(old, int):
                from mutate import apply
                msrc = apply(orig, old, new)
            else:
                assert orig.count(old) == 1, f"mutant {i} pattern count {orig.count(old)}"
                msrc = orig.replace(old, new)
            open(f"{repo}/src/lib.rs", "w").write(msrc)
            r = cargo_test(repo, target)
            out = r.stdout + r.stderr
            if r.returncode and "error[E" in out and "could not compile" in out and "tests/" not in out.split("could not compile")[0][-300:]:
                res.setdefault("mutant_compile_errors", []).append(i)  # mutant itself invalid
                continue
            if r.returncode: res["killed"] += 1
            else: res["survived"].append(i)
    finally:
        open(f"{repo}/src/lib.rs", "w").write(orig)
    return res
