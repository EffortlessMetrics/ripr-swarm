<!-- section: Internal / CI -->
- `cargo xtask token-cost` now counts SARIF findings including `folded_results` entries, so sarif bytes-per-finding compares with the JSON formats instead of undercounting folded same-line findings (#13309).
