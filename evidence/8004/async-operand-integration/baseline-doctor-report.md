# ripr worktree doctor

Status: fail

Checks:

- branch is not dirty main
- branch is current with origin/main
- generated badge endpoints are not dirty in ordinary work
- generated target/sample artifacts are not dirty
- broad source-of-truth diffs have an obvious work item marker

Errors:

- branch is behind origin/main by 36 commit(s); refresh before opening or updating a PR

Next actions:

- refresh the branch from `origin/main` before opening or updating a PR
- rerun `cargo xtask worktree doctor` after cleanup
