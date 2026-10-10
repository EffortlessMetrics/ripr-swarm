<!-- section: Fixed -->
- A default `ripr check` on a dirty workspace now binds the canonical next
  action to the working tree it actually analyzed. Provenance previously
  followed the `--worktree` flag alone, so a dirty-default run could list
  working-tree findings while the action claimed committed history (#7257).
