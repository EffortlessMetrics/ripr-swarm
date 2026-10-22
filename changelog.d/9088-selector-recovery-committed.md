<!-- section: Fixed -->
- `ripr explain`/`ripr context` selector-miss recovery now replays the
  committed listing first when `--worktree` came from the dirty-tree
  default, instead of changing the subject to the working tree (#9088).
