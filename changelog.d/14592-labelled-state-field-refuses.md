<!-- section: Internal / CI -->
- PR admission now refuses a ready review whose later line is a short field label with a state-token value, such as `Status: REPAIR_REQUIRED` or `VERDICT: CHANGES_REQUESTED`, instead of reading it as narration (#14592).
