<!-- section: Fixed -->
- `ripr init --agents` now requires a successful, complete analysis of the selected
  input and full finding counts before calling the static repair loop done.
  Empty scope, missing evidence, truncated output and unanalyzed saved changes
  instead stop with their limitation (#13125).
