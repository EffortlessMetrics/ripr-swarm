<!-- section: Added -->
- `ripr agent verify` JSON now carries additive `before_produced_from_dirty` / `after_produced_from_dirty` fields (emitted only as `true`) so an artifact produced from a dirty worktree keeps that fact after it is classified historical. The pair `artifact_currentness` tokens are unchanged (#3056).
