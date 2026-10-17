<!-- section: Fixed -->
- `ripr check --suppression-policy` now warns for each `test_efficiency`
  entry instead of dropping those rows with no trace. The entries still
  do not suppress check findings. Implicit finding-exposure badges omit
  that inapplicable-kind warning and keep applying TE rows through the
  dedicated matcher (#7760).
