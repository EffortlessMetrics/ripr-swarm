<!-- section: Fixed -->
- MCP surfaces now say plainly when a gap is closed: `ripr_list_gaps` labels an
  all-already-gripped, empty default list `no test gap found` instead of
  `findings below`, the `strongly_gripped` `policy_omitted` card refusal and the
  `ripr_get_top_limitation` recovery say the change is already pinned and nothing
  remains to do, and `ripr_prepare_repair`'s `nothing_to_do` detail names the
  closed-gap success state (`isError` is unchanged by contract) (#15245).
