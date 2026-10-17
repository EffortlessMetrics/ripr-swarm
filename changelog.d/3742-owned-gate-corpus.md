<!-- section: Internal / CI -->
- Gate-test corpus copies now remain owned through evaluation and are removed
  after the last reader or a setup failure, preserving current-worktree fixture
  bytes and hermetic evaluation roots (#3742).
