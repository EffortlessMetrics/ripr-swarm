<!-- section: Internal / CI -->
- The `cli_smoke` test `check_json_completed_run_carries_neither_status_nor_error`
  runs on the owned sample workspace instead of this whole checkout. It was the
  slowest test in the suite (286 s in Test Analytics run 42954772045) and keeps
  every assertion (#11602).
