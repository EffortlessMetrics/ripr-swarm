<!-- section: Fixed -->
- `ripr mcp`: a `ripr_refresh` failure whose detail names the CLI's `--base`
  flag now says which tool argument carries it (`base`, and `source:
  "worktree"` for `--worktree`), so an agent is not sent to a flag it cannot
  pass. The docs state why a gap list item's `file` (`./src/lib.rs`) and the
  path inside its `canonical_id` (`src/lib.rs`) are spelled differently
  (#15105).
