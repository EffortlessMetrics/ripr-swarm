<!-- section: Changed -->
- `ripr check` prints one `untracked:` header line beside `head:` when untracked
  Rust files were left out of a committed-history run, naming the file count and
  the exact repair (`git add -N <paths>` then `ripr check --worktree`), so an
  agent that just added a test file no longer has to find the end-of-report note
  (#11159).
