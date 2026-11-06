"""Build and grade evaluator-owned task crates; no task catalog or agent dependency."""
import os
from pathlib import Path
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
    with open(p, "w", encoding="utf-8", newline="") as stream:
        stream.write(s)

def _manifest(t):
    manifest = t.get("cargo_toml") or f'[package]\nname = "{t["id"]}"\nversion = "0.1.0"\nedition = "2021"\n\n[lib]\npath = "src/lib.rs"\n'
    if "workspace" not in tomllib.loads(manifest):
        manifest += "\n[workspace]\n"
    return manifest


def production_files(t):
    """The exact source/configuration bytes written by build_repo, before tests."""
    files = {"Cargo.toml": _manifest(t), "src/lib.rs": t["new_src"]}
    for name, content in (t.get("extra_files") or {}).items():
        path = Path(name)
        if (path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name
                or not path.parts or path.parts[0] == ".git"):
            raise ValueError(f"unsafe task path: {name}")
        files[path.as_posix()] = content
    # The builder always writes the weak suite last, including an explicit extra
    # with that name. Other tests remain outside this production-file projection.
    if "tests/behavior.rs" in files:
        files["tests/behavior.rs"] = t["weak_tests"]
    if any(not isinstance(content, str) for content in files.values()):
        raise ValueError("task source/configuration must be text")
    return files


def build_repo(t, dest):
    """Base commit, then a change commit with new src + weak tests."""
    inputs = production_files(t)
    if os.path.exists(dest): shutil.rmtree(dest)
    os.makedirs(dest)
    sh("git init -q -b main", dest, GIT, check=True)
    # Custom real-code tasks need the same isolation as generated defaults.
    # Parse the table rather than matching comments or duplicating an existing one.
    write(f"{dest}/Cargo.toml", _manifest(t))
    write(f"{dest}/.gitignore", "target\n")
    write(f"{dest}/src/lib.rs", t["base_src"])
    sh("git add -A && git commit -q -m base", dest, GIT, check=True)
    for rel, content in inputs.items(): write(f"{dest}/{rel}", content)
    write(f"{dest}/tests/behavior.rs", t["weak_tests"])
    sh(["git", "add", "-A"], dest, GIT, check=True)
    sh(["git", "commit", "-q", "-m", t["msg"]], dest, GIT, check=True)

def cargo_test(d, target, timeout=60):
    from scoring import run_cargo_tests
    return run_cargo_tests(d, target, timeout)


def score(t, repo, target, archive_tests=None):
    from scoring import score_task
    return score_task(t, repo, target, build_repo, cargo_test, archive_tests=archive_tests)
