<!-- section: Fixed -->
- DX scoreboard complete-kind admission now requires
  `analysis_outcome.outcome.limitations` to be a present array whose every
  entry has a string `kind`. Omitted, object-valued, and kind-less
  `[{}]` limitations can no longer improve a complete-work baseline; empty
  arrays and `eol_only_churn` still admit (#7276).
