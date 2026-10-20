# ripr worktree doctor

Status: pass

Checks:

- branch is not dirty main
- branch is current with origin/main
- generated badge endpoints are not dirty in ordinary work
- generated target/sample artifacts are not dirty
- broad source-of-truth diffs have an obvious work item marker

No findings.

Next actions:

- continue with `cargo xtask precommit`
