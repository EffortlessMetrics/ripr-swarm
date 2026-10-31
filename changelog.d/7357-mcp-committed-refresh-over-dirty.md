<!-- section: Fixed -->
- MCP `ripr_refresh` over a committed diff no longer serves repair cards or
  repair readiness from checkout Rust files the committed findings never saw.
  The committed findings read committed bytes while the seam inventory and
  cards read the checkout; the shared custody owner now compares the two on
  every committed refresh with findings (including an edit that lands after the
  findings ran, or a moved head), and any uncommitted or untracked Rust source
  or test file, including one outside the diff, also withholds. Withholding
  refuses `ripr_get_repair_card` (`incomplete_inventory`) and turns repair-ready
  items not ready (`worktree_source_not_committed`) with the path named when
  known. Dirty non-Rust files do not withhold. The `scope` note says
  `repair_cards: "withheld"`; commit the uncommitted Rust source or test files
  (untracked ones included) and refresh again, or refresh with source
  `worktree` (#7357).
