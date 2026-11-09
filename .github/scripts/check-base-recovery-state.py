#!/usr/bin/env python3
"""Refuse inputs outside the exact ordered base-matcher recovery contract."""

import argparse
import subprocess
import stat
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tree")
    parser.add_argument("--allow-compiler-repair", action="store_true")
    args = parser.parse_args()

    def git(*arguments):
        return subprocess.check_output(["git", "-C", args.tree, *arguments])

    # assume-unchanged and skip-worktree flags can conceal changes from status.
    flags = git("ls-files", "-v", "-z").split(b"\0")
    if any(row and (row[:1].islower() or row[:1] == b"S") for row in flags):
        raise SystemExit("base recovery refuses hidden index entries")
    state = git(
        "status", "--porcelain=v1", "-z", "--untracked-files=all",
        "--ignored=matching", "--ignore-submodules=none",
    )
    if not state:
        return
    path = "crates/ripr/src/app/test_stub.rs"
    expected = b" M " + path.encode() + b"\0"
    if args.allow_compiler_repair and state == expected:
        mode = (Path(args.tree) / path).lstat().st_mode
        if not stat.S_ISREG(mode) or mode & 0o111:
            raise SystemExit("base recovery refuses compiler repair mode changes")
        if not git("ls-tree", "HEAD", "--", path).startswith(b"100644 blob "):
            raise SystemExit("base recovery refuses unexpected compiler source mode")
        old = git("rev-parse", "HEAD:" + path).strip()
        new = git("hash-object", path).strip()
        if (old == b"41ce81981802365b31e5182b7a7a51540ff8d7be"
                and new == b"e934320980555de2d431b2aa8482a6059e9cf7f7"):
            return
    raise SystemExit("base recovery refuses changes outside the exact ordered repair")


if __name__ == "__main__":
    main()
