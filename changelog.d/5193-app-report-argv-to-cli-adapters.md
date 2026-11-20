<!-- section: Changed -->
- Argument parsing and `--help` for `ripr annotations`, `pr-evidence`, `pr-summary`, `plus` and `impacted-evidence` moved from `app` into CLI adapters; the `app` use cases now take parsed options. Flags, errors and output are unchanged, and the xtask `impacted-evidence` compatibility entry moved to `ripr::cli::run_impacted_evidence_at` (#5193).
