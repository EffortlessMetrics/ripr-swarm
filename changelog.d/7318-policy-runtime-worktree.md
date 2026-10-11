<!-- section: Fixed -->
- Repository policy build scripts resolve the current worktree at runtime when
  Cargo reuses compiled build scripts across checkouts, preserving absolute
  source watches and fail-closed source and compiler identity checks (#7318).
