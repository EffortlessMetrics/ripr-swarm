<!-- section: Fixed -->
- MCP `ripr_refresh` over a committed diff while tracked source or test files
  carry uncommitted edits no longer serves repair cards or repair readiness
  from live files the committed findings never saw. The committed findings read
  HEAD bytes while the seam inventory and cards read the checkout; that refresh
  now withholds readiness (`worktree_source_not_committed`) and
  `ripr_get_repair_card` (`incomplete_inventory`) with the cause named, and the
  `scope` note says `repair_cards: "withheld"`. Findings are still served;
  commit or stash the edits and refresh again, or refresh with source
  `worktree` (#7357).
