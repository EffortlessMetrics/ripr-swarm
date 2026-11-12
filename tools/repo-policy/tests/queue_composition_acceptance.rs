//! Local component controls only: synthetic Git subjects are not native admission.
#![cfg(target_os = "linux")]

use std::process::Command;

const CONTROLS: &str = r#"
#!/usr/bin/env python3
"""Component control: final tree cannot authenticate a queue's ordered members.

Exercises the checked-in singleton verifier with synthetic local Git objects.
This does not establish native membership, protected admission, or G-to-M proof.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(sys.argv[1]).resolve()
ACTION = ROOT / ".github/actions/verify-merge-group-subject/action.yml"


def run(args, cwd, env=None, ok=True):
    child_env = dict(os.environ if env is None else env)
    for key in tuple(child_env):
        if key.startswith("GIT_"):
            del child_env[key]
    child_env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
                     GIT_TERMINAL_PROMPT="0")
    result = subprocess.run(args, cwd=cwd, env=child_env, text=True, capture_output=True, timeout=30)
    if ok and result.returncode:
        raise AssertionError(f"{args}: {result.stderr}")
    return result


def observe(repo, scratch, verifier, base, head, group, tree):
    ref = f"refs/heads/gh-readonly-queue/main/pr-42-{base}"
    event = {"action": "checks_requested", "repository": {
        "full_name": "EffortlessMetrics/ripr-swarm", "node_id": "R_fixture"},
        "merge_group": {"head_sha": group, "base_sha": base,
                        "head_ref": ref, "base_ref": "refs/heads/main"}}
    event_path, output_path = scratch / "event.json", scratch / "output.txt"
    event_path.write_text(json.dumps(event), encoding="utf-8")
    output_path.unlink(missing_ok=True)
    env = dict(os.environ, GITHUB_EVENT_NAME="merge_group",
               GITHUB_REPOSITORY="EffortlessMetrics/ripr-swarm", GITHUB_REF=ref,
               GITHUB_SHA=group, GITHUB_WORKFLOW_SHA=group, GITHUB_RUN_ID="19",
               GITHUB_RUN_ATTEMPT="2", GITHUB_EVENT_PATH=str(event_path),
               GITHUB_OUTPUT=str(output_path), ADMITTED_HEAD=head,
               ADMITTED_BASE=base, ADMITTED_SUBJECT=group, ADMITTED_TREE=tree,
               ADMITTED_GENERATION=group, ADMITTED_REQUEST="19-2")
    result = run([sys.executable, "-c", verifier], repo, env, ok=False)
    receipt = output_path.read_text(encoding="utf-8") if output_path.exists() else ""
    return result, receipt


def product_compositions(parent, verifier):
    """Each head passes; the union has its independently specified outcome."""
    cases = {
        "compatible": (
            "pub fn value() -> i32 { 1 }\npub fn extra() -> i32 { 2 }\n",
            "pub fn value() -> i32 { 1 }\npub fn extra() -> i32 { 3 }\n", "pass"),
        "compile": (
            "pub fn value() -> i64 { 1 }\n",
            "pub fn value() -> i32 { crate::left::value() }\n", "compile-fail"),
        "behavior": (
            "pub fn value() -> i32 { 2 }\n",
            "pub fn value() -> i32 { crate::left::value() }\n", "behavior-fail"),
    }
    for name, (left, right, outcome) in cases.items():
        repo = parent / name
        repo.mkdir()
        def git(*args):
            return run(["git", *args], repo).stdout.strip()
        git("init", "-b", "main")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "commit.gpgsign", "false")
        (repo / "main.rs").write_text(
            "mod left; mod right; fn main() { assert_eq!(right::value(), 1); }\n", encoding="utf-8")
        for filename in ("left.rs", "right.rs"):
            (repo / filename).write_text("pub fn value() -> i32 { 1 }\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "base")
        base = git("rev-parse", "HEAD")
        def head(branch, path, content):
            git("checkout", "-b", branch, base)
            (repo / path).write_text(content, encoding="utf-8")
            git("add", path)
            git("commit", "-m", branch)
            return git("rev-parse", "HEAD")
        a = head("a", "left.rs", left)
        b = head("b", "right.rs", right)
        binary = parent / (name + (".exe" if os.name == "nt" else "-bin"))
        def exercise(expected):
            compiled = run(["rustc", "main.rs", "-o", str(binary)], repo, ok=False)
            if expected == "compile-fail":
                assert compiled.returncode != 0 and "E0308" in compiled.stderr, compiled.stderr
                return
            assert compiled.returncode == 0, compiled.stderr
            result = run([str(binary)], repo, ok=False)
            if expected == "behavior-fail":
                assert result.returncode != 0 and "assertion" in result.stderr, result.stderr
            else:
                assert result.returncode == 0, result.stderr
        git("checkout", "a")
        exercise("pass")
        git("checkout", "b")
        exercise("pass")
        for branch, head_sha in (("a", a), ("b", b)):
            git("checkout", "-b", "singleton-" + branch, head_sha)
            git("commit", "--allow-empty", "-m", "native-shaped singleton")
            group, tree = git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")
            accepted, receipt = observe(repo, parent, verifier, base, head_sha, group, tree)
            assert accepted.returncode == 0 and "subject_json=" in receipt, accepted.stderr
        git("checkout", "-b", "combined", base)
        git("cherry-pick", a, b)
        group, tree = git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")
        for head_sha in (a, b):
            refused, receipt = observe(repo, parent, verifier, base, head_sha, group, tree)
            assert refused.returncode != 0 and not receipt
            assert "G is not the admitted singleton H composed with B" in refused.stderr
        exercise(outcome)


def main():
    source = ACTION.read_text(encoding="utf-8")
    body = source.split("      run: |\n", 1)[1]
    assert all(not line or line.startswith("        ") for line in body.splitlines())
    verifier = "\n".join(line[8:] if line else "" for line in body.splitlines())
    with tempfile.TemporaryDirectory(prefix="ripr-queue-order-") as temp:
        root = Path(temp)
        product_compositions(root, verifier)
        repo = root / "repo"
        repo.mkdir()
        def git(*args):
            return run(["git", *args], repo).stdout.strip()
        git("init", "-b", "main")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "commit.gpgsign", "false")
        (repo / "base.txt").write_text("base\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "base")
        base = git("rev-parse", "HEAD")
        git("checkout", "-b", "a")
        (repo / "a.txt").write_text("a\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "a")
        head_a = git("rev-parse", "HEAD")
        git("checkout", "-b", "b", base)
        (repo / "b.txt").write_text("b\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-m", "b")
        head_b = git("rev-parse", "HEAD")

        def compose(name, heads):
            git("checkout", "-b", name, base)
            for head in heads:
                git("cherry-pick", head)
            return git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")

        group_ab, tree_ab = compose("ab", [head_a, head_b])
        group_ba, tree_ba = compose("ba", [head_b, head_a])
        group_a, tree_a = compose("only-a", [head_a])
        # A cherry-pick can reproduce H byte-for-byte within the same second.
        # A separate empty commit makes the positive native-shaped G distinct.
        git("commit", "--allow-empty", "-m", "native-shaped singleton")
        group_a = git("rev-parse", "HEAD")
        assert group_a != head_a
        assert group_ab != group_ba, "the two ordered histories must be distinct"
        assert tree_ab == tree_ba, "commuting changes must expose the ordering ambiguity"
        assert tree_ab != tree_a, "omission must change the fixture tree"

        def verify(group, tree, branch):
            git("checkout", branch)
            return observe(repo, root, verifier, base, head_a, group, tree)

        valid, receipt = verify(group_a, tree_a, "only-a")
        assert valid.returncode == 0 and "subject_json=" in receipt, valid.stderr
        for group, tree, branch in [(group_ab, tree_ab, "ab"), (group_ba, tree_ba, "ba")]:
            refused, receipt = verify(group, tree, branch)
            assert refused.returncode != 0 and not receipt
            assert "G is not the admitted singleton H composed with B" in refused.stderr
        print("PASS: six individual heads; compatible, compile-negative, and behavior-negative compositions")
        print("PASS: real singleton accepted; both equal-tree orderings refused as singleton; omission differs")


if __name__ == "__main__":
    main()
"#;

#[test]
fn composed_changes_and_singleton_verifier_are_discriminated()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "180s", "python3", "-I", "-c"])
        .arg(CONTROLS)
        .arg(root)
        .output()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
