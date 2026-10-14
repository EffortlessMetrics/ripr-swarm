<!-- section: Docs -->
- `docs/QUICKSTART.md` now matches the RIPR-SPEC-0116 dirty-tree default:
  plain `ripr check` analyzes the working tree when it has uncommitted
  tracked changes, `--worktree` forces that read, and `--committed` forces
  committed history (#7660).
