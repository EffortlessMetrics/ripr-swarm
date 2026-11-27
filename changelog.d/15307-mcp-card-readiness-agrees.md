<!-- section: Fixed -->
- MCP: an item whose `probe:` finding id does not re-mint from its seam (an
  `error_variant` gap) is no longer reported `seam_not_found` by
  `ripr_list_gaps`, `ripr_get_gap` and `ripr_prepare_repair` while
  `ripr_get_repair_card` and `ripr repair` start it; the admission retries the
  readable gap id. `ripr_get_repair_card` now refuses a seam whose inline test
  target has uncommitted changes, as the shared repair gate does, instead of
  claiming `repair_ready`, and `ripr_prepare_repair`'s next step for that hold
  names committing or stashing the file (#15307).
