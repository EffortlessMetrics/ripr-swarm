<!-- section: Internal / CI -->
- `check-file-policy` folds a `--lib` covered_by selector into its package's
  combined preparation build, so the hosted gates job no longer compiles the
  `ripr` lib unit-test target in a separate serial build (#13705).
