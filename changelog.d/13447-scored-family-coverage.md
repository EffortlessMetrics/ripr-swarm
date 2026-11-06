<!-- section: Internal / CI -->
- `cargo xtask mutation-spot-check` separates precise joins from scored-family
  coverage, retains the legacy join criterion and threshold compatibly, and
  rederives the retained receipt without a new mutation run (#13447).
