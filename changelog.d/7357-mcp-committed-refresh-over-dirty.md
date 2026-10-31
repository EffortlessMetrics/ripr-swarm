<!-- section: Fixed -->
- MCP `ripr_refresh` over a committed diff no longer serves repair cards or
  repair readiness from checkout Rust files the committed findings never saw.
  The committed findings read committed bytes while the seam inventory and
  cards read the checkout; the shared custody owner now compares the two on
  every committed refresh with findings (including an edit that lands after the
  findings ran) and, on disagreement (a consumed file edited, staged or
  deleted, or an untracked Rust file), withholds `ripr_get_repair_card`
  (`incomplete_inventory`) and turns repair-ready items not ready
  (`worktree_source_not_committed`) with the path named. Dirty non-Rust or
  out-of-scope files do not withhold. The `scope` note says
  `repair_cards: "withheld"`; settle the checkout (commit or stash, or stage
  the untracked file) and refresh again, or refresh with source `worktree`
  (#7357).
