"""Build and grade evaluator-owned task crates; no task catalog or agent dependency."""
import os
import shutil
import subprocess
import tomllib


def sh(cmd, cwd=None, env=None, timeout=300, check=False):
    r = subprocess.run(cmd, cwd=cwd, env=env, shell=isinstance(cmd, str), capture_output=True, text=True, timeout=timeout)
    if check and r.returncode:
        raise RuntimeError(f"{cmd} failed: {r.stdout}{r.stderr}")
    return r

GIT = dict(os.environ, GIT_AUTHOR_NAME="x", GIT_AUTHOR_EMAIL="x@x", GIT_COMMITTER_NAME="x", GIT_COMMITTER_EMAIL="x@x")

def write(p, s):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8") as stream:
        stream.write(s)

def build_repo(t, dest):
    """Base commit, then a change commit with new src + weak tests."""
    if os.path.exists(dest): shutil.rmtree(dest)
    os.makedirs(dest)
    sh("git init -q -b main", dest, GIT, check=True)
    manifest = t.get("cargo_toml") or f'[package]\nname = "{t["id"]}"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n'
    # Custom real-code tasks need the same isolation as generated defaults.
    # Parse the table rather than matching comments or duplicating an existing one.
    if "workspace" not in tomllib.loads(manifest):
        manifest += "\n[workspace]\n"
    write(f"{dest}/Cargo.toml", manifest)
    write(f"{dest}/.gitignore", "target\n")
    write(f"{dest}/src/lib.rs", t["base_src"])
    sh("git add -A && git commit -q -m base", dest, GIT, check=True)
    write(f"{dest}/src/lib.rs", t["new_src"])
    for rel, content in (t.get("extra_files") or {}).items(): write(f"{dest}/{rel}", content)
    write(f"{dest}/tests/behavior.rs", t["weak_tests"])
    sh(["git", "add", "-A"], dest, GIT, check=True)
    sh(["git", "commit", "-q", "-m", t["msg"]], dest, GIT, check=True)

def cargo_test(d, target, timeout=60):
    from scoring import run_cargo_tests
    return run_cargo_tests(d, target, timeout)


def score(t, repo, target, archive_tests=None):
    from scoring import score_task
    return score_task(t, repo, target, build_repo, cargo_test, archive_tests=archive_tests)
