<!-- section: Fixed -->
- `check --format agent` drill-in lines carry exactly one `--format agent`
  before the selector for reports without an explicit suppression policy,
  preserving the existing navigation scope. Policy-bearing reports retain
  their valid human route, including saved-artifact replay; context and
  other commands keep their existing format (#13157).
